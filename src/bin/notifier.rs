//! escrownad-notifier — a thin binary over `forge_notifier::serve`.
//!
//! Every piece of the machinery — the mpsc queue, rate limiting, the Tera
//! engine, the Telegram Bot API, ask/answer_callback_query/delete_message —
//! lives in `forge-notifier`. This file only configures and starts it. The
//! `NotifierDb` implementation for `DbClient` is in `escrownad::lib`.

use escrownad::{DbClient, sockets};
use tracing::info;

#[tokio::main]
async fn main() {
    let log_path = forge_core::logger::init_with_file("escrownad-notifier");
    info!(log = %log_path.display(), "logger initialized");

    let db = DbClient::spawn();

    let cfg = forge_notifier::Config {
        app_name: "escrownad",
        ipc_socket: sockets::NOTIFIER,
        config_path: "etc/notifier.toml",
        constants_key: "telegrams",
    };

    if let Err(e) = forge_notifier::serve(cfg, db).await {
        forge_core::error::log_chain(e.as_ref());
        std::process::exit(1);
    }
}
