//! Public deals list `/deals/` and deal card `/deals/{id}/`.

use async_trait::async_trait;
use forge_ws::{Page, RequestContext, WsError, WsResult};

use crate::app_context;

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
            .list_deals_listed()
            .await
            .map_err(|e| WsError::PageLoad(format!("list_deals_listed: {e}")))?;
        ctx.insert("deals", &deals);
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
        Ok(())
    }
}
