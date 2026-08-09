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
        let actor = ctx.user.as_ref().map(|u| u.usr_id);
        let access = crate::market_access::resolve(actor).await;
        let mut gate = crate::market_access::TemplateFlags::default();
        access.insert_template_flags(&mut gate);
        ctx.insert("market_allowed", &gate.market_allowed);
        ctx.insert("need_connect", &gate.need_connect);
        ctx.insert("need_identity", &gate.need_identity);
        ctx.insert("verify_url", &gate.verify_url);
        ctx.insert("gate_redirect", &"/cabinet/");

        let deals = if access.is_allowed() {
            if let Some(u) = ctx.user.as_ref() {
                app_context()
                    .db
                    .list_deals_for_user(u.usr_id)
                    .await
                    .map_err(|e| WsError::PageLoad(format!("cabinet deals: {e}")))?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        ctx.insert("deals", &deals);
        ctx.insert("logged_in", &ctx.user.is_some());
        Ok(())
    }
}
