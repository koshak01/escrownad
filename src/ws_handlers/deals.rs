//! Deal flow actions — forge ws-handler pattern (see demos.rs). EN-only toasts.
//!
//! **Authz:** every mutation checks seller/buyer party (IDOR-safe).

use forge_ws::{ActionResp, HtmlReplace};
use serde::{Deserialize, Serialize};

use crate::app_context;
use crate::models::Deal;
use crate::pages::deals::{BoardFilter, BoardScope, board_rows};

#[derive(Debug, Deserialize)]
pub struct DealSearchParams {
    #[serde(default)]
    pub q: Option<String>,
    /// all | mine | arbitration
    #[serde(default)]
    pub scope: Option<String>,
    /// offer | request | пусто = обе
    #[serde(default)]
    pub side: Option<String>,
}

#[derive(Serialize)]
struct RowsView {
    deals: Vec<crate::pages::deals::OfferRow>,
    show_status: bool,
    shown: usize,
    total: usize,
}

/// Живой поиск по доске: сервер отдаёт готовые строки таблицы.
///
/// Клиент шлёт запрос на каждый ввод символа, сервер рендерит партиал и
/// возвращает его для замены `#board-rows` + новый адрес для истории.
/// Никаких кнопок «Найти» — таблица обновляется по мере набора.
///
/// # Параметры
/// * `p` — строка поиска, набор и сторона
/// * `actor_usr_id` — текущий пользователь (для «моих» и «арбитража»)
///
/// # Возвращает
/// * `ActionResp` — замена строк таблицы и счётчика + push_url
pub async fn deals_search(
    p: DealSearchParams,
    actor_usr_id: Option<i64>,
) -> Result<ActionResp, String> {
    let scope = BoardScope::parse(p.scope.as_deref());
    let side = p
        .side
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase)
        .filter(|s| s == "offer" || s == "request")
        .unwrap_or_default();
    let query = p.q.as_deref().map(str::trim).unwrap_or("").to_string();

    let filter = BoardFilter {
        side: side.clone(),
        query: query.clone(),
        scope: scope.as_str().to_string(),
    };
    let (rows, total) = board_rows(&filter, scope, actor_usr_id).await?;

    let view = RowsView {
        shown: rows.len(),
        total,
        show_status: scope != BoardScope::All,
        deals: rows,
    };
    let html = {
        let renderer = app_context().renderer.read().await;
        renderer
            .render_partial("deals/_rows.html.tera", &view)
            .map_err(|e| format!("render rows: {e}"))?
    };

    // адрес синхронизируем, чтобы отфильтрованную доску можно было переслать
    let mut params: Vec<String> = Vec::new();
    if scope != BoardScope::All {
        params.push(format!("scope={}", scope.as_str()));
    }
    if !side.is_empty() {
        params.push(format!("side={side}"));
    }
    if !query.is_empty() {
        params.push(format!("q={}", urlencode(&query)));
    }
    let push_url = if params.is_empty() {
        "/deals/".to_string()
    } else {
        format!("/deals/?{}", params.join("&"))
    };

    let mut resp = ActionResp::replace(vec![HtmlReplace {
        selector: "#board-rows".into(),
        html,
    }]);
    resp.push_url = push_url;
    Ok(resp)
}

/// Минимальное percent-кодирование для строки поиска в адресе.
fn urlencode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for b in raw.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[derive(Debug, Deserialize)]
pub struct DealSaveParams {
    #[serde(default)]
    pub id: Option<i64>,
    /// Description (HTML from wysiwyg). No separate title field.
    #[serde(default)]
    pub del_title: Option<String>,
    /// Terms (HTML from wysiwyg).
    #[serde(default)]
    pub del_note: Option<String>,
    #[serde(default)]
    pub asset_type: Option<String>,
    #[serde(default)]
    pub resource_kind: Option<String>,
    /// Network / prefix text.
    #[serde(default)]
    pub prefix: Option<String>,
    #[serde(default)]
    pub from_org: Option<String>,
    #[serde(default)]
    pub to_org: Option<String>,
    #[serde(default)]
    pub seller_wallet: Option<String>,
    #[serde(default)]
    pub contact_email: Option<String>,
    /// FixedN<8> raw from `<fixed_n scale=8>` (not whole USDC units).
    #[serde(default)]
    pub del_amount: Option<i64>,
    /// If true after save → soft_verify + list (publish to market).
    #[serde(default)]
    pub publish: Option<String>,
    /// `offer` (sell) | `request` (buy demand).
    #[serde(default)]
    pub listing_side: Option<String>,
    /// IP meta (stored in checklist_json + mapped columns).
    #[serde(default)]
    pub geo: Option<String>,
    #[serde(default)]
    pub operator: Option<String>,
    /// CIDR size without slash, e.g. "24".
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub is_public: Option<bool>,
    /// Срок действия листинга, `YYYY-MM-DD` из `<date>`. Пусто → +31 день.
    #[serde(default)]
    pub deadline: Option<String>,
}

