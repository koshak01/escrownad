//! escrownad — the application skeleton on the forge platform.
//!
//! Four binaries (`database` / `notifier` / `ws` / `web`) share:
//!   - unix socket paths — `sockets::*`
//!   - the database command types — `DbCommand` / `DbResponse`
//!   - a common ws context — `AppContext`, via `init_app_context()`/`app_context()`
//!
//! System SQL (ping, now_epoch, constants) and the Postgres connection live in
//! the platform (`forge_db::pg`, `forge_db::system`). Platform admin pages are
//! in `forge-admin`; the Telegram queue and its templates in `forge-notifier`.
//!
//! This file holds only the skeleton needed to bring the stack up end to end.
//! Domain models, commands and pages are added by the project on top of that
//! minimum.

pub mod chain;
pub mod cleanverse;
pub mod models;
pub mod moderation;
pub mod observer;
pub mod pages;
pub mod sanitize;
pub mod tg_hook;
pub mod wallet_auth;
pub mod ws_handlers;

use std::sync::{Arc, OnceLock};

use forge_admin::{AdminDbCommand, AdminDbResponse};
use forge_session::RedisClient;
use forge_ws::{Page, Renderer};
use tokio::sync::RwLock;

pub const APP_NAME: &str = "escrownad";

/// Internal IPC sockets between the binaries (database/notifier/ws/web).
/// A `.` in the name is the platform convention for internal IPC. External
/// listeners (nginx → binary) use `_`: `/tmp/escrownad_ws.sock` and
/// `/tmp/escrownad_web.sock`, configured in etc/ws.toml and etc/web.toml,
/// not here.
pub mod sockets {
    pub const DATABASE: &str = "/tmp/escrownad.database.sock";
    pub const NOTIFIER: &str = "/tmp/escrownad.notifier.sock";
    pub const WS: &str = "/tmp/escrownad.ws.sock";
    pub const WEB: &str = "/tmp/escrownad.web.sock";
}

// ──────────────────────────────────────────────────────────────────────────────
// AppContext — the shared context of the ws daemon
// ──────────────────────────────────────────────────────────────────────────────

/// Everything a ws handler needs: the database client, Redis sessions, the
/// Tera renderer and the notifier client.
pub struct AppContext {
    pub db: Arc<DbClient>,
    pub redis: RedisClient,
    pub renderer: Arc<RwLock<Renderer>>,
    pub notifier: Arc<NotifierClient>,
}

static APP_CONTEXT: OnceLock<AppContext> = OnceLock::new();

pub fn init_app_context(ctx: AppContext) {
    APP_CONTEXT
        .set(ctx)
        .map_err(|_| ())
        .expect("AppContext already initialized");
}

pub fn app_context() -> &'static AppContext {
    APP_CONTEXT
        .get()
        .expect("AppContext not initialized — call init_app_context() when ws starts")
}

/// Page registry: the project's own pages first — they may override `/admin/`
/// with a stub of their own — then the platform admin pages.
pub fn pages() -> Vec<Box<dyn Page>> {
    let mut out: Vec<Box<dyn Page>> = vec![
        Box::new(pages::IndexPage),
        Box::new(pages::AboutPage),
        Box::new(pages::CabinetPage),
        Box::new(pages::DealsListPage),
        Box::new(pages::DealNewPage),
        Box::new(pages::DealShowPage),
        Box::new(pages::OraclePage),
        // Reference: the project's own admin pages (the demo entity).
        // Order list → new → edit: `/admin/demos/new/` MUST come before
        // `/admin/demos/{id}/`, or `new` would be captured as an `{id}`.
        Box::new(pages::admin::demos::list::ListPage),
        Box::new(pages::admin::demos::new::NewPage),
        Box::new(pages::admin::demos::edit::EditPage),
    ];
    out.extend(forge_admin::pages::all());
    out
}

// ──────────────────────────────────────────────────────────────────────────────
// IPC: commands from ws/web/notifier to database
// ──────────────────────────────────────────────────────────────────────────────

