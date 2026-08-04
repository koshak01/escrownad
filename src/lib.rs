//! escrownad — эталонный каркас приложения на forge.
//!
//! 4 бинарника (`database` / `notifier` / `ws` / `web`) делят:
//!   - пути к unix-сокетам — `sockets::*`
//!   - типы команд к database — `DbCommand` / `DbResponse`
//!   - общий ws-контекст — `AppContext` через `init_app_context()`/`app_context()`
//!
//! Системные SQL (ping, now_epoch, constants) и PG-подключение живут в ядре
//! (`forge_db::pg`, `forge_db::system`). Ядерные admin-страницы — в `forge-admin`.
//! Telegram-очередь и шаблоны — в `forge-notifier`.
//!
//! Здесь — только базовый каркас, минимально необходимый чтобы стек поднялся
//! end-to-end. Доменные модели / команды / страницы добавляются проектом
//! поверх этого минимума.

pub mod chain;
pub mod models;
pub mod observer;
pub mod pages;
pub mod wallet_auth;
pub mod ws_handlers;

use std::sync::{Arc, OnceLock};

use forge_admin::{AdminDbCommand, AdminDbResponse};
use forge_session::RedisClient;
use forge_ws::{Page, Renderer};
use tokio::sync::RwLock;

pub const APP_NAME: &str = "escrownad";

/// Внутренние IPC-сокеты между бинарями (database/notifier/ws/web).
/// Имя через `.` — конвенция forge для внутреннего IPC.
/// Внешние listener'ы (nginx → бинарь) идут через `_`:
/// `/tmp/escrownad_ws.sock`, `/tmp/escrownad_web.sock` — настраиваются в etc/ws.toml
/// и etc/web.toml, не здесь.
pub mod sockets {
    pub const DATABASE: &str = "/tmp/escrownad.database.sock";
    pub const NOTIFIER: &str = "/tmp/escrownad.notifier.sock";
    pub const WS: &str = "/tmp/escrownad.ws.sock";
    pub const WEB: &str = "/tmp/escrownad.web.sock";
}

// ──────────────────────────────────────────────────────────────────────────────
// AppContext — общий контекст ws-демона
// ──────────────────────────────────────────────────────────────────────────────

/// Контейнер для всего что нужно ws-handler'ам: клиент к database,
/// Redis-сессии, Tera renderer, клиент к notifier'у.
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
        .expect("AppContext not initialized — вызови init_app_context() при старте ws")
}

/// Registry страниц: проектные сначала (могут перекрывать `/admin/` если
/// захотят свою заглушку), потом ядерные admin-страницы.
pub fn pages() -> Vec<Box<dyn Page>> {
    let mut out: Vec<Box<dyn Page>> = vec![
        Box::new(pages::IndexPage),
        Box::new(pages::AboutPage),
        Box::new(pages::CabinetPage),
        Box::new(pages::DealsListPage),
        Box::new(pages::DealNewPage),
        Box::new(pages::DealShowPage),
        Box::new(pages::OraclePage),
        // ЭТАЛОН: доменные admin-страницы проекта (demo-сущность).
        // Порядок list → new → edit: `/admin/demos/new/` обязан идти ДО
        // `/admin/demos/{id}/`, иначе `new` попадёт в `{id}` (см. page.rs::matches).
        Box::new(pages::admin::demos::list::ListPage),
        Box::new(pages::admin::demos::new::NewPage),
        Box::new(pages::admin::demos::edit::EditPage),
    ];
    out.extend(forge_admin::pages::all());
    out
}

// ──────────────────────────────────────────────────────────────────────────────
// IPC: команды ws/web/notifier → database
// ──────────────────────────────────────────────────────────────────────────────

/// Минимальный набор команд к database. Системные (Ping/GetSalt/...) и
/// ядерные admin-CRUD — всё что нужно для запуска. Проектные команды
/// добавляются поверх (новые варианты enum + ветки в `database.rs`).
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

    /// Все ядерные admin-CRUD команды — в одном wrapper'е.
    /// Расширяется автоматически при добавлении вариантов в forge-admin.
    /// Login (find user by email) тоже через admin — см.
    /// `AdminDbCommand::GetUserByEmail` + `forge_admin::ws_auth::login`.
    Admin {
        cmd: AdminDbCommand,
    },

    // ── ЭТАЛОН доменных команд (demo-сущность `demos`) ──────────────────────
    // Так проект добавляет CRUD своей таблицы: варианты в СВОЙ `DbCommand`
    // (не в `AdminDbCommand`), обработка — в `bin/database.rs` (см. `match`).
    // Структура путешествует целиком (`03_handlers.md`): `data: Demo`, не россыпь.
    /// Список demo с фильтром + сортировкой (structure-driven, ADR-0010).
    ListDemosFiltered {
        filter: models::DemoListFilter,
        sort: Option<forge_admin::handlers::SortSpec>,
    },
    /// Одна запись по id — для формы редактирования.
    GetDemo {
        id: i64,
    },
    /// Upsert: `dmo_id == 0` → insert, иначе update.
    SaveDemo {
        data: models::Demo,
    },
    /// Удаление по id.
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
    SaveDeal {
        data: models::Deal,
    },

    /// Find or create forge user for lowercase 0x EVM address (wallet login).
    WalletFindOrCreate {
        address: String,
    },
    WalletAddressForUser {
        usr_id: i64,
    },
    ListVerifiedSellers,
}

