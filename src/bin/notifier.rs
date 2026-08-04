//! escrownad-notifier — тонкий бинарник поверх `forge_notifier::serve`.
//!
//! Вся ядерная логика (mpsc-очередь, rate-limit, Tera engine, Telegram Bot
//! API, ask/answer_callback_query/delete_message) живёт в `forge-notifier`.
//! Здесь только конфигурация и запуск. Реализация `NotifierDb` для
//! `DbClient` — в `escrownad::lib`.

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
