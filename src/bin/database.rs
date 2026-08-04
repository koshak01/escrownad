//! escrownad-database — IPC демон. Подключается к Postgres, обслуживает запросы.
//!
//! Системные команды (Ping/GetSalt/GetConstants/UpdateConstant/ReloadConstant)
//! обрабатываются ядерным макросом `forge_db::system_commands!`. Admin-CRUD —
//! делегируется в `forge_admin::ipc::dispatch`. Доменных команд в escrownad нет;
//! проекты добавляют свои варианты `DbCommand` и обрабатывают их здесь.

use std::sync::Arc;

use anyhow::{Context, Result};
use forge_db::{ListFilter, pg};
use forge_ipc::{CommandHandler, serve_ipc};
use escrownad::{Constants, DbCommand, DbResponse, sockets};
use serde::Deserialize;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::RwLock;
use tracing::info;

#[derive(Deserialize)]
struct DbToml {
    postgres: pg::PgConfig,
}

type ConstantsCache = Arc<RwLock<Constants>>;

#[derive(Clone)]
struct DbState {
    pool: PgPool,
    constants: ConstantsCache,
}

impl DbState {
    async fn new(pool: PgPool) -> Result<Self> {
        let map = forge_db::system::warm_constants(&pool)
            .await
            .context("warm constants cache")?;
        info!(count = map.len(), "constants cache warmed");
        Ok(Self {
            pool,
            constants: Arc::new(RwLock::new(Constants(map))),
        })
    }
}

impl CommandHandler<DbCommand, DbResponse> for DbState {
    async fn handle(&self, cmd: DbCommand) -> Result<DbResponse, String> {
        // Системные ветки покрывает forge_db::system_commands! макрос ниже.
        // Если cmd оказалась системной — макрос сам ответит. Иначе возвращает
        // Err(cmd) обратно для дальнейшей обработки.
        let cmd = match self.handle_system_cmd(cmd).await {
            Ok(result) => return result,
            Err(other) => other,
        };
        match cmd {
            DbCommand::Ping
            | DbCommand::GetSalt
            | DbCommand::GetConstants
            | DbCommand::UpdateConstant { .. }
            | DbCommand::ReloadConstant { .. } => {
                unreachable!("system commands handled by forge_db::system_commands!")
            }
            DbCommand::Admin { cmd } => {
                let resp = forge_admin::ipc::dispatch(&self.pool, cmd).await?;
                Ok(DbResponse::Admin(resp))
            }

            // ── ЭТАЛОН: обработка доменных команд demo-сущности ──────────────
            // `list_*_filtered`-«handler» (§4 09_admin_lists): фильтр накладывается
            // на `Demo::query()`, безопасный ORDER BY — через whitelist
            // `order_clause` (колонки зарегистрированы `register_list_model` ниже).
            DbCommand::ListDemosFiltered { filter, sort } => {
                use escrownad::models::Demo;
                let demos = filter
                    .apply(Demo::query())
                    .order(forge_admin::handlers::order_clause("demos", &sort, "dmo_code"))
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(DbResponse::Demos(demos))
            }
            DbCommand::GetDemo { id } => {
                let demo = escrownad::models::Demo::find_by_id(&self.pool, id)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(DbResponse::Demo(demo))
            }
            DbCommand::SaveDemo { mut data } => {
                data.save(&self.pool).await.map_err(|e| e.to_string())?;
                Ok(DbResponse::Ok)
            }
            DbCommand::DeleteDemo { id } => {
                escrownad::models::Demo::delete_by_id(&self.pool, id)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(DbResponse::Ok)
            }

            DbCommand::ListDealsListed => {
                let deals = escrownad::models::Deal::list_open(&self.pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(DbResponse::Deals(deals))
            }
            DbCommand::ListDealsBoard => {
                let deals = escrownad::models::Deal::list_board(&self.pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(DbResponse::Deals(deals))
            }
            DbCommand::ListDealsForUser { usr_id } => {
                let deals = escrownad::models::Deal::list_for_user(&self.pool, usr_id)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(DbResponse::Deals(deals))
            }
            DbCommand::ListDealsFiltered { filter, sort } => {
                use escrownad::models::Deal;
                let deals = filter
                    .apply(Deal::query())
                    .order(forge_admin::handlers::order_clause("deals", &sort, "del_id"))
                    .fetch_all(&self.pool)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(DbResponse::Deals(deals))
            }
            DbCommand::GetDeal { id } => {
                let deal = escrownad::models::Deal::find_by_id(&self.pool, id)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(DbResponse::Deal(deal))
            }
            DbCommand::SaveDeal { mut data } => {
                data.save(&self.pool).await.map_err(|e| e.to_string())?;
                Ok(DbResponse::Ok)
            }
        }
    }
}

// Макрос разворачивает 5 системных веток в `impl DbState::handle_system_cmd`.
forge_db::system_commands! {
    impl DbState {
        command:  DbCommand,
        response: DbResponse,
        cache:    constants,
    }
}

async fn run() -> Result<()> {
    info!("starting");

    let cfg: DbToml = forge_core::config::load_toml("etc/database.toml").context("load etc/database.toml")?;

    let (url, mode) = pg::connect_string(&cfg.postgres).context("build postgres connection string")?;

    let pool = PgPoolOptions::new()
        .max_connections(cfg.postgres.max_connections)
        .connect(&url)
        .await
        .with_context(|| {
            format!(
                "connect to postgres ({mode}) at {host}:{port}/{db} as {user}",
                mode = mode.label(),
                host = cfg.postgres.host,
                port = cfg.postgres.port,
                db = cfg.postgres.db,
                user = cfg.postgres.user,
            )
        })?;

    let now = forge_db::system::ping_now(&pool)
        .await
        .context("postgres ping (SELECT NOW)")?;
    info!(
        db = cfg.postgres.db.as_str(),
        mode = %mode.label(),
        time = %now.format("%Y-%m-%d %H:%M:%S UTC"),
        "postgres connected"
    );

    let state = DbState::new(pool).await.context("init db state")?;

    // ── ЭТАЛОН: whitelist проектной таблицы для generic list-операций ────────
    // Регистрируем СВОЮ таблицу `demos` — иначе count_rows / inline_toggle /
    // сортировка по ней отклонятся (ядерные модели засеяны по умолчанию).
    // Одна строка на таблицу, при старте database-бинаря (§4 09_admin_lists).
    forge_admin::register_list_model(
        "demos",
        forge_admin::ListModelSpec {
            table: "demos",
            pk: "dmo_id",
            bool_cols: &["dmo_is_enable"],
            sort_cols: &["dmo_code", "dmo_title", "dmo_is_enable"],
            ..Default::default()
        },
    );
    forge_admin::register_list_model(
        "deals",
        forge_admin::ListModelSpec {
            table: "deals",
            pk: "del_id",
            bool_cols: &["del_is_enable"],
            sort_cols: &["del_id", "del_status", "prefix", "resource_kind"],
            ..Default::default()
        },
    );

    serve_ipc::<DbCommand, DbResponse, _>(sockets::DATABASE, state)
        .await
        .context("serve ipc")?;
    Ok(())
}

#[tokio::main]
async fn main() {
    let log_path = forge_core::logger::init_with_file("escrownad-database");
    info!(log = %log_path.display(), "logger initialized");
    if let Err(e) = run().await {
        forge_core::error::log_chain(e.as_ref());
        std::process::exit(1);
    }
}
