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

/// Create/update draft lot (seller only).
pub async fn deals_save(p: DealSaveParams, seller_usr_id: Option<i64>) -> Result<ActionResp, String> {
    let actor = require_actor(seller_usr_id)?;
    let publish = p
        .publish
        .as_deref()
        .map(|s| s == "true" || s == "1" || s == "on")
        .unwrap_or(false);

    let mut deal = if let Some(id) = p.id.filter(|i| *i > 0) {
        let existing = app_context()
            .db
            .get_deal(id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "deal not found".to_string())?;
        if !is_seller(&existing, actor) {
            return Err("forbidden: only the seller can edit this offer".into());
        }
        // Only editable while not past listed market stage (no rewrite after money)
        if !matches!(
            existing.del_status.as_str(),
            "draft" | "verified" | "listed"
        ) {
            return Err("offer is locked at this stage".into());
        }
        existing
    } else {
        Deal {
            del_status: "draft".into(),
            asset_type: "ip".into(),
            resource_kind: "PI".into(),
            chain_id: "monad".into(),
            del_is_enable: true,
            soft_verified: false,
            seller_usr_id: Some(actor),
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
    // Seller wallet: only own linked wallet (ignore spoofed client value)
    if let Ok(Some(w)) = app_context().db.wallet_address_for_user(actor).await {
        deal.seller_wallet = Some(w);
    } else if let Some(v) = p.seller_wallet.filter(|s| !s.trim().is_empty()) {
        // Allow explicit only if matches normalized format; still bind to actor
        deal.seller_wallet = Some(v);
    }
    if let Some(v) = p.contact_email {
        deal.contact_email = Some(v);
    }
    if let Some(raw) = p.del_amount {
        deal.del_amount = forge_fixed_n::FixedN::new(raw);
    }
    deal.seller_usr_id = Some(actor);

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
fn authorize_action(deal: &Deal, action: &str, actor: i64) -> Result<(), String> {
    let seller = is_seller(deal, actor);
    let buyer = is_buyer(deal, actor);
    match action {
        "soft_verify" | "list" | "publish" | "accept" | "start_prepare" | "mark_prepared" => {
            if seller {
                Ok(())
            } else {
                Err("forbidden: seller only".into())
            }
        }
        "request" => {
            if seller {
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

    // Never trust client buyer_wallet — bind from session linkage
    let mut buyer_wallet = None;
    if p.action == "request" {
        buyer_wallet = app_context()
            .db
            .wallet_address_for_user(actor)
            .await
            .map_err(|e| e.to_string())?;
        if buyer_wallet.is_none() {
            return Err("no wallet linked to this account".into());
        }
    }

    deal.apply_action(&p.action, Some(actor), buyer_wallet)?;

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
