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
    /// offer | request | empty = both
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

/// Live search on the board: the server returns ready-made table rows.
///
/// The client sends a request on every keystroke; the server renders a partial
/// and returns it to replace `#board-rows`, plus a new URL for history. There
/// is no "Search" button — the table updates as you type.
///
/// # Parameters
/// * `p` — the query string, the set and the side
/// * `actor_usr_id` — the current user (for "mine" and "arbitration")
///
/// # Returns
/// * `ActionResp` — replacement rows and counter, plus push_url
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

    // keep the URL in sync so a filtered board can be shared as a link
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

/// Minimal percent-encoding for the query string in a URL.
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
    /// Where the block is — display only; takes no part in matching the fact.
    #[serde(default)]
    pub geo: Option<String>,
    /// Regional registry: RIPE | ARIN | APNIC | LACNIC | AFRINIC.
    #[serde(default)]
    pub rir: Option<String>,
    /// CIDR size without slash, e.g. "24".
    #[serde(default)]
    pub size: Option<String>,
    #[serde(default)]
    pub is_public: Option<bool>,
    /// Listing expiry, `YYYY-MM-DD` from `<date>`. Empty → +31 days.
    #[serde(default)]
    pub deadline: Option<String>,
}

/// Parses a date from the form into the end of that day.
///
/// # Parameters
/// * `raw` — a `YYYY-MM-DD` string from the `<date>` component
///
/// # Returns
/// * `Ok(Some(_))` — parsed · `Ok(None)` — the field was empty ·
///   `Err(_)` — unrecognised format
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
pub async fn deals_save(
    p: DealSaveParams,
    actor_usr_id: Option<i64>,
) -> Result<ActionResp, String> {
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
            "draft" | "verified" | "listed" | "rejected"
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

    // What an operator actually reviewed: the network, the price and the
    // registry. If any of it changes on a lot that is already public, the
    // approval no longer covers what is on the board — so it goes back for
    // review. Without this, a harmless lot could be approved and then quietly
    // turned into a different one.
    let reviewed_before = (
        deal.prefix.clone(),
        deal.del_amount.raw(),
        deal.rir.clone(),
        deal.resource_kind.clone(),
    );
    let was_public = deal.del_status == crate::models::deal::status::LISTED;

    // the wysiwyg returns HTML — clean it against a whitelist, else it cannot be rendered
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
        if !net.contains('/')
            && let Some(sz) = p.size.as_deref().map(str::trim).filter(|s| !s.is_empty())
        {
            let sz = sz.trim_start_matches('/');
            if sz.chars().all(|c| c.is_ascii_digit()) {
                net = format!("{net}/{sz}");
            }
        }
        deal.prefix = net;
    }
    // Geography and registry are display fields. The holder organisation is
    // what the oracle looks for in the registry, so it is stored separately and
    // geography never stands in for it.
    if let Some(v) = p.geo {
        deal.geo = Some(v).filter(|s| !s.trim().is_empty());
    }
    if let Some(v) = p.rir {
        let code = v.trim().to_ascii_uppercase();
        if crate::models::deal::RIRS.contains(&code.as_str()) {
            deal.rir = code;
        }
    }
    if let Some(v) = p.from_org {
        deal.from_org = Some(v).filter(|s| !s.trim().is_empty());
    }
    if let Some(v) = p.to_org {
        deal.to_org = Some(v).filter(|s| !s.trim().is_empty());
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

    // Ask the registry about the network: holder, resource type and country
    // come from there rather than from the seller's word. It doubles as a check
    // that the network exists at all. If the registry stays silent we do not
    // block the listing — we simply keep what was typed in.
    if !deal.prefix.trim().is_empty() {
        match crate::observer::rdap::lookup(&deal.prefix).await {
            Ok(Some(record)) => {
                if deal.from_org.as_deref().unwrap_or("").trim().is_empty()
                    && !record.holder.is_empty()
                {
                    deal.from_org = Some(record.holder.clone());
                }
                if !record.kind.is_empty() {
                    deal.resource_kind = record.kind.clone();
                }
                if deal.geo.as_deref().unwrap_or("").trim().is_empty() && !record.country.is_empty()
                {
                    deal.geo = Some(record.country.clone());
                }
                tracing::info!(
                    prefix = %deal.prefix,
                    holder = %record.holder,
                    kind = %record.kind,
                    "registry confirmed the network"
                );
            }
            Ok(None) => {
                return Err(format!(
                    "network {} is not found in the registry — check the address",
                    deal.prefix
                ));
            }
            Err(e) => {
                tracing::warn!(prefix = %deal.prefix, error = %e, "registry unreachable");
            }
        }
    }

    // A public lot whose substance changed goes back to an operator.
    let reviewed_now = (
        deal.prefix.clone(),
        deal.del_amount.raw(),
        deal.rir.clone(),
        deal.resource_kind.clone(),
    );
    let needs_review = was_public && reviewed_now != reviewed_before;
    if needs_review {
        deal.del_status = crate::models::deal::status::MODERATION.into();
    }

    // the hash is needed before sending: the card's URL is built from it
    deal.assign_hash();
    let hash = deal.del_hash.clone();

    app_context()
        .db
        .save_deal(deal.clone())
        .await
        .map_err(|e| e.to_string())?;

    // the listing went to the operator — send the card with decision buttons
    if publish || needs_review {
        crate::moderation::notify(&deal).await;
    }

    let msg = if publish {
        "Submitted for review — you will see it on the board once approved"
    } else if needs_review {
        "Changed the network, price or registry — the lot goes back for review"
    } else {
        "Listing saved"
    };
    Ok(ActionResp::redirect_with_success(
        format!("/deals/{hash}/"),
        msg,
    ))
}

