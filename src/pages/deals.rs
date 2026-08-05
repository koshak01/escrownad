//! Market board — exchange: offers (sell) + requests (buy demand).

use async_trait::async_trait;
use forge_core::Timestamp;
use forge_ws::{Page, RequestContext, WsError, WsResult};
use serde::Serialize;

use crate::app_context;
use crate::models::Deal;

pub const OFFER_TYPES: &[&str] = &["ip", "domain", "property", "work", "other"];

#[derive(Debug, Serialize)]
pub struct OfferRow {
    /// Постоянный хэш — адрес карточки `/deals/<hash>/`.
    del_hash: String,
    /// Публичный номер лота `ddd-ddd` (id наружу не показываем).
    deal_no: String,
    /// offer | request
    listing_side: String,
    offer_type: String,
    description: String,
    /// Сырое значение FixedN<8> — форматируется фильтром в шаблоне.
    /// Наружу цена не уезжает как f64: денежные значения только целые.
    price_raw: i64,
    listed_at: String,
    listed_for: String,
    expires_at: String,
    /// Creator verified (seller on offer, buyer on request).
    party_verified: bool,
    del_status: String,
}

/// Набор сделок на доске.
///
/// - `all` — рынок: только активные предложения (`listed`, срок не вышел);
/// - `mine` — созданные мной или где я сторона, в любом статусе;
/// - `arbitration` — споры, где я сторона.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardScope {
    All,
    Mine,
    Arbitration,
}

impl BoardScope {
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim).unwrap_or("") {
            "mine" => Self::Mine,
            "arbitration" => Self::Arbitration,
            _ => Self::All,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Mine => "mine",
            Self::Arbitration => "arbitration",
        }
    }

    /// В своих наборах статус сделки виден — там он несёт смысл (стадия).
    /// На общей доске все строки активные, колонка статуса бессмысленна.
    fn shows_status(self) -> bool {
        self != Self::All
    }
}

/// Параметры выборки доски — одни и те же для страницы и для живого поиска.
#[derive(Debug, Default, Serialize)]
pub struct BoardFilter {
    pub side: String,
    pub query: String,
    pub scope: String,
}

impl BoardFilter {
    /// Собирает фильтр из query-параметров страницы.
    fn from_query(ctx: &RequestContext) -> Self {
        let side = ctx
            .query
            .get("side")
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| s == "offer" || s == "request")
            .unwrap_or_default();
        Self {
            side,
            query: ctx
                .query
                .get("q")
                .map(|s| s.trim().to_string())
                .unwrap_or_default(),
            scope: BoardScope::parse(ctx.query.get("scope").map(|s| s.as_str()))
                .as_str()
                .to_string(),
        }
    }

    /// Строка подходит под фильтр (сторона + текстовый поиск).
    fn matches(&self, deal: &Deal) -> bool {
        if !self.side.is_empty() && deal.listing_side != self.side {
            return false;
        }
        if !self.query.is_empty() {
            let needle = self.query.to_lowercase();
            let haystack = format!(
                "{} {} {} {}",
                deal.prefix,
                crate::sanitize::plain_text(deal.del_title.as_deref().unwrap_or("")),
                deal.from_org.as_deref().unwrap_or(""),
                deal.to_org.as_deref().unwrap_or("")
            )
            .to_lowercase();
            if !haystack.contains(&needle) {
                return false;
            }
        }
        true
    }
}

/// Сделка входит в выбранный набор.
fn in_scope(deal: &Deal, scope: BoardScope, actor: Option<i64>) -> bool {
    let is_party = actor.is_some() && (deal.seller_usr_id == actor || deal.buyer_usr_id == actor);
    match scope {
        // Рынок: только живые предложения. Черновики, сделки в работе и
        // завершённые на общей доске не висят.
        BoardScope::All => deal.del_status == "listed" && !deal.is_expired(),
        BoardScope::Mine => is_party,
        BoardScope::Arbitration => is_party && deal.del_status == "dispute",
    }
}

/// Собирает строки доски — общий путь для страницы и для живого поиска.
///
/// # Параметры
/// * `filter` — сторона и строка поиска
/// * `scope` — какой набор показывать
/// * `actor` — текущий пользователь (для «моих» и «арбитража»)
///
/// # Возвращает
/// * `(строки, всего в наборе)`
pub async fn board_rows(
    filter: &BoardFilter,
    scope: BoardScope,
    actor: Option<i64>,
) -> Result<(Vec<OfferRow>, usize), String> {
    let deals = app_context()
        .db
        .list_deals_board()
        .await
        .map_err(|e| format!("list_deals_board: {e}"))?;
    let verified_users = app_context()
        .db
        .list_verified_sellers()
        .await
        .unwrap_or_default();

    let scoped: Vec<&Deal> = deals.iter().filter(|d| in_scope(d, scope, actor)).collect();
    let total = scoped.len();
    let rows = scoped
        .into_iter()
        .filter(|d| filter.matches(d))
        .map(|d| to_offer_row(d, &verified_users))
        .collect();
    Ok((rows, total))
}

