//! Список demo — `/admin/demos/`. Structure-driven фильтр (`DemoListFilter`)
//! + сортировка. Общий пролог/эпилог — ядерные `forge_admin::{load_filter,
//! page_sort, prepare_list_ctx}` (те же, что у 7 ядерных списков).
//!
//! Отличие от ядерных страниц: данные тянем через `app_context().db.*` (свой
//! `DbCommand`), а не через `env()` — это проектная сущность.

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
