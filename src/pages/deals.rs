//! Market board — exchange: offers (sell) + requests (buy demand).

use async_trait::async_trait;
use forge_core::Timestamp;
use forge_ws::{Page, RequestContext, WsError, WsResult};
use serde::Serialize;

use crate::app_context;
use crate::models::Deal;

pub const OFFER_TYPES: &[&str] = &["ip", "domain", "property", "work", "other"];

/// Статусы, по которым фильтруется доска (порядок = ход сделки).
pub const BOARD_STATUSES: &[&str] = &[
    "listed",
    "requested",
    "accepted",
    "funded",
    "preparing",
    "prepared",
    "awaiting_proof",
    "released",
    "refunded",
    "dispute",
];

#[derive(Debug, Serialize)]
struct OfferRow {
    /// Постоянный хэш — адрес карточки `/deals/<hash>/`.
    del_hash: String,
    /// Публичный номер лота `ddd-ddd` (id наружу не показываем).
    deal_no: String,
    /// offer | request
    listing_side: String,
    offer_type: String,
    description: String,
    total_price: String,
    listed_at: String,
    listed_for: String,
    expires_at: String,
    /// Creator verified (seller on offer, buyer on request).
    party_verified: bool,
    del_status: String,
}

/// Значения фильтра доски — приходят в query, возвращаются в шаблон,
/// чтобы отрисовать выбранное состояние чипов.
#[derive(Debug, Default, Serialize)]
struct BoardFilter {
    side: String,
    asset_type: String,
    status: String,
    query: String,
}

impl BoardFilter {
    /// Собирает фильтр из query-параметров страницы.
    fn from_query(ctx: &RequestContext) -> Self {
        let get = |key: &str| {
            ctx.query
                .get(key)
                .map(|s| s.trim().to_ascii_lowercase())
                .filter(|s| !s.is_empty() && s != "all")
                .unwrap_or_default()
        };
        Self {
            side: get("side"),
            asset_type: get("type"),
            status: get("status"),
            query: ctx
                .query
                .get("q")
                .map(|s| s.trim().to_string())
                .unwrap_or_default(),
        }
    }

    /// Строка подходит под фильтр.
    fn matches(&self, deal: &Deal) -> bool {
        if !self.side.is_empty() && deal.listing_side != self.side {
            return false;
        }
        if !self.asset_type.is_empty() && deal.asset_type != self.asset_type {
            return false;
        }
        if !self.status.is_empty() && deal.del_status != self.status {
            return false;
        }
        if !self.query.is_empty() {
            let needle = self.query.to_lowercase();
            let haystack = format!(
                "{} {} {}",
                deal.prefix,
                deal.del_title.as_deref().unwrap_or(""),
                deal.from_org.as_deref().unwrap_or("")
            )
            .to_lowercase();
            if !haystack.contains(&needle) {
                return false;
            }
        }
        true
    }
}

fn format_usdc(raw: i64) -> String {
    let scale = 100_000_000i64;
    let neg = raw < 0;
    let a = raw.unsigned_abs() as i128;
    let whole = a / scale as i128;
    let frac = a % scale as i128;
    let s = if frac == 0 {
        format!("{whole}")
    } else {
        let mut f = format!("{frac:08}");
        while f.ends_with('0') {
            f.pop();
        }
        format!("{whole}.{f}")
    };
    if neg {
        format!("-{s}")
    } else {
        s
    }
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
        total_price: format_usdc(d.del_amount.raw()),
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

        let deals = app_context()
            .db
            .list_deals_board()
            .await
            .map_err(|e| WsError::PageLoad(format!("list_deals_board: {e}")))?;

        // Verified party = completed released deal as seller or buyer
        let verified_users = app_context()
            .db
            .list_verified_sellers()
            .await
            .unwrap_or_default();

        let visible: Vec<&Deal> = deals
            .iter()
            .filter(|d| !matches!(d.del_status.as_str(), "draft" | "verified"))
            // просроченные предложения с доски уходят — рынок не держит
            // вечные заявки
            .filter(|d| !(d.del_status == "listed" && d.is_expired()))
            .collect();
        let total = visible.len();

        let rows: Vec<OfferRow> = visible
            .into_iter()
            .filter(|d| filter.matches(d))
            .map(|d| to_offer_row(d, &verified_users))
            .collect();

        ctx.insert("shown", &rows.len());
        ctx.insert("total", &total);
        ctx.insert("deals", &rows);
        ctx.insert("filter", &filter);
        ctx.insert("offer_types", &OFFER_TYPES);
        ctx.insert("statuses", &BOARD_STATUSES);
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
        ctx.insert("price", &format_usdc(deal.del_amount.raw()));
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
