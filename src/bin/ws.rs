//! escrownad-ws — два слушателя в одном бинарнике:
//!   1. **IPC** (`/tmp/escrownad.ws.sock`) — принимает `RenderRequest` от web,
//!      рендерит HTML через Tera. Логика session/cookie/auth/page-dispatch.
//!   2. **TCP WS** (или unix-сокет за nginx) — websocket-gateway для браузера.
//!      msgpack-протокол, per-connection identity. URL `/ws/{session_id}/`.
//!
//! Обе функции делят общий state (Renderer + DbClient + RedisClient).
//! Прямо в Postgres не лезет — всё через DbClient (IPC к escrownad-database).
//! Redis — напрямую через unix-socket (forge-session).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::Router;
use axum::routing::get;
use forge_ipc::{CommandHandler, serve_ipc};
use forge_session::RedisClient;
use escrownad::ws_handlers::echo::EchoParams;
use escrownad::ws_handlers::{
    DealActionParams, DealSaveParams, DemoDeleteParams, DemoSaveParams,
};
use escrownad::{AppContext, DbClient, NotifierClient, init_app_context, sockets};
// ──────────────────────────────────────────────────────────────────────────────
// Config
// ──────────────────────────────────────────────────────────────────────────────

// Configs (WsConfig + RedisConfig) и per-connection state (ForgeWsConn) —
// ядерные, вынесены в forge_ws::bootstrap. Здесь только использование.
use forge_ws::bootstrap::{ForgeWsConn as WsConn, RedisConfig as RedisToml, WsConfig as WsToml};
use forge_ws::wsgate::{Hub, WsAppState, WsConnExt};
use forge_ws::{ActionResp, GlobalContext, RenderRequest, RenderResponse, Renderer};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{error, info};

#[derive(Clone)]
#[allow(dead_code)] // db / notifier / hub — задел: реальные проекты используют в handlers
struct WsState {
    renderer: Arc<RwLock<Renderer>>,
    db: Arc<DbClient>,
    redis: RedisClient,
    notifier: Arc<NotifierClient>,
    hub: Arc<Hub>,
}

// ──────────────────────────────────────────────────────────────────────────────
// AdminHooks — реакции на изменения в админке (constants reload).
// ──────────────────────────────────────────────────────────────────────────────

// AppHooks — теперь `forge_admin::hooks::DefaultAdminHooks` (см. Phase 1.1.E).
// Boilerplate в bin/ws.rs больше нет; DbClient реализует ConstantsSink в lib.rs.

// ──────────────────────────────────────────────────────────────────────────────
// IPC render — приходит от web. Здесь живёт session/cookie/auth/dispatch.
// ──────────────────────────────────────────────────────────────────────────────

