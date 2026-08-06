//! Home page — `/`.
//!
//! A minimal Page: a path, a template and an empty load. Real projects call
//! `app_context().db.*` inside `load` to fetch data and place it into
//! `ctx.insert("key", &value)` for the Tera template.

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