// large_enum_variant: `Admin` несёт весь AdminDbResponse (большой по дизайну) —
// как и `DbCommand::Admin`. Это IPC-wrapper, который сразу сериализуется в
// msgpack и дропается; боксить ради memory-layout смысла нет (+ heap-alloc на
// каждый ответ). Решение симметрично `DbCommand` выше — allow, не Box.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum DbResponse {
    Pong,
    Salt(i64),
    Constants(Constants),
    Admin(AdminDbResponse),
    // ── ответы demo-сущности (эталон) ──────────────────────────────────────
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
// Constants — снимок ядерных констант, кешируется в database
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
    pub fn get<T: serde::de::DeserializeOwned>(&self, key: &str) -> Result<Option<T>, serde_json::Error> {
        match self.0.get(key) {
            Some(bytes) => Ok(Some(serde_json::from_slice(bytes)?)),
            None => Ok(None),
        }
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Typed клиенты
// ──────────────────────────────────────────────────────────────────────────────

forge_ipc::client! {
    /// Клиент к escrownad-database. Единственный путь к Postgres-данным.
    pub DbClient(DbCommand, DbResponse, sockets::DATABASE) {
        pub fn ping() = Ping -> Pong;
        pub fn get_salt() -> i64 = GetSalt -> Salt(v) = v;
        pub fn get_constants() -> Constants = GetConstants -> Constants(v) = v;
        pub fn update_constant(key: String, value: Vec<u8>)
            = UpdateConstant { key, value } -> Ok;
        pub fn reload_constant(code: Option<String>)
            = ReloadConstant { code } -> Ok;

        /// Один wrapper-метод на ВСЕ admin-CRUD команды forge-admin.
        /// Расширяется автоматически — приложение не правит.
        pub fn admin(cmd: AdminDbCommand) -> AdminDbResponse
            = Admin { cmd } -> Admin(resp) = resp;

        // ── ЭТАЛОН: доменные методы demo-сущности ──────────────────────────
        // По ним Page/ws-handler ходят в БД: `app_context().db.list_demos_filtered(...)`.
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
        pub fn save_deal(data: models::Deal) = SaveDeal { data } -> Ok;

        /// Find-or-create user by EVM wallet address (lowercase 0x…).
        pub fn wallet_find_or_create(address: String) -> wallet_auth::WalletUserRow
            = WalletFindOrCreate { address } -> WalletUser(v) = v;
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

/// `ConstantsSink` для `forge_admin::hooks::DefaultAdminHooks` —
/// делегирует в client!-сгенерированные методы DbClient.
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
    /// Блокирующее ожидание готовности database (для bootstrap зависимых демонов).
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
// WsClient — клиент к escrownad-ws для web-бинарника (IPC render).
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
// NotifierClient — клиент IPC от ws/web в notifier.
// Использует протокол из forge_notifier (NotifierCommand/Response).
// ──────────────────────────────────────────────────────────────────────────────

forge_ipc::client! {
    /// Клиент к escrownad-notifier для постановки сообщений в очередь.
    pub NotifierClient(forge_notifier::NotifierCommand, forge_notifier::NotifierResponse, sockets::NOTIFIER) {
        pub fn ping() = Ping -> Pong;
    }
}

impl NotifierClient {
    /// Поставить сообщение в очередь по шаблону `tpl_code`. `params` —
    /// контекст для Tera-рендера, обычно `serde_json!({...})`.
    pub async fn send<P: serde::Serialize>(
        &self,
        tpl_code: impl Into<String>,
        params: &P,
    ) -> forge_ipc::IpcResult<()> {
        let bytes = serde_json::to_vec(params).map_err(|e| forge_ipc::IpcError::Encode(e.to_string()))?;
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
            other => Err(forge_ipc::IpcError::Server(format!("unexpected: {other:?}"))),
        }
    }
}

/// Реализация `forge_notifier::NotifierDb` для `DbClient`. Notifier работает
/// с любым приложением через этот trait, не зная про конкретный DbCommand.
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

    async fn list_templates_with_channel(&self) -> anyhow::Result<Vec<forge_admin::TemplateWithChannel>> {
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

/// `ErrorReporter` для `forge_admin::hooks::register_error_hooks` —
/// шлёт ошибки в шаблон `error` (канал `errors` по проектной конфигурации).
/// Errors без шаблона в БД молча игнорируются (см. forge-notifier
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

/// `ChannelSender` для `forge_admin::hooks::set_channel_sender` — «сырая»
/// отправка текста в канал по `tlg_code` (кнопка «Проверить» в admin-шаблонах):
/// шлёт тело шаблона как есть, без Tera-рендера.
#[async_trait::async_trait]
impl forge_admin::hooks::ChannelSender for NotifierClient {
    async fn send_raw(&self, tlg_code: String, text: String, parse_mode: String) -> Result<(), String> {
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