/// The minimal command set for the database: system commands (Ping/GetSalt/…)
/// and platform admin CRUD — everything needed to start. Project commands are
/// added on top: new enum variants plus branches in `database.rs`.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum DbCommand {
    Ping,
    GetSalt,
    GetConstants,
    UpdateConstant {
        key: String,
        value: Vec<u8>,
    },
    ReloadConstant {
        code: Option<String>,
    },

    /// Every platform admin CRUD command inside one wrapper.
    /// It grows automatically as variants are added to forge-admin.
    /// Login (find user by email) also goes through admin — see
    /// `AdminDbCommand::GetUserByEmail` + `forge_admin::ws_auth::login`.
    Admin {
        cmd: AdminDbCommand,
    },

    // ── Reference domain commands (the `demos` entity) ──────────────────────
    // This is how a project adds CRUD for its own table: variants go into ITS
    // OWN `DbCommand`, not into `AdminDbCommand`, and are handled in
    // `bin/database.rs`. The whole struct travels as one: `data: Demo`, never
    // a scatter of fields.
    /// Demo list with a filter and sorting (structure-driven, ADR-0010).
    ListDemosFiltered {
        filter: models::DemoListFilter,
        sort: Option<forge_admin::handlers::SortSpec>,
    },
    /// A single record by id — for the edit form.
    GetDemo {
        id: i64,
    },
    /// Upsert: `dmo_id == 0` inserts, anything else updates.
    SaveDemo {
        data: models::Demo,
    },
    /// Delete by id.
    DeleteDemo {
        id: i64,
    },

    // ── deals (proof-escrow) ────────────────────────────────────────────────
    ListDealsListed,
    ListDealsBoard,
    ListDealsForUser {
        usr_id: i64,
    },
    ListDealsFiltered {
        filter: models::DealListFilter,
        sort: Option<forge_admin::handlers::SortSpec>,
    },
    GetDeal {
        id: i64,
    },
    /// Public access to a deal — by its permanent hash, never by id.
    GetDealByHash {
        hash: String,
    },
    /// Lookup by hash prefix: a full hash does not fit in a Telegram button.
    GetDealByHashPrefix {
        prefix: String,
    },
    SaveDeal {
        data: models::Deal,
    },

    /// Find or create forge user for lowercase 0x EVM address (wallet login).
    /// `pubkey` — the public key recovered from the sign-in signature; empty
    /// when recovery failed, in which case the stored value is left alone.
    WalletFindOrCreate {
        address: String,
        pubkey: String,
    },
    WalletAddressForUser {
        usr_id: i64,
    },
    ListVerifiedSellers,
}

// large_enum_variant: `Admin` carries the whole AdminDbResponse, which is
// large by design — same as `DbCommand::Admin`. This is an IPC wrapper that is
// serialised to msgpack and dropped immediately; boxing it for memory layout
// buys nothing and costs a heap allocation per reply. Symmetric with
// `DbCommand` above — allow rather than Box.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum DbResponse {
    Pong,
    Salt(i64),
    Constants(Constants),
    Admin(AdminDbResponse),
    // ── replies of the demo entity (reference) ─────────────────────────────
    Demos(Vec<models::Demo>),
    Demo(Option<models::Demo>),
    // deals
    Deals(Vec<models::Deal>),
    Deal(Option<models::Deal>),
    WalletUser(wallet_auth::WalletUserRow),
    WalletAddress(Option<String>),
    VerifiedSellers(Vec<i64>),
    Ok,
}

// ──────────────────────────────────────────────────────────────────────────────
// Constants — a snapshot of the platform constants, cached in database
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Constants(pub std::collections::HashMap<String, Vec<u8>>);