fn listed_for(from: Timestamp) -> String {
    let now = Timestamp::now().raw();
    let then = from.raw();
    let secs = (now - then).max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

/// Plain one-line text for market board (wysiwyg stores HTML).
fn strip_html_one_line(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let t = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() {
        "—".into()
    } else {
        t
    }
}

fn description(d: &Deal) -> String {
    d.del_title
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| d.del_note.as_deref().filter(|s| !s.trim().is_empty()))
        .map(strip_html_one_line)
        .unwrap_or_else(|| "—".into())
}

/// Дата для таблицы и карточки — короткий вид `2026-08-05`.
fn date_short(ts: Timestamp) -> String {
    ts.format("%Y-%m-%d")
}

/// Дата с временем — для карточки сделки.
fn date_full(ts: Timestamp) -> String {
    ts.format("%Y-%m-%d %H:%M")
}

fn to_offer_row(d: &Deal, verified_users: &[i64]) -> OfferRow {
    let creator = if d.listing_side == "request" {
        d.buyer_usr_id
    } else {
        d.seller_usr_id
    };
    let party_verified = creator
        .map(|id| verified_users.contains(&id))
        .unwrap_or(false);
    OfferRow {
        del_hash: d.del_hash.clone(),
        deal_no: d.public_no(),
        listing_side: if d.listing_side.is_empty() {
            "offer".into()
        } else {
            d.listing_side.clone()
        },
        offer_type: d.asset_type.clone(),
        description: description(d),
        price_raw: d.del_amount.raw(),
        listed_at: date_short(d.del_dat),
        listed_for: listed_for(d.del_dat),
        expires_at: d.deadline_ts.map(date_short).unwrap_or_else(|| "—".into()),
        party_verified,
        del_status: d.del_status.clone(),
    }
}

pub struct DealsListPage;

#[async_trait]
impl Page for DealsListPage {
    fn path(&self) -> &'static str {
        "/deals/"
    }
    fn template(&self) -> &'static str {
        "deals/list.html.tera"
    }
    async fn load(&self, ctx: &mut RequestContext) -> WsResult<()> {
        let filter = BoardFilter::from_query(ctx);
        let scope = BoardScope::parse(Some(filter.scope.as_str()));
        let actor = ctx.user.as_ref().map(|u| u.usr_id);

        let (rows, total) = board_rows(&filter, scope, actor)
            .await
            .map_err(WsError::PageLoad)?;

        ctx.insert("shown", &rows.len());
        ctx.insert("total", &total);
        ctx.insert("deals", &rows);
        ctx.insert("filter", &filter);
        ctx.insert("scope", &scope.as_str());
        ctx.insert("show_status", &scope.shows_status());
        ctx.insert("logged_in", &actor.is_some());
        Ok(())
    }
}

pub struct DealNewPage;

#[async_trait]
impl Page for DealNewPage {
    fn path(&self) -> &'static str {
        "/deals/new/"
    }
    fn template(&self) -> &'static str {
        "deals/new.html.tera"
    }
    async fn load(&self, ctx: &mut RequestContext) -> WsResult<()> {
        ctx.insert("user", &ctx.user.is_some());
        ctx.insert("offer_types", &OFFER_TYPES);
        // срок действия предложения по умолчанию — +31 день
        let default_deadline =
            Timestamp(Timestamp::now().raw() + crate::models::deal::DEFAULT_LISTING_DAYS * 86_400);
        ctx.insert("default_deadline", &date_short(default_deadline));
        // default side from query ?side=request
        let side = ctx
            .query
            .get("side")
            .map(|s| s.as_str())
            .unwrap_or("offer");
        let side = if side == "request" { "request" } else { "offer" };
        ctx.insert("listing_side", &side);
        Ok(())
    }
}

/// Всё, что нужно кошельку покупателя, чтобы оплатить замок.
#[derive(Debug, Serialize)]
struct ChainParams {
    chain_id: u64,
    /// hex chain id для `wallet_switchEthereumChain`
    chain_id_hex: String,
    rpc_url: String,
    usdc: String,
    lock: String,
    deal_id: String,
    condition_hash: String,
    seller: String,
    /// Сумма в базовых единицах USDC, строкой — JS теряет точность на больших числах.
    amount_units: String,
    deadline: u64,
}