// IPC render-dispatcher: cookie → session → pages-router → 500/404 fallback.
// Вся логика в forge_admin::render::dispatch_render (Phase 1.1.F).
impl CommandHandler<RenderRequest, RenderResponse> for WsState {
    async fn handle(&self, req: RenderRequest) -> Result<RenderResponse, String> {
        forge_admin::render::dispatch_render(req, &self.renderer, &self.redis, &escrownad::pages()).await
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// WS-handlers — реализация через forge-admin макрос. Минимум для эталона.
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct SetRouteParams {
    route: String,
}
#[derive(Serialize)]
struct OkResp {
    ok: bool,
}
#[derive(Serialize)]
struct PongResp {
    pong: bool,
}

forge_admin::wsgate_handler_with_admin! {
    impl WsHandler for WsState as WsConn {
        // Каждая команда объявляет auth: `| AuthRequirement::...`. Без объявления
        // default — Authenticated (safe-by-default). Публичные (анон) помечены явно.
        async fn set_route (&self, conn: &_, params: SetRouteParams) -> OkResp
            | forge_ws::AuthRequirement::Public;
        async fn ping      (&self, conn: &_, params: ()) -> PongResp
            | forge_ws::AuthRequirement::Public;
        async fn login     (&self, conn: &_, params: forge_admin::ws_auth::LoginParams)
            -> forge_admin::ws_auth::LoginResp
            | forge_ws::AuthRequirement::Public;
        async fn logout    (&self, conn: &_, params: forge_admin::ws_auth::LogoutParams)
            -> ActionResp
            | forge_ws::AuthRequirement::Authenticated;
        // open_modal публичен — через него аноним открывает модалку логина.
        async fn open_modal(&self, conn: &_, params: forge_admin::ws_modals::OpenModalParams)
            -> forge_admin::ws_modals::OpenModalResp
            | forge_ws::AuthRequirement::Public;
        async fn combobox_search(&self, conn: &_, params: forge_admin::ws_combobox::ComboboxSearchParams)
            -> ActionResp
            | forge_ws::AuthRequirement::Authenticated;
        async fn combobox_chips_batch(&self, conn: &_, params: forge_admin::ws_combobox::ComboboxChipsBatchParams)
            -> ActionResp
            | forge_ws::AuthRequirement::Authenticated;
        // generic inline-toggle bool в строке любого списка (нужен renderer →
        // объявляем явно, как combobox). ОДНА команда на все ядерные списки.
        async fn inline_toggle(&self, conn: &_, params: forge_admin::ws_handlers::InlineToggleParams)
            -> ActionResp
            | forge_ws::AuthRequirement::Roles(&["manager", "admin"]);
        // Демо WS-push HTML — см. §7.5 CONVENTIONS. Принимает текст,
        // отдаёт server-rendered partial для in-place замены `#echo-result`.
        async fn echo      (&self, conn: &_, params: EchoParams) -> ActionResp
            | forge_ws::AuthRequirement::Public;
        // ЭТАЛОН: save/delete доменной demo-сущности. Проектные CRUD-команды
        // объявляются здесь (как echo) + делегат в impl WsState ниже.
        // Auth — как у ядерных admin CRUD: Roles(manager/admin).
        async fn demos_save  (&self, _conn: &_, params: DemoSaveParams) -> ActionResp
            | forge_ws::AuthRequirement::Roles(&["manager", "admin"]);
        async fn demos_delete(&self, _conn: &_, params: DemoDeleteParams) -> ActionResp
            | forge_ws::AuthRequirement::Roles(&["manager", "admin"]);
        // Deal flow (cabinet): public for demo; login binds seller/buyer usr_id.
        async fn deals_save   (&self, conn: &_, params: DealSaveParams) -> ActionResp
            | forge_ws::AuthRequirement::Public;
        async fn deals_action (&self, conn: &_, params: DealActionParams) -> ActionResp
            | forge_ws::AuthRequirement::Public;
    }
}

impl WsState {
    async fn set_route(&self, conn: &WsConn, p: SetRouteParams) -> Result<OkResp, String> {
        conn.set_route(Some(p.route));
        Ok(OkResp { ok: true })
    }
    async fn ping(&self, _: &WsConn, _: ()) -> Result<PongResp, String> {
        Ok(PongResp { pong: true })
    }
    /// Логин — тонкая обёртка над ядерным `forge_admin::ws_auth::login`.
    /// Проектные надстройки (например merge гостевой корзины в юзерскую)
    /// добавляются здесь — пока пусто, чистый ядерный вызов.
    async fn login(
        &self,
        conn: &WsConn,
        p: forge_admin::ws_auth::LoginParams,
    ) -> Result<forge_admin::ws_auth::LoginResp, String> {
        forge_admin::ws_auth::login(&self.redis, conn, p).await
    }
    async fn logout(
        &self,
        conn: &WsConn,
        p: forge_admin::ws_auth::LogoutParams,
    ) -> Result<ActionResp, String> {
        forge_admin::ws_auth::logout(&self.redis, conn, p).await
    }
    async fn open_modal(
        &self,
        _conn: &WsConn,
        p: forge_admin::ws_modals::OpenModalParams,
    ) -> Result<forge_admin::ws_modals::OpenModalResp, String> {
        forge_admin::ws_modals::open_modal(&self.renderer, p).await
    }
    async fn combobox_search(
        &self,
        conn: &WsConn,
        p: forge_admin::ws_combobox::ComboboxSearchParams,
    ) -> Result<ActionResp, String> {
        // Язык — из route вкладки (`conn.lang()`), а не из payload: клиент
        // мог бы прислать любой. Одноязычному проекту это всегда дефолт.
        forge_admin::ws_combobox::combobox_search(&self.renderer, p, conn.lang()).await
    }
    async fn combobox_chips_batch(
        &self,
        _conn: &WsConn,
        p: forge_admin::ws_combobox::ComboboxChipsBatchParams,
    ) -> Result<ActionResp, String> {
        forge_admin::ws_combobox::combobox_chips_batch(&self.renderer, p).await
    }
    async fn inline_toggle(
        &self,
        _conn: &WsConn,
        p: forge_admin::ws_handlers::InlineToggleParams,
    ) -> Result<ActionResp, String> {
        // auth — через per-command гейт (| Roles(manager/admin) в объявлении выше).
        forge_admin::ws_handlers::inline_toggle(&self.renderer, p).await
    }
    async fn echo(&self, _: &WsConn, p: EchoParams) -> Result<ActionResp, String> {
        escrownad::ws_handlers::echo(p).await
    }
    /// ЭТАЛОН: делегаты save/delete demo-сущности в проектные ws-handler'ы.
    async fn demos_save(&self, _: &WsConn, p: DemoSaveParams) -> Result<ActionResp, String> {
        escrownad::ws_handlers::demos_save(p).await
    }
    async fn demos_delete(&self, _: &WsConn, p: DemoDeleteParams) -> Result<ActionResp, String> {
        escrownad::ws_handlers::demos_delete(p).await
    }
    async fn deals_save(&self, conn: &WsConn, p: DealSaveParams) -> Result<ActionResp, String> {
        escrownad::ws_handlers::deals_save(p, conn.usr_id()).await
    }
    async fn deals_action(&self, conn: &WsConn, p: DealActionParams) -> Result<ActionResp, String> {
        escrownad::ws_handlers::deals_action(p, conn.usr_id()).await
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// WS-route /ws/{session_id}/ — обёртка над wsgate::ws_route с захватом session.
// ──────────────────────────────────────────────────────────────────────────────

// ws-route /ws/{session_id}/ — ядерный, forge_ws::bootstrap::ws_route_with_session.

// ──────────────────────────────────────────────────────────────────────────────
// Bootstrap
// ──────────────────────────────────────────────────────────────────────────────

async fn run() -> Result<()> {
    info!("starting");

    let cfg: WsToml = forge_core::config::load_toml("etc/ws.toml").context("load etc/ws.toml")?;
    let redis_cfg: RedisToml =
        forge_core::config::load_toml("etc/redis.toml").context("load etc/redis.toml")?;

    let db = Arc::new(DbClient::spawn());
    db.wait_ready(Duration::from_millis(cfg.ws.db_wait_ms))
        .await
        .context("escrownad-database not ready")?;
    info!("database ready");

    let redis = RedisClient::connect(&redis_cfg.redis.socket_path)
        .await
        .context("connect redis")?;

    // forge-admin Pages работают через AdminEnv — регистрируем дефолтную
    // реализацию поверх нашего DbClient (он сам реализует AdminCommandSender).
    forge_admin::init(Arc::new(forge_admin::DefaultAdminEnv::new(db.clone())));

    let salt = db.get_salt().await.context("get_salt")?;
    let constants = db.get_constants().await.context("get_constants")?;
    let mut tera_constants = HashMap::new();
    for key in constants.keys() {
        if let Ok(Some(v)) = constants.get::<serde_json::Value>(key) {
            tera_constants.insert(key.clone(), v);
        }
    }

    // Mini App: bot-token для валидации Telegram initData (опознание юзера в
    // /miniapp/*, ws-команда mini_auth). Берём из constants.telegrams.token;
    // нет токена — mini_auth вернёт понятную ошибку.
    if let Some(token) = tera_constants
        .get("telegrams")
        .and_then(|v| v.get("token"))
        .and_then(|v| v.as_str())
    {
        forge_admin::set_mini_app_bot_token(token);
    }

    // Билдер, не литерал: `GlobalContext` помечен `#[non_exhaustive]`, чтобы
    // новое поле в ядре не ломало сборку всем проектам разом.
    //
    // Одноязычный эталон переводы не грузит — `lang::init` не звали.
    // Многоязычный проект дописывает `.with_translations(db.warm_translations().await?)`.
    let global =
        GlobalContext::new(salt.to_string(), cfg.ws.env, cfg.ws.ws_url).with_constants(tera_constants);
    info!(salt = %global.salt, constants = global.constants.len(), env = ?global.env,
          ws_url = %global.ws_url, "global context loaded");

    // Templates: forge core + project templates/.
    let renderer =
        Renderer::with_roots(&["../forge/templates", "templates"], Arc::new(global)).context("renderer init")?;
    let hub = Hub::new();
    let notifier = Arc::new(NotifierClient::spawn());
    let renderer_arc = Arc::new(RwLock::new(renderer));

    
    init_app_context(AppContext {
        db: db.clone(),
        redis: redis.clone(),
        renderer: renderer_arc.clone(),
        notifier: notifier.clone(),
    });

    forge_admin::init_hooks(Arc::new(forge_admin::hooks::DefaultAdminHooks::new(
        db.clone(),
        renderer_arc.clone(),
    )));

    // Авто-алерты ошибок в Telegram. System-ошибки (баги, render-фейлы,
    // упавшие ws-handler'ы) уходят в шаблон `error` → канал `errors`.
    // User-ошибки (валидация формы, NeedLogin, Forbidden) НЕ шлются — это
    // ожидаемый input-flow, не баг. Без этой строки ошибки видны только в
    // логах — на проде о них не узнаешь.
    forge_admin::hooks::register_error_hooks(notifier.clone());

    // Sink для кнопки «Проверить» в admin-форме шаблонов — сырая отправка тела
    // шаблона в его канал (без Tera-рендера) через notifier.
    forge_admin::hooks::set_channel_sender(notifier.clone());

    let state = WsState {
        renderer: renderer_arc,
        db: db.clone(),
        redis,
        notifier,
        hub: hub.clone(),
    };

    // Билдер, а не литерал: новые опции ядра добавляются молча, без правки
    // тринадцати проектов (см. CHANGELOG 2.94.0).
    let ws_app = WsAppState::new(Arc::new(state.clone()), hub.clone(), state.redis.clone());
    // Нативные клиенты (`/ws-app/{usr_hash}/`) — раскомментировать вместе с
    // роутом ниже. Ключи выдаются на карточке пользователя в админке.
    // let ws_app = ws_app.with_app_auth(Arc::new(forge_admin::app_keys::DbAppAuth));
    let ws_mode = cfg.ws.mode.clone();
    let ws_port = cfg.ws.ws_port;
    let ws_sock = cfg.ws.socket_path.clone();
    tokio::spawn(async move {
        // Базовый роут — UI WebSocket'а с session-based identity.
        // Если проект хочет принимать peer-tool команды от synapse-relay'я —
        // подключает дополнительный роут `/peer-tool/{usr_hash}/` (см. ниже) и кладёт
        // в `WsAppState::peer_ws` собранный `PeerWs { validator, registry }`.
        // По умолчанию UI-only — UI-only, `peer_ws: None`.
        let app = Router::new()
            .route(
                "/ws/{session_id}/",
                get(forge_ws::bootstrap::ws_route_with_session::<WsState>),
            )
            // .route("/peer-tool/{usr_hash}/", get(forge_ws::peer_ws::ws_route_peer::<WsState>))
            // Вход нативного клиента в ТОТ ЖЕ канал: ключ вместо куки.
            // .route("/ws-app/{usr_hash}/", get(forge_ws::bootstrap::ws_route_app::<WsState>))
            .with_state(ws_app);

        let serve_mode = match ws_mode.as_str() {
            "tcp" => forge_web::ServeMode::Tcp { port: ws_port },
            "unix" => forge_web::ServeMode::Unix {
                path: ws_sock.clone(),
            },
            other => {
                error!(mode = other, "etc/ws.toml: unknown ws.mode (tcp | unix)");
                return;
            }
        };
        info!(?serve_mode, "ws gateway listening (path: /ws/{{session_id}}/)");
        if let Err(e) = forge_web::serve(app, serve_mode).await {
            error!(error = %e, "ws gateway serve failed");
        }
    });

    serve_ipc::<RenderRequest, RenderResponse, _>(sockets::WS, state)
        .await
        .context("serve ipc")?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let log_path = forge_core::logger::init_with_file("escrownad-ws");
    info!(log = %log_path.display(), "logger initialized");
    if let Err(e) = run().await {
        forge_core::error::log_chain(e.as_ref());
        std::process::exit(1);
    }
}
