//! escrownad-ws — two listeners in one binary:
//!   1. **IPC** (`/tmp/escrownad.ws.sock`) — takes a `RenderRequest` from web
//!      and renders HTML through Tera. Session, cookie, auth and page dispatch
//!      all live here.
//!   2. **TCP WS** (or a unix socket behind nginx) — the websocket gateway for
//!      the browser. Msgpack protocol, per-connection identity, at the URL
//!      `/ws/{session_id}/`.
//!
//! Both share one state: Renderer + DbClient + RedisClient. It never touches
//! Postgres directly — everything goes through DbClient over IPC to
//! escrownad-database. Redis is reached directly over a unix socket.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use axum::Router;
use axum::routing::get;
use escrownad::wallet_auth::{
    ChallengeParams, ChallengeResp, WalletChallenges, WalletLoginParams, WalletLoginResp,
};
use escrownad::ws_handlers::echo::EchoParams;
use escrownad::ws_handlers::{
    DealActionParams, DealFundedParams, DealSaveParams, DealSearchParams, DemoDeleteParams,
    DemoSaveParams,
};
use escrownad::{AppContext, DbClient, NotifierClient, init_app_context, sockets};
use forge_ipc::{CommandHandler, serve_ipc};
use forge_session::RedisClient;
// ──────────────────────────────────────────────────────────────────────────────
// Config
// ──────────────────────────────────────────────────────────────────────────────

// Configs (WsConfig + RedisConfig) and per-connection state (ForgeWsConn) are
// platform-level and live in forge_ws::bootstrap. Here they are only used.
use forge_ws::bootstrap::{ForgeWsConn as WsConn, RedisConfig as RedisToml, WsConfig as WsToml};
use forge_ws::wsgate::{Hub, WsAppState, WsConnExt};
use forge_ws::{ActionResp, GlobalContext, RenderRequest, RenderResponse, Renderer};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tracing::{error, info};

#[derive(Clone)]
#[allow(dead_code)] // db / notifier / hub — kept for handlers in real projects
struct WsState {
    renderer: Arc<RwLock<Renderer>>,
    db: Arc<DbClient>,
    redis: RedisClient,
    notifier: Arc<NotifierClient>,
    hub: Arc<Hub>,
    /// Pending EVM personal_sign challenges (wallet login).
    wallet_challenges: Arc<WalletChallenges>,
}

// ──────────────────────────────────────────────────────────────────────────────
// AdminHooks — reactions to admin-area changes (constants reload).
// ──────────────────────────────────────────────────────────────────────────────

// AppHooks is now `forge_admin::hooks::DefaultAdminHooks`. No boilerplate is
// left in bin/ws.rs; DbClient implements ConstantsSink in lib.rs.

// ──────────────────────────────────────────────────────────────────────────────
// IPC render — arrives from web. Session, cookie, auth and dispatch live here.
// ──────────────────────────────────────────────────────────────────────────────

