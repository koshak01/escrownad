//! Deal flow actions — forge ws-handler pattern (see demos.rs). EN-only toasts.
//!
//! **Authz:** every mutation checks seller/buyer party (IDOR-safe).

use forge_ws::ActionResp;
use serde::Deserialize;

use crate::app_context;
use crate::models::Deal;

#[derive(Debug, Deserialize)]
pub struct DealSaveParams {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub del_title: Option<String>,
    #[serde(default)]
    pub del_note: Option<String>,
    #[serde(default)]
    pub asset_type: Option<String>,
    #[serde(default)]
    pub resource_kind: Option<String>,
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
    #[serde(default)]
    pub del_amount: Option<i64>,
    /// If true after save → soft_verify + list (publish to market).
    #[serde(default)]
    pub publish: Option<String>,
    /// `offer` (sell) | `request` (buy demand).
    #[serde(default)]
    pub listing_side: Option<String>,
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

    if let Some(v) = p.del_title {
        deal.del_title = Some(v);
    }
    if let Some(v) = p.del_note {
        deal.del_note = Some(v);
    }
    if let Some(v) = p.asset_type {
        deal.asset_type = v;
    }
    if let Some(v) = p.resource_kind {
        deal.resource_kind = v;
    }
    if let Some(v) = p.prefix {
        deal.prefix = v;
    }
    if let Some(v) = p.from_org {
        deal.from_org = Some(v);
    }
    if let Some(v) = p.to_org {
        deal.to_org = Some(v);
    }
    if let Some(v) = p.contact_email {
        deal.contact_email = Some(v);
    }
    if let Some(units) = p.del_amount {
        deal.del_amount = forge_fixed_n::FixedN::from_int(units);
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

    app_context()
        .db
        .save_deal(deal.clone())
        .await
        .map_err(|e| e.to_string())?;

    let id = deal.del_id;
    let msg = if publish {
        "Offer published"
    } else {
        "Offer saved"
    };
    Ok(ActionResp::redirect_with_success(
        &format!("/deals/{id}/"),
        msg,
    ))
}

#[derive(Debug, Deserialize)]
pub struct DealActionParams {
    pub id: i64,
    pub action: String,
    #[serde(default)]
    pub buyer_wallet: Option<String>,
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
        .get_deal(p.id)
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
        &format!("/deals/{}/", p.id),
        msg,
    ))
}