/// Собирает параметры оплаты. `None`, если цепь не настроена или сделке
/// нечего оплачивать (нет продавца, суммы или срока).
fn chain_params(deal: &Deal) -> Option<ChainParams> {
    if !crate::chain::types::ChainMode::from_env().is_live() {
        return None;
    }
    let seller = deal.seller_wallet.clone().filter(|s| !s.is_empty())?;
    let amount = crate::chain::core::usdc_units(deal.del_amount).ok()?;
    if amount.is_zero() {
        return None;
    }
    let deadline = deal.deadline_ts.map(|d| d.raw()).filter(|d| *d > 0)? as u64;
    let chain_id: u64 = std::env::var("MONAD_CHAIN_ID").ok()?.trim().parse().ok()?;

    Some(ChainParams {
        chain_id,
        chain_id_hex: format!("0x{chain_id:x}"),
        rpc_url: std::env::var("MONAD_RPC").ok()?,
        usdc: std::env::var("USDC_ADDRESS").ok()?,
        lock: std::env::var("ESCROW_LOCK_ADDRESS").ok()?,
        deal_id: crate::chain::core::deal_id(&deal.del_hash).to_string(),
        condition_hash: crate::chain::core::condition_hash(
            &deal.prefix,
            &deal.resource_kind,
            deal.from_org.as_deref().unwrap_or(""),
            deal.to_org.as_deref().unwrap_or(""),
        )
        .to_string(),
        seller,
        amount_units: amount.to_string(),
        deadline,
    })
}

pub struct DealShowPage;

#[async_trait]
impl Page for DealShowPage {
    fn path(&self) -> &'static str {
        "/deals/{hash}/"
    }
    fn template(&self) -> &'static str {
        "deals/show.html.tera"
    }
    async fn load(&self, ctx: &mut RequestContext) -> WsResult<()> {
        let hash = ctx
            .path_params
            .get("hash")
            .filter(|s| !s.is_empty())
            .cloned()
            .ok_or_else(|| WsError::PageLoad("invalid deal hash".into()))?;
        let deal = app_context()
            .db
            .get_deal_by_hash(hash)
            .await
            .map_err(|e| WsError::PageLoad(format!("get_deal_by_hash: {e}")))?
            .ok_or_else(|| WsError::NotFound("deal not found".into()))?;

        let verified_users = app_context()
            .db
            .list_verified_sellers()
            .await
            .unwrap_or_default();
        let creator = if deal.listing_side == "request" {
            deal.buyer_usr_id
        } else {
            deal.seller_usr_id
        };
        let party_verified = creator
            .map(|id| verified_users.contains(&id))
            .unwrap_or(false);

        // Роли актора — по ним шаблон показывает только СВОИ кнопки.
        // Правила обязаны совпадать с `ws_handlers::deals::authorize_action`.
        let actor = ctx.user.as_ref().map(|u| u.usr_id);
        let is_seller = actor.is_some() && actor == deal.seller_usr_id;
        let is_buyer = actor.is_some() && actor == deal.buyer_usr_id;
        let is_creator = if deal.listing_side == "request" {
            is_buyer
        } else {
            is_seller
        };

        ctx.insert("deal_no", &deal.public_no());
        // цена уходит в шаблон сырым FixedN — форматирует фильтр, не f64
        ctx.insert("price_raw", &deal.del_amount.raw());

        // Параметры для кошелька покупателя: approve + fund делает браузер.
        // Суммы отдаём строкой — в JS числа больше 2^53 теряют точность.
        let chain = chain_params(&deal);
        ctx.insert("chain_ready", &chain.is_some());
        if let Some(c) = chain {
            ctx.insert("chain", &c);
        }
        ctx.insert("created_at", &date_full(deal.del_dat));
        ctx.insert(
            "expires_at",
            &deal
                .deadline_ts
                .map(date_full)
                .unwrap_or_else(|| "—".into()),
        );
        ctx.insert("is_expired", &deal.is_expired());
        ctx.insert("is_seller", &is_seller);
        ctx.insert("is_buyer", &is_buyer);
        ctx.insert("is_creator", &is_creator);
        ctx.insert("is_party", &(is_seller || is_buyer));
        ctx.insert("usr_id", &actor);
        ctx.insert("seller_verified", &party_verified);
        ctx.insert("party_verified", &party_verified);
        ctx.insert("listed_for", &listed_for(deal.del_dat));
        ctx.insert("deal", &deal);
        Ok(())
    }
}
