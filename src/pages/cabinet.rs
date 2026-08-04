//! `/cabinet/` — one cabinet (seller + buyer sides).

use async_trait::async_trait;
use forge_ws::{Page, RequestContext, WsError, WsResult};

use crate::app_context;

pub struct CabinetPage;

#[async_trait]
impl Page for CabinetPage {
    fn path(&self) -> &'static str {
        "/cabinet/"
    }
    fn template(&self) -> &'static str {
        "cabinet.html.tera"
    }
    async fn load(&self, ctx: &mut RequestContext) -> WsResult<()> {
        let deals = if let Some(u) = ctx.user.as_ref() {
            app_context()
                .db
                .list_deals_for_user(u.usr_id)
                .await
                .map_err(|e| WsError::PageLoad(format!("cabinet deals: {e}")))?
        } else {
            // Not logged in: show board overview (demo); login for own lots.
            app_context()
                .db
                .list_deals_board()
                .await
                .map_err(|e| WsError::PageLoad(format!("board: {e}")))?
        };
        ctx.insert("deals", &deals);
        ctx.insert("logged_in", &ctx.user.is_some());
        Ok(())
    }
}