/// Разбирает дату из формы в конец указанных суток.
///
/// # Параметры
/// * `raw` — строка `YYYY-MM-DD` из компонента `<date>`
///
/// # Возвращает
/// * `Ok(Some(_))` — дата разобрана · `Ok(None)` — поле пустое ·
///   `Err(_)` — формат не распознан
fn parse_deadline(raw: Option<&str>) -> Result<Option<forge_core::Timestamp>, String> {
    let Some(text) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let date = chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d")
        .map_err(|_| format!("bad date format: {text} (expected YYYY-MM-DD)"))?;
    let end_of_day = date
        .and_hms_opt(23, 59, 59)
        .ok_or_else(|| "bad date".to_string())?;
    Ok(Some(forge_core::Timestamp::from_dt(end_of_day)))
}

fn require_actor(actor: Option<i64>) -> Result<i64, String> {
    actor.ok_or_else(|| "authentication required".to_string())
}

fn is_seller(deal: &Deal, actor: i64) -> bool {
    deal.seller_usr_id == Some(actor)
}

fn is_buyer(deal: &Deal, actor: i64) -> bool {
    deal.buyer_usr_id == Some(actor)
}

fn is_creator(deal: &Deal, actor: i64) -> bool {
    if deal.listing_side == "request" {
        is_buyer(deal, actor)
    } else {
        is_seller(deal, actor)
    }
}

/// Create/update market listing (offer = sell, request = buy demand).
pub async fn deals_save(p: DealSaveParams, actor_usr_id: Option<i64>) -> Result<ActionResp, String> {
    let actor = require_actor(actor_usr_id)?;
    let publish = p
        .publish
        .as_deref()
        .map(|s| s == "true" || s == "1" || s == "on")
        .unwrap_or(false);
    let side = p
        .listing_side
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| s == "request" || s == "offer")
        .unwrap_or_else(|| "offer".into());

    let mut deal = if let Some(id) = p.id.filter(|i| *i > 0) {
        let existing = app_context()
            .db
            .get_deal(id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "deal not found".to_string())?;
        if !is_creator(&existing, actor) {
            return Err("forbidden: only the creator can edit this listing".into());
        }
        if !matches!(
            existing.del_status.as_str(),
            "draft" | "verified" | "listed"
        ) {
            return Err("listing is locked at this stage".into());
        }
        existing
    } else {
        Deal {
            del_status: "draft".into(),
            asset_type: "ip".into(),
            resource_kind: "PI".into(),
            listing_side: side.clone(),
            chain_id: "monad".into(),
            del_is_enable: true,
            soft_verified: false,
            ..Default::default()
        }
    };

    // wysiwyg отдаёт HTML — чистим по белому списку, иначе его нельзя выводить
    if let Some(v) = p.del_title {
        deal.del_title = crate::sanitize::rich_text(&v);
    }
    if let Some(v) = p.del_note {
        deal.del_note = crate::sanitize::rich_text(&v);
    }
    if let Some(ts) = parse_deadline(p.deadline.as_deref())? {
        deal.deadline_ts = Some(ts);
    }
    if let Some(v) = p.asset_type {
        let t = v.trim().to_ascii_lowercase();
        // v1 product path: only IP is live
        if t != "ip" && !t.is_empty() {
            return Err("only IP listings are available for now".into());
        }
        deal.asset_type = if t.is_empty() { "ip".into() } else { t };
    }
    if let Some(v) = p.resource_kind {
        let k = v.trim().to_ascii_uppercase();
        if k == "PA" || k == "PI" {
            deal.resource_kind = k;
        }
    }
    if let Some(v) = p.prefix {
        let mut net = v.trim().to_string();
        // compose CIDR if network has no slash and size chip is set
        if !net.contains('/') {
            if let Some(sz) = p.size.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                let sz = sz.trim_start_matches('/');
                if sz.chars().all(|c| c.is_ascii_digit()) {
                    net = format!("{net}/{sz}");
                }
            }
        }
        deal.prefix = net;
    }
    // geo → from_org (display); operator → to_org
    if let Some(v) = p.geo.or(p.from_org) {
        deal.from_org = Some(v);
    }
    if let Some(v) = p.operator.or(p.to_org) {
        deal.to_org = Some(v);
    }
    if let Some(v) = p.contact_email {
        deal.contact_email = Some(v);
    }
    if let Some(raw) = p.del_amount {
        // form `<fixed_n scale=8>` already sends FixedN raw
        deal.del_amount = forge_fixed_n::FixedN::new(raw);
    }

    // structured IP meta in checklist_json
    {
        let mut meta = serde_json::Map::new();
        if let Some(ref g) = deal.from_org {
            meta.insert("geo".into(), serde_json::Value::String(g.clone()));
        }
        if let Some(ref op) = deal.to_org {
            meta.insert("operator".into(), serde_json::Value::String(op.clone()));
        }
        if let Some(sz) = p.size.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            meta.insert(
                "size".into(),
                serde_json::Value::String(sz.trim_start_matches('/').into()),
            );
        }
        if let Some(pub_) = p.is_public {
            meta.insert("public".into(), serde_json::Value::Bool(pub_));
        }
        if !meta.is_empty() {
            deal.checklist_json = Some(serde_json::Value::Object(meta).to_string());
        }
    }

    // Bind creator to offer (seller) or request (buyer) + linked wallet
    let wallet = app_context()
        .db
        .wallet_address_for_user(actor)
        .await
        .map_err(|e| e.to_string())?;
    if side == "request" || deal.listing_side == "request" {
        deal.listing_side = "request".into();
        deal.buyer_usr_id = Some(actor);
        if let Some(w) = wallet {
            deal.buyer_wallet = Some(w);
        }
    } else {
        deal.listing_side = "offer".into();
        deal.seller_usr_id = Some(actor);
        if let Some(w) = wallet {
            deal.seller_wallet = Some(w);
        }
    }

    if publish {
        deal.apply_action("publish", Some(actor), None)?;
    }

    // хэш нужен до отправки: по нему строится адрес карточки
    deal.assign_hash();
    let hash = deal.del_hash.clone();

    app_context()
        .db
        .save_deal(deal.clone())
        .await
        .map_err(|e| e.to_string())?;

    let msg = if publish {
        if deal.listing_side == "request" {
            "Request published"
        } else {
            "Offer published"
        }
    } else {
        "Listing saved"
    };
    Ok(ActionResp::redirect_with_success(
        &format!("/deals/{hash}/"),
        msg,
    ))
}

