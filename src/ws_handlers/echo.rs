//! Демо WS-handler: принимает строку, возвращает её сервер-рендеренным
//! HTML-фрагментом через `ActionResp::replace_with_success`. Демонстрирует
//! WS-push HTML паттерн из `forge/docs/CONVENTIONS.md §7.5` —
//! никаких `window.location.reload()`, всё обновляется in-place.

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
        "ECHO принят",
    ))
}
