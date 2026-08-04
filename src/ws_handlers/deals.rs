//! Deal flow actions — forge ws-handler pattern (see demos.rs).

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
}

/// Create/update draft lot (seller).
pub async fn deals_save(p: DealSaveParams, seller_usr_id: Option<i64>) -> Result<ActionResp, String> {
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
    if let Some(v) = p.seller_wallet {
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

    app_context()
        .db
        .save_deal(deal)
        .await
        .map_err(|e| e.to_string())?;

    Ok(ActionResp::redirect_with_success(
        "/cabinet/",
        "Лот сохранён",
    ))
}

#[derive(Debug, Deserialize)]
pub struct DealActionParams {
    pub id: i64,
    pub action: String,
    #[serde(default)]
    pub buyer_wallet: Option<String>,
}

/// Lifecycle transition (want / accept / prepare / fund / …).
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

    deal.apply_action(&p.action, actor_usr_id, p.buyer_wallet)?;

    app_context()
        .db
        .save_deal(deal.clone())
        .await
        .map_err(|e| e.to_string())?;

    let msg = match p.action.as_str() {
        "soft_verify" => "Soft-check OK (email stub)",
        "list" => "Лот в поиске",
        "request" => "Заявка «хочу» отправлена",
        "accept" => "Заявка принята",
        "start_prepare" => "Подготовка начата",
        "mark_prepared" => "Готово к оплате USDC",
        "fund" => "USDC в замке (mock) — ждём proof",
        "open_dispute" => "Спор открыт",
        "cancel" => "Отменено / refund",
        _ => "OK",
    };

    Ok(ActionResp::redirect_with_success(
        &format!("/deals/{}/", p.id),
        msg,
    ))
}