#[derive(Debug, Deserialize)]
pub struct DealActionParams {
    /// Постоянный хэш сделки — публичный идентификатор вместо id.
    pub hash: String,
    pub action: String,
    #[serde(default)]
    pub buyer_wallet: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DealFundedParams {
    pub hash: String,
}

/// Покупатель сообщает, что оплатил замок. Сервер проверяет это в цепи.
///
/// Клиенту не верим: статус `funded` ставится, только если контракт
/// подтвердил, что деньги действительно лежат в замке под этой сделкой.
/// Хэш транзакции от клиента вообще не принимаем — он ничего не доказывает.
///
/// # Параметры
/// * `p` — хэш сделки
/// * `actor_usr_id` — покупатель
///
/// # Возвращает
/// * `ActionResp` — переход на карточку сделки
pub async fn deals_funded(
    p: DealFundedParams,
    actor_usr_id: Option<i64>,
) -> Result<ActionResp, String> {
    let actor = require_actor(actor_usr_id)?;
    let mut deal = app_context()
        .db
        .get_deal_by_hash(p.hash.clone())
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "deal not found".to_string())?;

    // Настройки цепи — из константы `chain` в базе (правится в админке).
    let constants = app_context()
        .db
        .get_constants()
        .await
        .map_err(|e| e.to_string())?;
    let mut chain_map = std::collections::HashMap::new();
    for key in constants.keys() {
        if let Ok(Some(v)) = constants.get::<serde_json::Value>(key) {
            chain_map.insert(key.clone(), v);
        }
    }
    let config = crate::chain::types::ChainConfig::from_constants(&chain_map)
        .ok_or_else(|| "on-chain settlement is not configured".to_string())?;
    let reader = crate::chain::core::ChainReader::new(&config)
        .ok_or_else(|| "on-chain settlement is not configured".to_string())?;
    let (state, amount) = reader
        .deal_state(&deal.del_hash)
        .await
        .map_err(|e| format!("chain read failed: {e}"))?;

