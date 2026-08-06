//! escrownad-web — a thin HTTP facade. It proxies rendering to escrownad-ws
//! over IPC and serves static files: `/static` is the project's, `/shared` the
//! platform's.
//!
//! All the plumbing (WebConfig + proxy_handler + Router) lives in
//! forge_web::bootstrap. The project binary comes down to loading the config,
//! picking a ServeMode and calling serve.

use anyhow::{Context, Result, bail};
use escrownad::sockets;
use forge_web::bootstrap::{WebConfig, WebState, build_router};
use forge_web::{ServeMode, serve};
use tracing::info;

async fn run() -> Result<()> {
    info!("starting");
    let cfg: WebConfig =
        forge_core::config::load_toml("etc/web.toml").context("load etc/web.toml")?;

    // Project static/ + shared forge static (../forge/static).
    let state = WebState::spawn(sockets::WS);
    let app = build_router(state, "static", "../forge/static");

    let serve_mode = match cfg.server.mode.as_str() {
        "tcp" => ServeMode::Tcp {
            port: cfg.server.http_port,
        },
        "unix" => ServeMode::Unix {
            path: cfg.server.socket_path.clone(),
        },
        other => bail!("etc/web.toml: unknown server.mode '{other}' (expected: tcp | unix)"),
    };
    info!(?serve_mode, "http listening");
    serve(app, serve_mode).await.context("axum serve")?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let log_path = forge_core::logger::init_with_file("escrownad-web");
    info!(log = %log_path.display(), "logger initialized");
    if let Err(e) = run().await {
        forge_core::error::log_chain(e.as_ref());
        std::process::exit(1);
    }
}