#[derive(Debug, Deserialize)]
pub struct DealActionParams {
    /// The deal's permanent hash — the public identifier, used instead of id.
    pub hash: String,
    pub action: String,
    #[serde(default)]
    pub buyer_wallet: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct DealFundedParams {
    pub hash: String,
}

/// The buyer reports having funded the lock. The server checks it on chain.
///
/// The client is not trusted: the `funded` status is set only if the contract
/// confirms the money really sits in the lock under this deal. A transaction
/// hash from the client is not accepted at all — it proves nothing.
///
/// # Parameters
/// * `p` — the deal's hash
/// * `actor_usr_id` — the buyer
///
/// # Returns
/// * `ActionResp` — navigation to the deal card
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

    // Chain settings come from the `chain` constant, edited in the admin area.
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
    let locked = reader
        .locked_deal(&deal.del_hash)
        .await
        .map_err(|e| format!("chain read failed: {e}"))?;

    if locked.state != crate::chain::types::LockState::Funded {
        return Err(format!(
            "lock is not funded yet (on-chain state: {})",
            locked.state.as_str()
        ));
    }

    let expected = crate::chain::core::usdc_units(deal.del_amount).map_err(|e| e.to_string())?;
    if locked.amount < expected.to::<u128>() {
        return Err(format!(
            "locked amount {} is less than the deal price {expected}",
            locked.amount
        ));
    }

    // The wallet of whoever is asking. It has to be the one that actually paid:
    // otherwise the first person to call this handler after somebody else's
    // payment would become the buyer of record — and the exact network would
    // open to them.
    let caller_wallet = app_context()
        .db
        .wallet_address_for_user(actor)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no wallet linked to this account".to_string())?;
    if !locked.buyer_is(&caller_wallet) {
        return Err("this deal was funded from a different wallet".into());
    }

    // The seller in the contract is an argument of `fund`, chosen by the payer.
    // If it does not match the listing, the money would be released to a
    // stranger — refuse to record such a deal as paid.
    if let Some(listed_seller) = deal.seller_wallet.as_deref().filter(|s| !s.trim().is_empty())
        && !locked.seller_is(listed_seller)
    {
        return Err("the contract names a different seller than this listing".into());
    }

    // the buyer only becomes known here — by the fact of payment
    if deal.buyer_usr_id.is_none() {
        deal.buyer_usr_id = Some(actor);
        deal.buyer_wallet = Some(caller_wallet);
    }
    deal.del_status = crate::models::deal::status::FUNDED.into();
    deal.lock_tx = Some(crate::chain::deal_id_hex(&deal.del_hash));

    app_context()
        .db
        .save_deal(deal.clone())
        .await
        .map_err(|e| e.to_string())?;

    Ok(ActionResp::redirect_with_success(
        format!("/deals/{}/", p.hash),
        "USDC locked on-chain",
    ))
}

