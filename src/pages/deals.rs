//! Deals search, new lot, deal card. Market board = fintech table (not cards).

use async_trait::async_trait;
use forge_ws::{Page, RequestContext, WsError, WsResult};
use serde::Serialize;

use crate::app_context;
use crate::models::Deal;

/// Row for offers table — no full prefix/network identity on the board.
#[derive(Debug, Serialize)]
struct OfferRow {
    del_id: i64,
    asset_type: String,
    size_label: String,
    ip_count: String,
    resource_kind: String,
    price_per_ip: String,
    total_price: String,
    seller_verified: bool,
    del_status: String,
}

/// IPv4 host count from CIDR mask; non-IP → 1.
fn ip_count_from_prefix(prefix: &str, asset_type: &str) -> u64 {
    if asset_type != "ip" {
        return 1;
    }
    let Some((_, mask_s)) = prefix.rsplit_once('/') else {
        return 1;
    };
    let Ok(mask) = mask_s.parse::<u32>() else {
        return 1;
    };
    if mask > 32 {
        return 1;
    }
    1u64 << (32 - mask)
}

fn size_label(prefix: &str, asset_type: &str) -> String {
    match asset_type {
        "ip" => {
            if let Some((_, m)) = prefix.rsplit_once('/') {
                format!("/{m}")
            } else {
                "—".into()
            }
        }
        "domain" => "domain".into(),
        other => other.to_string(),
    }
}

/// FixedN&lt;8&gt; raw → human USDC string.
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

fn to_offer_row(d: &Deal, verified_sellers: &[i64]) -> OfferRow {
    let n = ip_count_from_prefix(&d.prefix, &d.asset_type).max(1);
    let raw = d.del_amount.raw();
    let per = raw / n as i64;
    let seller_verified = d
        .seller_usr_id
        .map(|id| verified_sellers.contains(&id))
        .unwrap_or(false);
    OfferRow {
        del_id: d.del_id,
        asset_type: d.asset_type.clone(),
        size_label: size_label(&d.prefix, &d.asset_type),
        ip_count: n.to_string(),
        resource_kind: if d.asset_type == "ip" {
            d.resource_kind.clone()
        } else {
            "—".into()
        },
        price_per_ip: format_usdc(per),
        total_price: format_usdc(raw),
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
        ctx.insert("user", &ctx.user.is_some());
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
        ctx.insert("deal", &deal);
        ctx.insert("usr_id", &ctx.user.as_ref().map(|u| u.usr_id));
        Ok(())
    }
}