impl Constants {
    pub fn new() -> Self {
        Self(std::collections::HashMap::new())
    }
    pub fn insert(&mut self, key: String, json_bytes: Vec<u8>) {
        self.0.insert(key, json_bytes);
    }
    pub fn remove(&mut self, key: &str) -> Option<Vec<u8>> {
        self.0.remove(key)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn keys(&self) -> impl Iterator<Item = &String> {
        self.0.keys()
    }
    pub fn raw(&self, key: &str) -> Option<&[u8]> {
        self.0.get(key).map(|v| v.as_slice())
    }
    pub fn get<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> Result<Option<T>, serde_json::Error> {
        match self.0.get(key) {
            Some(bytes) => Ok(Some(serde_json::from_slice(bytes)?)),
            None => Ok(None),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Typed clients
// ──────────────────────────────────────────────────────────────────────────────

forge_ipc::client! {
    /// Client to escrownad-database. The only route to Postgres data.
    pub DbClient(DbCommand, DbResponse, sockets::DATABASE) {
        pub fn ping() = Ping -> Pong;
        pub fn get_salt() -> i64 = GetSalt -> Salt(v) = v;
        pub fn get_constants() -> Constants = GetConstants -> Constants(v) = v;
        pub fn update_constant(key: String, value: Vec<u8>)
            = UpdateConstant { key, value } -> Ok;
        pub fn reload_constant(code: Option<String>)
            = ReloadConstant { code } -> Ok;

        /// One wrapper method for EVERY forge-admin CRUD command.
        /// It grows automatically; the application never edits it.
        pub fn admin(cmd: AdminDbCommand) -> AdminDbResponse
            = Admin { cmd } -> Admin(resp) = resp;

        // ── Reference: domain methods of the demo entity ───────────────────
        // Pages and ws handlers reach the database through these:
        // `app_context().db.list_demos_filtered(...)`.
        pub fn list_demos_filtered(filter: models::DemoListFilter, sort: Option<forge_admin::handlers::SortSpec>)
            -> Vec<models::Demo>
            = ListDemosFiltered { filter, sort } -> Demos(v) = v;
        pub fn get_demo(id: i64) -> Option<models::Demo> = GetDemo { id } -> Demo(v) = v;
        pub fn save_demo(data: models::Demo) = SaveDemo { data } -> Ok;
        pub fn delete_demo(id: i64) = DeleteDemo { id } -> Ok;

        // deals
        pub fn list_deals_listed() -> Vec<models::Deal> = ListDealsListed -> Deals(v) = v;
        pub fn list_deals_board() -> Vec<models::Deal> = ListDealsBoard -> Deals(v) = v;
        pub fn list_deals_for_user(usr_id: i64) -> Vec<models::Deal>
            = ListDealsForUser { usr_id } -> Deals(v) = v;
        pub fn list_deals_filtered(filter: models::DealListFilter, sort: Option<forge_admin::handlers::SortSpec>)
            -> Vec<models::Deal>
            = ListDealsFiltered { filter, sort } -> Deals(v) = v;
        pub fn get_deal(id: i64) -> Option<models::Deal> = GetDeal { id } -> Deal(v) = v;
        pub fn get_deal_by_hash(hash: String) -> Option<models::Deal>
            = GetDealByHash { hash } -> Deal(v) = v;
        pub fn get_deal_by_hash_prefix(prefix: String) -> Option<models::Deal>
            = GetDealByHashPrefix { prefix } -> Deal(v) = v;
        pub fn save_deal(data: models::Deal) = SaveDeal { data } -> Ok;

        /// Find-or-create user by EVM wallet address (lowercase 0x…).
        pub fn wallet_find_or_create(address: String, pubkey: String)
            -> wallet_auth::WalletUserRow
            = WalletFindOrCreate { address, pubkey } -> WalletUser(v) = v;
        pub fn wallet_address_for_user(usr_id: i64) -> Option<String>
            = WalletAddressForUser { usr_id } -> WalletAddress(v) = v;
        pub fn list_verified_sellers() -> Vec<i64>
            = ListVerifiedSellers -> VerifiedSellers(v) = v;
    }
}

#[async_trait::async_trait]
impl forge_admin::AdminCommandSender for DbClient {
    async fn send(&self, cmd: AdminDbCommand) -> Result<AdminDbResponse, String> {
        self.admin(cmd).await.map_err(|e| e.to_string())
    }
}

/// `ConstantsSink` for `forge_admin::hooks::DefaultAdminHooks` — delegates to
/// the DbClient methods generated by the `client!` macro.
#[async_trait::async_trait]
impl forge_admin::hooks::ConstantsSink for DbClient {
    async fn reload_constant(&self, code: Option<String>) -> Result<(), String> {
        DbClient::reload_constant(self, code)
            .await
            .map_err(|e| e.to_string())
    }

    async fn get_constants_json(
        &self,
    ) -> Result<std::collections::HashMap<String, serde_json::Value>, String> {
        let snapshot = self.get_constants().await.map_err(|e| e.to_string())?;
        let mut out = std::collections::HashMap::new();
        for key in snapshot.keys() {
            if let Ok(Some(v)) = snapshot.get::<serde_json::Value>(key) {
                out.insert(key.clone(), v);
            }
        }
        Ok(out)
    }
}

impl DbClient {
    /// Blocks until database is ready — used when bootstrapping dependent daemons.
    pub async fn wait_ready(&self, timeout: std::time::Duration) -> forge_ipc::IpcResult<()> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if self.ping().await.is_ok() {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                return Err(forge_ipc::IpcError::Reconnecting);
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// WsClient — the web binary's client to escrownad-ws (IPC render).
// ──────────────────────────────────────────────────────────────────────────────

pub struct WsClient {
    inner: forge_ipc::IpcClient<forge_ws::RenderRequest, forge_ws::RenderResponse>,
}

impl WsClient {
    pub fn spawn() -> Self {
        Self {
            inner: forge_ipc::IpcClient::spawn(sockets::WS),
        }
    }

    pub async fn render(
        &self,
        req: forge_ws::RenderRequest,
    ) -> forge_ipc::IpcResult<forge_ws::RenderResponse> {
        self.inner.execute(req).await
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// NotifierClient — the IPC client from ws/web into notifier.
// Speaks the forge_notifier protocol (NotifierCommand/Response).
// ──────────────────────────────────────────────────────────────────────────────

forge_ipc::client! {
    /// Client to escrownad-notifier for queueing messages.
    pub NotifierClient(forge_notifier::NotifierCommand, forge_notifier::NotifierResponse, sockets::NOTIFIER) {
        pub fn ping() = Ping -> Pong;
    }
}

impl NotifierClient {
    /// Answer a Telegram button press — stops the operator's spinner.
    ///
    /// Bypasses the queue: Telegram waits 15 seconds for an answer.
    ///
    /// # Parameters
    /// * `callback_query_id` — the press identifier from the webhook
    /// * `text` — a short notice above the button
    /// * `show_alert` — `true` shows a dialog instead of a toast
    pub async fn answer_callback(
        &self,
        callback_query_id: String,
        text: Option<String>,
        show_alert: bool,
    ) -> forge_ipc::IpcResult<()> {
        match self
            .inner
            .execute(forge_notifier::NotifierCommand::AnswerCallbackQuery {
                callback_query_id,
                text,
                show_alert,
            })
            .await?
        {
            forge_notifier::NotifierResponse::Queued => Ok(()),
            other => Err(forge_ipc::IpcError::Server(format!(
                "unexpected: {other:?}"
            ))),
        }
    }

    /// Queue a message rendered from template `tpl_code`. `params` is the
    /// context for the Tera render, usually a `serde_json!({...})`.
    pub async fn send<P: serde::Serialize>(
        &self,
        tpl_code: impl Into<String>,
        params: &P,
    ) -> forge_ipc::IpcResult<()> {
        let bytes =
            serde_json::to_vec(params).map_err(|e| forge_ipc::IpcError::Encode(e.to_string()))?;
        match self
            .inner
            .execute(forge_notifier::NotifierCommand::Send {
                tpl_code: tpl_code.into(),
                params: bytes,
                usr_id: None,
            })
            .await?
        {
            forge_notifier::NotifierResponse::Queued => Ok(()),
            other => Err(forge_ipc::IpcError::Server(format!(
                "unexpected: {other:?}"
            ))),
        }
    }
}

/// `forge_notifier::NotifierDb` implemented for `DbClient`. The notifier works
/// with any application through this trait, knowing nothing of its DbCommand.
#[async_trait::async_trait]
impl forge_notifier::NotifierDb for DbClient {
    async fn wait_ready(&self, timeout: std::time::Duration) -> anyhow::Result<()> {
        DbClient::wait_ready(self, timeout)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    async fn get_constant_json(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let constants = self.get_constants().await?;
        Ok(constants.raw(key).map(|b| b.to_vec()))
    }

    async fn list_templates_with_channel(
        &self,
    ) -> anyhow::Result<Vec<forge_admin::TemplateWithChannel>> {
        match self.admin(AdminDbCommand::ListTemplatesWithChannel).await? {
            AdminDbResponse::TemplatesWithChannel(v) => Ok(v),
            other => Err(anyhow::anyhow!("unexpected: {other:?}")),
        }
    }

    async fn list_telegram_channels(&self) -> anyhow::Result<Vec<forge_admin::models::Telegram>> {
        match self.admin(AdminDbCommand::ListTelegrams).await? {
            AdminDbResponse::Telegrams(v) => Ok(v),
            other => Err(anyhow::anyhow!("unexpected: {other:?}")),
        }
    }
}

/// `ErrorReporter` for `forge_admin::hooks::register_error_hooks` — sends
/// errors through the `error` template (the `errors` channel per project
/// configuration). Errors with no template in the database are silently
/// ignored (see forge-notifier
/// safe-fallback).
#[async_trait::async_trait]
impl forge_admin::hooks::ErrorReporter for NotifierClient {
    async fn report_error(&self, source: String, text: String) {
        let payload = serde_json::json!({ "source": source, "text": text });
        if let Err(e) = self.send("error", &payload).await {
            tracing::error!(error = %e, "error reporter: send failed");
        }
    }
}

/// `ChannelSender` for `forge_admin::hooks::set_channel_sender` — raw text
/// delivery to a channel by `tlg_code` (the "Test" button on admin templates):
/// it sends the template body as is, with no Tera render.
#[async_trait::async_trait]
impl forge_admin::hooks::ChannelSender for NotifierClient {
    async fn send_raw(
        &self,
        tlg_code: String,
        text: String,
        parse_mode: String,
    ) -> Result<(), String> {
        match self
            .inner
            .execute(forge_notifier::NotifierCommand::SendRaw {
                target: forge_notifier::AskTarget::Code(tlg_code),
                text,
                parse_mode,
            })
            .await
        {
            Ok(forge_notifier::NotifierResponse::Queued) => Ok(()),
            Ok(other) => Err(format!("unexpected: {other:?}")),
            Err(e) => Err(e.to_string()),
        }
    }
}
