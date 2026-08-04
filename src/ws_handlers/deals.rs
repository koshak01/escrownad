//! Deal flow actions — forge ws-handler pattern (see demos.rs). EN-only toasts.

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

/// Create/update draft lot (seller).
pub async fn deals_save(p: DealSaveParams, seller_usr_id: Option<i64>) -> Result<ActionResp, String> {
    let publish = p.publish.as_deref().map(|s| s == "true" || s == "1" || s == "on").unwrap_or(false);
    let mut deal = if let Some(id) = p.id.filter(|i| *i > 0) {
        app_context()
            .db
            .get_deal(id)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "deal not found".to_string())?
    } else {
        Deal {
            del_status: "draft".into(),
            asset_type: "ip".into(),
            resource_kind: "PI".into(),
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
    if let Some(v) = p.seller_wallet.filter(|s| !s.trim().is_empty()) {
        deal.seller_wallet = Some(v);
    }
    if let Some(v) = p.contact_email {
        deal.contact_email = Some(v);
    }
    if let Some(raw) = p.del_amount {
        deal.del_amount = forge_fixed_n::FixedN::new(raw);
    }
    if deal.seller_usr_id.is_none() {
        deal.seller_usr_id = seller_usr_id;
    }

    // Prefer linked wallet if seller_wallet empty
    if deal.seller_wallet.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
        if let Some(uid) = deal.seller_usr_id {
            if let Ok(Some(w)) = app_context().db.wallet_address_for_user(uid).await {
                deal.seller_wallet = Some(w);
            }
        }
    }

    if publish {
        deal.apply_action("publish", seller_usr_id, None)?;
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

/// Lifecycle transition (want / accept / fund / prepare / confirm / …).
pub async fn deals_action(
    p: DealActionParams,
    actor_usr_id: Option<i64>,
) -> Result<ActionResp, String> {
    let mut deal = app_context()
        .db
        .get_deal(p.id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "deal not found".to_string())?;

    let mut buyer_wallet = p.buyer_wallet;
    // Auto-fill buyer wallet from users2wallets
    if p.action == "request" && buyer_wallet.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
        if let Some(uid) = actor_usr_id {
            if let Ok(Some(w)) = app_context().db.wallet_address_for_user(uid).await {
                buyer_wallet = Some(w);
            }
        }
    }

    deal.apply_action(&p.action, actor_usr_id, buyer_wallet)?;

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