/// Is the chain live for this installation?
///
/// Several actions exist in two flavours: a stand-in for local work, and the
/// real thing that moves money. Once a chain is configured the stand-ins have
/// to be refused — otherwise a deal could be marked paid without a single
/// token moving, which is exactly the barrier this product sells.
async fn chain_is_live() -> bool {
    let Ok(constants) = app_context().db.get_constants().await else {
        return false;
    };
    let mut map = std::collections::HashMap::new();
    for key in constants.keys() {
        if let Ok(Some(v)) = constants.get::<serde_json::Value>(key) {
            map.insert(key.clone(), v);
        }
    }
    crate::chain::types::ChainConfig::from_constants(&map)
        .map(|c| c.mode().is_live())
        .unwrap_or(false)
}

/// Authorize action by party role.
/// - **offer** (sell): creator = seller; counterparty = buyer
/// - **request** (buy demand): creator = buyer; counterparty = seller
fn authorize_action(deal: &Deal, action: &str, actor: i64) -> Result<(), String> {
    let seller = is_seller(deal, actor);
    let buyer = is_buyer(deal, actor);
    let is_request = deal.listing_side == "request";
    match action {
        // `list` is deliberately absent: it puts a lot straight onto the public
        // board, bypassing review. The only way out of a draft is `publish`,
        // which sends it to an operator. The transition itself still exists in
        // the model — `approve` uses it — but nobody reaches it from outside.
        "soft_verify" | "publish" => {
            if is_creator(deal, actor) {
                Ok(())
            } else {
                Err("forbidden: creator only".into())
            }
        }
        // Anyone but the person who listed it may take a lot.
        "take" => {
            if is_creator(deal, actor) {
                Err("forbidden: you cannot take your own listing".into())
            } else {
                Ok(())
            }
        }
        // The fate of a hold is decided by whoever listed the lot.
        "confirm_deal" | "decline_deal" => {
            if is_creator(deal, actor) {
                Ok(())
            } else {
                Err("forbidden: creator only".into())
            }
        }
        // Either side may release a hold that has expired.
        "release_reserve" => {
            if seller || buyer || is_creator(deal, actor) {
                Ok(())
            } else {
                Err("forbidden: party only".into())
            }
        }
        // Accepting a response belongs to whoever published the listing — they
        // are the one being responded to. On an offer that is the seller; on a
        // demand listing it is the buyer, and requiring the seller there left
        // the button visible to somebody who could not press it.
        "accept" => {
            if is_creator(deal, actor) {
                Ok(())
            } else {
                Err("forbidden: only the person who listed it can accept".into())
            }
        }
        // Preparing the resource is the seller's work, whichever side listed it.
        "start_prepare" | "mark_prepared" => {
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

    // With a chain configured, two actions must not be taken through this path.
    //
    // `fund` here is the local stand-in: it marks the deal paid and writes a
    // made-up transaction, which would open the exact network to a buyer who
    // paid nothing. Real payment goes from the wallet, and the server confirms
    // it by reading the contract — see `deals_funded`.
    //
    // `cancel` on a funded deal would mark it refunded while the money stayed
    // in the lock. A refund is a chain operation: either the observer performs
    // it, or the buyer takes the money back themselves once the deadline has
    // passed. Marking a refund that never happened is worse than refusing.
    if chain_is_live().await {
        let paid = deal.is_paid()
            || matches!(
                deal.del_status.as_str(),
                crate::models::deal::status::FUNDED
                    | crate::models::deal::status::AWAITING_PROOF
            );
        match p.action.as_str() {
            "fund" => {
                return Err("pay from your wallet — this button is for local runs only".into());
            }
            "cancel" if paid => {
                return Err(
                    "a funded deal cannot be cancelled here: the money is in the contract, \
                     and it comes back either through the observer or through \
                     refundAfterDeadline"
                        .into(),
                );
            }
            _ => {}
        }
    }

    // The wallet comes from the user's binding, never from the client's request.
    let mut wallet_arg = None;
    if p.action == "take" {
        let w = app_context()
            .db
            .wallet_address_for_user(actor)
            .await
            .map_err(|e| e.to_string())?;
        if w.is_none() {
            return Err("no wallet linked to this account".into());
        }
        wallet_arg = w;
    }
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
        "take" => "Lot is yours for the next 6 hours — waiting for the seller to confirm",
        "confirm_deal" => "Confirmed — the buyer can fund the escrow now",
        "decline_deal" => "Declined — the lot is back on the board",
        "release_reserve" => "Reservation released — the lot is back on the board",
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
        format!("/deals/{}/", p.hash),
        msg,
    ))
}
