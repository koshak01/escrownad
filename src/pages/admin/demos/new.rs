//! Создание demo — `/admin/demos/new/`. Шаблон общий с edit: кладём пустой
//! `Demo` (`dmo_id = 0`, `dmo_is_enable = true`) — форма отрендерит новую запись.

use async_trait::async_trait;
use forge_ws::{AuthRequirement, Page, RequestContext, WsResult};

use crate::models::Demo;

pub struct NewPage;

#[async_trait]
impl Page for NewPage {
    fn path(&self) -> &'static str {
        "/admin/demos/new/"
    }
    fn template(&self) -> &'static str {
        "admin/demos/edit.html.tera"
    }
    fn auth(&self) -> AuthRequirement {
        AuthRequirement::Roles(&["admin"])
    }

    async fn load(&self, ctx: &mut RequestContext) -> WsResult<()> {
        let empty = Demo {
            dmo_is_enable: true,
            ..Default::default()
        };
        ctx.insert("demo", &empty);
        Ok(())
    }
}
