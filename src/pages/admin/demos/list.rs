//! Demo list — `/admin/demos/`. Structure-driven filter (`DemoListFilter`)
//! plus sorting. The shared prologue and epilogue are the platform's
//! `forge_admin::{load_filter, page_sort, prepare_list_ctx}` — the same ones
//! seven platform lists use.
//!
//! Unlike the platform pages, the data comes through `app_context().db.*` —
//! our own `DbCommand` — rather than through `env()`, because this entity
//! belongs to the project.

use async_trait::async_trait;
use forge_ws::{AuthRequirement, Page, RequestContext, WsError, WsResult};

use crate::models::DemoListFilter;

pub struct ListPage;

#[async_trait]
impl Page for ListPage {
    fn path(&self) -> &'static str {
        "/admin/demos/"
    }
    fn template(&self) -> &'static str {
        "admin/demos/list.html.tera"
    }
    fn auth(&self) -> AuthRequirement {
        AuthRequirement::Roles(&["admin"])
    }

    async fn load(&self, ctx: &mut RequestContext) -> WsResult<()> {
        let filter: DemoListFilter = forge_admin::load_filter(ctx).await?;
        let sort = forge_admin::page_sort(ctx);
        let demos = crate::app_context()
            .db
            .list_demos_filtered(filter.clone(), sort)
            .await
            .map_err(|e| WsError::PageLoad(format!("list_demos_filtered: {e}")))?;
        forge_admin::prepare_list_ctx(ctx, "demos", "demos", &demos, &filter).await?;
        Ok(())
    }
}
