//! Market board — auction table: lot #, offer type, price, listed age, seller ✓.

use async_trait::async_trait;
use forge_core::Timestamp;
use forge_ws::{Page, RequestContext, WsError, WsResult};
use serde::Serialize;

use crate::app_context;
use crate::models::Deal;

/// Known offer types (more fields per type later via checklist_json / forms).
pub const OFFER_TYPES: &[&str] = &["ip", "domain", "property", "work", "other"];

#[derive(Debug, Serialize)]
struct OfferRow {
    del_id: i64,
    /// Offer type: ip | domain | property | work | other
    offer_type: String,
    total_price: String,
    /// Human age since created/listed, e.g. "12m", "3h", "2d"
    listed_for: String,
    seller_verified: bool,
    del_status: String,
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

fn to_offer_row(d: &Deal, verified_sellers: &[i64]) -> OfferRow {
    let seller_verified = d
        .seller_usr_id
        .map(|id| verified_sellers.contains(&id))
        .unwrap_or(false);
    OfferRow {
        del_id: d.del_id,
        offer_type: d.asset_type.clone(),
        total_price: format_usdc(d.del_amount.raw()),
        listed_for: listed_for(d.del_dat),
        seller_verified,
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
        let deals = app_context()
            .db
            .list_deals_board()
            .await
            .map_err(|e| WsError::PageLoad(format!("list_deals_board: {e}")))?;

        let verified_sellers = app_context()
            .db
            .list_verified_sellers()
            .await
            .unwrap_or_default();

        let rows: Vec<OfferRow> = deals
            .iter()
            .filter(|d| !matches!(d.del_status.as_str(), "draft" | "verified"))
            .map(|d| to_offer_row(d, &verified_sellers))
            .collect();

        ctx.insert("deals", &rows);
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
        Ok(())
    }
}

pub struct DealShowPage;

#[async_trait]
impl Page for DealShowPage {
    fn path(&self) -> &'static str {
        "/deals/{id}/"
    }
    fn template(&self) -> &'static str {
        "deals/show.html.tera"
    }
    async fn load(&self, ctx: &mut RequestContext) -> WsResult<()> {
        let id: i64 = ctx
            .path_params
            .get("id")
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| WsError::PageLoad("invalid deal id".into()))?;
        let deal = app_context()
            .db
            .get_deal(id)
            .await
            .map_err(|e| WsError::PageLoad(format!("get_deal: {e}")))?
            .ok_or_else(|| WsError::NotFound("deal not found".into()))?;

        let verified_sellers = app_context()
            .db
            .list_verified_sellers()
            .await
            .unwrap_or_default();
        let seller_verified = deal
            .seller_usr_id
            .map(|id| verified_sellers.contains(&id))
            .unwrap_or(false);

        ctx.insert("deal", &deal);
        ctx.insert("usr_id", &ctx.user.as_ref().map(|u| u.usr_id));
        ctx.insert("seller_verified", &seller_verified);
        ctx.insert("listed_for", &listed_for(deal.del_dat));
        Ok(())
    }
}
