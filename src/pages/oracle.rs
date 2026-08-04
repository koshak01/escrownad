//! `/oracle/` — explain proof-oracle for judges / buyers.

use async_trait::async_trait;
use forge_ws::{Page, RequestContext, WsResult};

pub struct OraclePage;

#[async_trait]
impl Page for OraclePage {
    fn path(&self) -> &'static str {
        "/oracle/"
    }
    fn template(&self) -> &'static str {
        "oracle.html.tera"
    }
    async fn load(&self, _ctx: &mut RequestContext) -> WsResult<()> {
        Ok(())
    }
}
