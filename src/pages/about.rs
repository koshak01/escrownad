//! `/about/` — product about page (EN).

use async_trait::async_trait;
use forge_ws::{Page, RequestContext, WsResult};

pub struct AboutPage;

#[async_trait]
impl Page for AboutPage {
    fn path(&self) -> &'static str {
        "/about/"
    }
    fn template(&self) -> &'static str {
        "about.html.tera"
    }
    async fn load(&self, _ctx: &mut RequestContext) -> WsResult<()> {
        Ok(())
    }
}
