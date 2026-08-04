//! Редактирование demo — `/admin/demos/{id}/`. Тянет запись через
//! `app_context().db.get_demo(id)` и кладёт в `ctx` под ключом `demo`.

use async_trait::async_trait;
use forge_ws::{AuthRequirement, Page, RequestContext, WsError, WsResult};

pub struct EditPage;

#[async_trait]
impl Page for EditPage {
    fn path(&self) -> &'static str {
        "/admin/demos/{id}/"
    }
    fn template(&self) -> &'static str {
        "admin/demos/edit.html.tera"
    }
    fn auth(&self) -> AuthRequirement {
        AuthRequirement::Roles(&["admin"])
    }

    async fn load(&self, ctx: &mut RequestContext) -> WsResult<()> {
        let id: i64 = ctx
            .path_params
            .get("id")
            .ok_or_else(|| WsError::PageLoad("id missing in path_params".into()))?
            .parse()
            .map_err(|e| WsError::PageLoad(format!("invalid id: {e}")))?;
        let demo = crate::app_context()
            .db
            .get_demo(id)
            .await
            .map_err(|e| WsError::PageLoad(format!("get_demo: {e}")))?
            .ok_or_else(|| WsError::NotFound(format!("demo {id}")))?;
        ctx.insert("demo", &demo);
        Ok(())
    }
}
