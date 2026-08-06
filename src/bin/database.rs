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
            DbCommand::GetDealByHashPrefix { prefix } => {
                // Ищем по началу хэша — в callback-кнопку влезает 16 символов.
                let deal: Option<escrownad::models::Deal> = forge_db::sqlx::query_as(
                    "SELECT * FROM deals WHERE del_hash LIKE $1 || '%' LIMIT 1",
                )
                .bind(&prefix)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(DbResponse::Deal(deal))
            }
            DbCommand::GetDealByHash { hash } => {
                let deal = escrownad::models::Deal::find_by_hash(&self.pool, &hash)
                    .await
                    .map_err(|e| e.to_string())?;
                Ok(DbResponse::Deal(deal))
            }
            DbCommand::SaveDeal { mut data } => {
                data.save(&self.pool).await.map_err(|e| e.to_string())?;
                Ok(DbResponse::Ok)
            }

            DbCommand::WalletFindOrCreate { address, pubkey } => {
                // Same as password login, but identity = EVM address + signature.
                // New users: usr_is_staff=false, role "user" (never admin),
                // link users2wallets + configs2users.wallet_address.
                let existing: Option<(i64,)> = forge_db::sqlx::query_as(include_str!(
                    "../../sqls/wallets_find_user.sql"
                ))
                .bind(&address)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| e.to_string())?;

                if let Some((usr_id,)) = existing {
                    // Публичный ключ приезжает с каждым входом: у тех, кто
                    // входил до появления колонки, он появится сам собой.
                    let _: (i64,) =
                        forge_db::sqlx::query_as(include_str!("../../sqls/wallets_link.sql"))
                            .bind(usr_id)
                            .bind(&address)
                            .bind(&pubkey)
                            .fetch_one(&self.pool)
                            .await
                            .map_err(|e| e.to_string())?;
                    // Ensure role + config stay in sync on every login
                    forge_db::sqlx::query(include_str!("../../sqls/wallets_assign_user_role.sql"))
                        .bind(usr_id)
                        .execute(&self.pool)
                        .await
                        .map_err(|e| e.to_string())?;
                    forge_db::sqlx::query(include_str!("../../sqls/wallets_upsert_config.sql"))
                        .bind(usr_id)
                        .bind(&address)
                        .execute(&self.pool)
                        .await
                        .map_err(|e| e.to_string())?;

                    let row: (String, bool, bool) = forge_db::sqlx::query_as(
                        "SELECT usr_hash, usr_is_staff, usr_is_enable FROM users WHERE usr_id = $1",
                    )
                    .bind(usr_id)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(|e| e.to_string())?;
                    return Ok(DbResponse::WalletUser(escrownad::wallet_auth::WalletUserRow {
                        usr_id,
                        usr_hash: row.0,
                        usr_is_staff: row.1,
                        usr_is_enable: row.2,
                        is_new: false,
                    }));
                }

                let usr_hash = escrownad::wallet_auth::new_usr_hash();
                let descr = escrownad::wallet_auth::wallet_descr(&address);
                // staff=false — product user only (see wallets_insert_user.sql)
                let (usr_id, usr_hash, usr_is_staff, usr_is_enable): (i64, String, bool, bool) =
                    forge_db::sqlx::query_as(include_str!("../../sqls/wallets_insert_user.sql"))
                        .bind(&usr_hash)
                        .bind(&descr)
                        .fetch_one(&self.pool)
                        .await
                        .map_err(|e| e.to_string())?;

                let _: (i64,) = forge_db::sqlx::query_as(include_str!("../../sqls/wallets_link.sql"))
                    .bind(usr_id)
                    .bind(&address)
                    .bind(&pubkey)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(|e| e.to_string())?;

                // Role "user" — not admin/manager
                forge_db::sqlx::query(include_str!("../../sqls/wallets_assign_user_role.sql"))
                    .bind(usr_id)
                    .execute(&self.pool)
                    .await
                    .map_err(|e| e.to_string())?;

                // settings table configs2users
                forge_db::sqlx::query(include_str!("../../sqls/wallets_upsert_config.sql"))
                    .bind(usr_id)
                    .bind(&address)
                    .execute(&self.pool)
                    .await
                    .map_err(|e| e.to_string())?;

                Ok(DbResponse::WalletUser(escrownad::wallet_auth::WalletUserRow {
                    usr_id,
                    usr_hash,
                    usr_is_staff,
                    usr_is_enable,
                    is_new: true,
                }))
            }

            DbCommand::WalletAddressForUser { usr_id } => {
                let row: Option<(String,)> = forge_db::sqlx::query_as(include_str!(
                    "../../sqls/wallets_find_by_user.sql"
                ))
                .bind(usr_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(DbResponse::WalletAddress(row.map(|r| r.0)))
            }

            DbCommand::ListVerifiedSellers => {
                let rows: Vec<(i64,)> = forge_db::sqlx::query_as(include_str!(
                    "../../sqls/sellers_verified.sql"
                ))
                .fetch_all(&self.pool)
                .await
                .map_err(|e| e.to_string())?;
                Ok(DbResponse::VerifiedSellers(
                    rows.into_iter().map(|r| r.0).collect(),
                ))
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
