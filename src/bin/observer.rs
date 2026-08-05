//! escrownad-observer — polls RIPE PA+PI, matches open deals, mock-releases.
//!
//! Order: database must be up (IPC). Env:
//!   OBSERVER_INTERVAL_SEC (default 300)
//!   OBSERVER_ONCE=1 — single pass then exit (for smoke)

use std::time::Duration;

use anyhow::{Context, Result};
use escrownad::chain::core::ObserverChain;
use escrownad::chain::types::{ChainConfig, LockState};
use escrownad::observer::{self, ResourceKind};
use escrownad::{DbClient, sockets};
use tracing::{error, info, warn};

#[tokio::main]
async fn main() {
    let log_path = forge_core::logger::init_with_file("escrownad-observer");
    info!(log = %log_path.display(), "logger initialized");
    if let Err(e) = run().await {
        forge_core::error::log_chain(e.as_ref());
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let interval = std::env::var("OBSERVER_INTERVAL_SEC")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300u64);
    let once = std::env::var("OBSERVER_ONCE").ok().as_deref() == Some("1");

    let db = DbClient::spawn();
    db.wait_ready(Duration::from_secs(30))
        .await
        .context("wait database")?;
    info!(socket = sockets::DATABASE, "database ready");

    // Настройки цепи живут в константе `chain` (таблица constants),
    // правятся через /admin/constants/ — ни окружения, ни файлов.
    // Наблюдателю, в отличие от остальных бинарников, нужен приватный ключ.
    // Нет ключа — не падаем, а работаем в mock и говорим об этом громко:
    // сервис должен пережить неполную конфигурацию, а не уйти в крашлуп.
    let chain = match load_chain_config(&db).await {
        Some(config) if config.mode().is_live() => match ObserverChain::new(config) {
            Ok(c) => match c.observer_address() {
                Ok(addr) => {
                    info!(observer = %addr, "цепь: живой режим");
                    Some(c)
                }
                Err(e) => {
                    warn!(error = %e, "ключ наблюдателя не разбирается — работаю в mock");
                    None
                }
            },
            Err(e) => {
                warn!(error = %e, "цепь настроена не полностью — работаю в mock");
                None
            }
        },
        _ => {
            warn!("цепь: режим mock — выпуск денег не отправляется в сеть");
            None
        }
    };

    loop {
        match tick(&db, chain.as_ref()).await {
            Ok(n) => info!(matched = n, "observer tick done"),
            Err(e) => warn!(error = %e, "observer tick failed"),
        }
        if once {
            break;
        }
        tokio::time::sleep(Duration::from_secs(interval)).await;
    }
    Ok(())
}

/// Читает настройки цепи из констант базы.
async fn load_chain_config(db: &DbClient) -> Option<ChainConfig> {
    let constants = match db.get_constants().await {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "не прочитал константы — цепь недоступна");
            return None;
        }
    };
    let mut map = std::collections::HashMap::new();
    for key in constants.keys() {
        if let Ok(Some(v)) = constants.get::<serde_json::Value>(key) {
            map.insert(key.clone(), v);
        }
    }
    ChainConfig::from_constants(&map)
}

async fn tick(db: &DbClient, chain: Option<&ObserverChain>) -> Result<u32> {
    // Open deals: funded / awaiting_proof (and listed for demo fixtures)
    let deals = db
        .list_deals_listed()
        .await
        .map_err(|e| anyhow::anyhow!("list deals: {e}"))?;

    let open: Vec<_> = deals
        .into_iter()
        .filter(|d| {
            matches!(
                d.del_status.as_str(),
                "listed" | "funded" | "awaiting_proof"
            )
        })
        .collect();
    if open.is_empty() {
        info!("no open deals");
        return Ok(0);
    }

    let pi = observer::fetch_transfers(ResourceKind::Pi)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    let pa = observer::fetch_transfers(ResourceKind::Pa)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    info!(pi = pi.len(), pa = pa.len(), "RIPE tables loaded");

    let mut matched = 0u32;
    for mut deal in open {
        let kind = match ResourceKind::parse(&deal.resource_kind) {
            Some(k) => k,
            None => {
                warn!(id = deal.del_id, kind = %deal.resource_kind, "unknown resource_kind");
                continue;
            }
        };
        let table = match kind {
            ResourceKind::Pi => &pi,
            ResourceKind::Pa => &pa,
        };
        let hit = table.iter().find(|t| {
            observer::match_deal(
                &deal.prefix,
                deal.from_org.as_deref(),
                deal.to_org.as_deref(),
                t,
            )
        });
        let Some(t) = hit else {
            continue;
        };
        let key = observer::match_key(kind, t);
        info!(
            deal = %deal.del_hash,
            prefix = %deal.prefix,
            key = %key,
            "совпадение с реестром RIPE"
        );

        let release_tx = match chain {
            Some(chain) => {
                // Не верим базе: деньги должны реально лежать в замке.
                match chain.deal_state(&deal.del_hash).await {
                    Ok((LockState::Funded, amount)) => {
                        info!(deal = %deal.del_hash, %amount, "замок оплачен — выпускаю");
                    }
                    Ok((state, _)) => {
                        warn!(
                            deal = %deal.del_hash,
                            state = state.as_str(),
                            "в цепи не funded — выпуск пропущен"
                        );
                        continue;
                    }
                    Err(e) => {
                        error!(deal = %deal.del_hash, error = %e, "не прочитал состояние замка");
                        continue;
                    }
                }
                match chain.release(&deal.del_hash, &key).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        // статус сделки не меняем — попробуем на следующем круге
                        error!(deal = %deal.del_hash, error = %e, "выпуск не прошёл");
                        continue;
                    }
                }
            }
            None => escrownad::chain::mock_release_tx(&key),
        };

        deal.del_status = "released".into();
        deal.ripe_match_key = Some(key.clone());
        deal.release_tx = Some(release_tx);
        db.save_deal(deal)
            .await
            .map_err(|e| anyhow::anyhow!("save deal: {e}"))?;
        matched += 1;
    }
    Ok(matched)
}
