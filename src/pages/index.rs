//! Главная страница — `/`.
//!
//! Минимальный пример Page: путь, шаблон, пустой load. Реальные проекты
//! в `load` дёргают `app_context().db.*` чтобы достать данные и положить
//! в `ctx.insert("key", &value)` для Tera-шаблона.

use async_trait::async_trait;
use forge_ws::{Page, RequestContext, WsResult};

pub struct IndexPage;

#[async_trait]
impl Page for IndexPage {
    fn path(&self) -> &'static str {
        "/"
    }
    fn template(&self) -> &'static str {
        "index.html.tera"
    }
    async fn load(&self, _ctx: &mut RequestContext) -> WsResult<()> {
        Ok(())
    }
}