// IPC render-dispatcher: cookie → session → pages-router → 500/404 fallback.
// All of the logic sits in forge_admin::render::dispatch_render.
impl CommandHandler<RenderRequest, RenderResponse> for WsState {
    async fn handle(&self, req: RenderRequest) -> Result<RenderResponse, String> {
        forge_admin::render::dispatch_render(req, &self.renderer, &self.redis, &escrownad::pages())
            .await
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// WS handlers — implemented through the forge-admin macro. A reference minimum.
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
        // Every command declares its auth: `| AuthRequirement::...`. Without a
        // declaration the default is Authenticated — safe by default. Public
        // (anonymous) commands are marked explicitly.
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
        // open_modal is public — an anonymous visitor opens the login dialog with it.
        async fn open_modal(&self, conn: &_, params: forge_admin::ws_modals::OpenModalParams)
            -> forge_admin::ws_modals::OpenModalResp
            | forge_ws::AuthRequirement::Public;
        async fn combobox_search(&self, conn: &_, params: forge_admin::ws_combobox::ComboboxSearchParams)
            -> ActionResp
            | forge_ws::AuthRequirement::Authenticated;
        async fn combobox_chips_batch(&self, conn: &_, params: forge_admin::ws_combobox::ComboboxChipsBatchParams)
            -> ActionResp
            | forge_ws::AuthRequirement::Authenticated;
        // Generic inline bool toggle in any list row. It needs the renderer, so
        // it is declared explicitly, like combobox. ONE command for every list.
        async fn inline_toggle(&self, conn: &_, params: forge_admin::ws_handlers::InlineToggleParams)
            -> ActionResp
            | forge_ws::AuthRequirement::Roles(&["manager", "admin"]);
        // Demonstrates WS-pushed HTML: takes text and returns a server-rendered
        // partial that replaces `#echo-result` in place.
        async fn echo      (&self, conn: &_, params: EchoParams) -> ActionResp
            | forge_ws::AuthRequirement::Public;
        // Reference: save/delete of the demo entity. A project declares its own
        // CRUD commands here, as with echo, plus a delegate in impl WsState
        // below. Auth matches the platform admin CRUD: Roles(manager/admin).
        async fn demos_save  (&self, _conn: &_, params: DemoSaveParams) -> ActionResp
            | forge_ws::AuthRequirement::Roles(&["manager", "admin"]);
        async fn demos_delete(&self, _conn: &_, params: DemoDeleteParams) -> ActionResp
            | forge_ws::AuthRequirement::Roles(&["manager", "admin"]);
        // Deal flow: wallet session required for mutations.
        async fn deals_save   (&self, conn: &_, params: DealSaveParams) -> ActionResp
            | forge_ws::AuthRequirement::Authenticated;
        async fn deals_action (&self, conn: &_, params: DealActionParams) -> ActionResp
            | forge_ws::AuthRequirement::Authenticated;
        // Board search: still Public at the WS layer so the page can call it,
        // but the handler returns empty rows without a CVI wallet (see market_access).
        async fn deals_search (&self, conn: &_, params: DealSearchParams) -> ActionResp
            | forge_ws::AuthRequirement::Public;
        // The buyer funded the lock — the server verifies the fact on chain.
        async fn deals_funded (&self, conn: &_, params: DealFundedParams) -> ActionResp
            | forge_ws::AuthRequirement::Authenticated;
        // EVM wallet login (MetaMask / personal_sign). Public — anon flow.
        async fn wallet_challenge(&self, conn: &_, params: ChallengeParams) -> ChallengeResp
            | forge_ws::AuthRequirement::Public;
        async fn wallet_login(&self, conn: &_, params: WalletLoginParams) -> WalletLoginResp
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
    /// Sign-in — a thin wrapper over the platform's `forge_admin::ws_auth::login`.
    /// Project-specific additions (merging a guest cart into the user's, say)
    /// would go here; for now it is a plain platform call.
    async fn login(
        &self,
        conn: &WsConn,
        p: forge_admin::ws_auth::LoginParams,
    ) -> Result<forge_admin::ws_auth::LoginResp, String> {
        // The public product UI is English only, while the platform returns its
        // errors in Russian. This is the one boundary where we translate them,
        // and matching on the text is unavoidable: the platform hands us a
        // string and nothing else. The mapping lives here alone — no `contains`
        // on error text anywhere downstream.
        forge_admin::ws_auth::login(&self.redis, conn, p)
            .await
            .map_err(|e| {
                if e.contains("Неверный") || e.contains("email") && e.contains("парол")
                {
                    "Invalid email or password".into()
                } else if e.contains("отключена") {
                    "Account disabled".into()
                } else {
                    e
                }
            })
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
        // The language comes from the tab's route (`conn.lang()`), never from
        // the payload — a client could send anything. For a single-language
        // project this is always the default.
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
        // auth is handled by the per-command gate — | Roles(manager/admin) above.
        forge_admin::ws_handlers::inline_toggle(&self.renderer, p).await
    }
    async fn echo(&self, _: &WsConn, p: EchoParams) -> Result<ActionResp, String> {
        escrownad::ws_handlers::echo(p).await
    }
    /// Reference: save/delete of the demo entity delegated to project ws handlers.
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
    async fn deals_search(&self, conn: &WsConn, p: DealSearchParams) -> Result<ActionResp, String> {
        escrownad::ws_handlers::deals_search(p, conn.usr_id()).await
    }
    async fn deals_funded(&self, conn: &WsConn, p: DealFundedParams) -> Result<ActionResp, String> {
        escrownad::ws_handlers::deals_funded(p, conn.usr_id()).await
    }
    async fn wallet_challenge(
        &self,
        conn: &WsConn,
        p: ChallengeParams,
    ) -> Result<ChallengeResp, String> {
        escrownad::wallet_auth::wallet_challenge(&self.wallet_challenges, conn, p).await
    }
    async fn wallet_login(
        &self,
        conn: &WsConn,
        p: WalletLoginParams,
    ) -> Result<WalletLoginResp, String> {
        escrownad::wallet_auth::wallet_login(&self.wallet_challenges, &self.redis, conn, p).await
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// WS route /ws/{session_id}/ — a wrapper over wsgate::ws_route capturing the session.
// ──────────────────────────────────────────────────────────────────────────────

// ws route /ws/{session_id}/ — platform-level, forge_ws::bootstrap::ws_route_with_session.

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

    // forge-admin Pages work through AdminEnv — register the default
    // implementation over our DbClient, which implements AdminCommandSender.
    forge_admin::init(Arc::new(forge_admin::DefaultAdminEnv::new(db.clone())));

    let salt = db.get_salt().await.context("get_salt")?;
    let constants = db.get_constants().await.context("get_constants")?;
    let mut tera_constants = HashMap::new();
    for key in constants.keys() {
        if let Ok(Some(v)) = constants.get::<serde_json::Value>(key) {
            tera_constants.insert(key.clone(), v);
        }
    }

    // Mini App: the bot token validating Telegram initData (identifying a user
    // in /miniapp/*, ws command mini_auth). Taken from constants.telegrams.token;
    // no token — mini_auth returns a clear error.
    if let Some(token) = tera_constants
        .get("telegrams")
        .and_then(|v| v.get("token"))
        .and_then(|v| v.as_str())
    {
        forge_admin::set_mini_app_bot_token(token);
    }

    // A builder rather than a literal: `GlobalContext` is `#[non_exhaustive]`,
    // so a new field in the platform does not break every project at once.
    //
    // A single-language build loads no translations — `lang::init` is never
    // called. A multilingual project appends
    // `.with_translations(db.warm_translations().await?)`.
    let global = GlobalContext::new(salt.to_string(), cfg.ws.env, cfg.ws.ws_url)
        .with_constants(tera_constants);
    info!(salt = %global.salt, constants = global.constants.len(), env = ?global.env,
          ws_url = %global.ws_url, "global context loaded");

    // Templates: forge core + project templates/.
    let renderer = Renderer::with_roots(&["../forge/templates", "templates"], Arc::new(global))
        .context("renderer init")?;
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

    // Automatic error alerts to Telegram. System errors — bugs, render
    // failures, panicking ws handlers — go through the `error` template into
    // the `errors` channel. User errors (form validation, NeedLogin, Forbidden)
    // are NOT sent: they are the expected input flow, not defects. Without this
    // line errors live only in the logs, where nobody sees them in production.
    forge_admin::hooks::register_error_hooks(notifier.clone());

    // Sink for the "Test" button on the admin template form — raw delivery of
    // the template body to its channel, with no Tera render, via notifier.
    forge_admin::hooks::set_channel_sender(notifier.clone());

    let state = WsState {
        renderer: renderer_arc,
        db: db.clone(),
        redis,
        notifier,
        hub: hub.clone(),
        wallet_challenges: WalletChallenges::new(),
    };

    // A builder rather than a literal: new platform options land quietly,
    // without editing thirteen projects.
    let ws_app = WsAppState::new(Arc::new(state.clone()), hub.clone(), state.redis.clone());
    // Native clients (`/ws-app/{usr_hash}/`) — uncomment together with the
    // route below. Keys are issued on the user's card in the admin area.
    // let ws_app = ws_app.with_app_auth(Arc::new(forge_admin::app_keys::DbAppAuth));
    let ws_mode = cfg.ws.mode.clone();
    let ws_port = cfg.ws.ws_port;
    let ws_sock = cfg.ws.socket_path.clone();
    tokio::spawn(async move {
        // The base route — the UI WebSocket with session-based identity.
        // A project that wants to accept peer-tool commands adds the extra
        // `/peer-tool/{usr_hash}/` route below and puts an assembled
        // `PeerWs { validator, registry }` into `WsAppState::peer_ws`.
        // The default is UI-only, `peer_ws: None`.
        let app = Router::new()
            .route(
                "/ws/{session_id}/",
                get(forge_ws::bootstrap::ws_route_with_session::<WsState>),
            )
            // The Telegram webhook: an operator presses Approve or Decline on
            // a listing card. The path is fixed and the secret travels in a
            // header — in the URL it would leak into nginx's access_log.
            .route("/tg/hook", axum::routing::post(escrownad::tg_hook::handle))
            // .route("/peer-tool/{usr_hash}/", get(forge_ws::peer_ws::ws_route_peer::<WsState>))
            // A native client entering the SAME channel: a key instead of a cookie.
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
        info!(
            ?serve_mode,
            "ws gateway listening (path: /ws/{{session_id}}/)"
        );
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