    if state != crate::chain::types::LockState::Funded {
        return Err(format!(
            "lock is not funded yet (on-chain state: {})",
            state.as_str()
        ));
    }

    let expected = crate::chain::core::usdc_units(deal.del_amount).map_err(|e| e.to_string())?;
    if amount < expected {
        return Err(format!(
            "locked amount {amount} is less than the deal price {expected}"
        ));
    }

    // покупатель становится известен только здесь — по факту оплаты
    if deal.buyer_usr_id.is_none() {
        deal.buyer_usr_id = Some(actor);
        deal.buyer_wallet = app_context()
            .db
            .wallet_address_for_user(actor)
            .await
            .map_err(|e| e.to_string())?;
    }
    deal.del_status = crate::models::deal::status::FUNDED.into();
    deal.lock_tx = Some(crate::chain::deal_id_hex(&deal.del_hash));

    app_context()
        .db
        .save_deal(deal.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(ActionResp::redirect_with_success(
        &format!("/deals/{}/", p.hash),
        "USDC locked on-chain",
    ))
}

/// Authorize action by party role.
/// - **offer** (sell): creator = seller; counterparty = buyer
/// - **request** (buy demand): creator = buyer; counterparty = seller
fn authorize_action(deal: &Deal, action: &str, actor: i64) -> Result<(), String> {
    let seller = is_seller(deal, actor);
    let buyer = is_buyer(deal, actor);
    let is_request = deal.listing_side == "request";
    match action {
        "soft_verify" | "list" | "publish" => {
            if is_creator(deal, actor) {
                Ok(())
            } else {
                Err("forbidden: creator only".into())
            }
        }
        "accept" | "start_prepare" | "mark_prepared" => {
            // After match: seller-side ops (prep). On offer creator is seller;
            // on request, seller is the one who responded.
            if seller {
                Ok(())
            } else {
                Err("forbidden: seller only".into())
            }
        }
        "request" => {
            // Counterparty expresses interest
            if is_request {
                // Demand listing: only a non-buyer (seller-side) can respond
                if buyer {
                    Err("forbidden: cannot respond to own request".into())
                } else {
                    Ok(())
                }
            } else if seller {
                Err("forbidden: seller cannot buy own offer".into())
            } else {
                Ok(())
            }
        }
        "fund" => {
            if buyer {
                Ok(())
            } else {
                Err("forbidden: buyer only".into())
            }
        }
        "confirm_intent" | "open_dispute" | "cancel" => {
            if seller || buyer {
                Ok(())
            } else {
                Err("forbidden: party only".into())
            }
        }
        _ => Err(format!("unknown action: {action}")),
    }
}

/// Lifecycle transition (want / accept / fund / prepare / confirm / …).
pub async fn deals_action(
    p: DealActionParams,
    actor_usr_id: Option<i64>,
) -> Result<ActionResp, String> {
    let actor = require_actor(actor_usr_id)?;
    let mut deal = app_context()
        .db
        .get_deal_by_hash(p.hash.clone())
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "deal not found".to_string())?;

    authorize_action(&deal, &p.action, actor)?;

    // Counterparty wallet from linkage — never trust client payload
    let mut wallet_arg = None;
    if p.action == "request" {
        let w = app_context()
            .db
            .wallet_address_for_user(actor)
            .await
            .map_err(|e| e.to_string())?;
        if w.is_none() {
            return Err("no wallet linked to this account".into());
        }
        if deal.listing_side == "request" {
            // Responding seller on a buy-request
            deal.seller_usr_id = Some(actor);
            deal.seller_wallet = w;
            wallet_arg = None;
        } else {
            wallet_arg = w;
        }
    }

    deal.apply_action(&p.action, Some(actor), wallet_arg)?;

    app_context()
        .db
        .save_deal(deal.clone())
        .await
        .map_err(|e| e.to_string())?;

    let msg = match p.action.as_str() {
        "soft_verify" => "Verification OK",
        "list" | "publish" => "Offer listed on market",
        "request" => "Buy request sent",
        "accept" => "Buyer accepted",
        "fund" => "USDC deposited (mock lock)",
        "start_prepare" => "Preparation started",
        "mark_prepared" => "Marked ready",
        "confirm_intent" => "Intent confirmed — waiting for oracle",
        "open_dispute" => "Dispute opened",
        "cancel" => "Cancelled / refunded",
        _ => "OK",
    };

    Ok(ActionResp::redirect_with_success(
        &format!("/deals/{}/", p.hash),
        msg,
    ))
}
