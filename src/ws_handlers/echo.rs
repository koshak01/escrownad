//! Demo ws handler: takes a string and returns it as a server-rendered HTML
//! fragment through `ActionResp::replace_with_success`. It demonstrates the
//! WS-pushed HTML pattern — no `window.location.reload()`, everything updates
//! in place.

use forge_ws::{ActionResp, HtmlReplace};
use serde::Deserialize;
use serde_json::json;

use crate::app_context;

#[derive(Debug, Deserialize)]
pub struct EchoParams {
    pub text: String,
}

pub async fn echo(p: EchoParams) -> Result<ActionResp, String> {
    let ctx = json!({
        "text": p.text,
        "now":  chrono::Utc::now().to_rfc3339(),
    });

    let html = app_context()
        .renderer
        .read()
        .await
        .render_partial("_echo_result.html.tera", &ctx)
        .map_err(|e| format!("render: {e}"))?;

    Ok(ActionResp::replace_with_success(
        vec![HtmlReplace {
            selector: "#echo-result".into(),
            html,
        }],
        "ECHO received",
    ))
}
