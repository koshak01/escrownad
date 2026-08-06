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

    // Chain settings live in the `chain` constant and are edited through
    // /admin/constants/ — no environment variables, no files. Unlike the other
    // binaries, the observer needs a private key. Without one we do not die:
    // we run in mock and say so loudly. A service should survive an incomplete
    // configuration rather than fall into a crash loop.
    let chain = match load_chain_config(&db).await {
        Some(config) if config.mode().is_live() => match ObserverChain::new(config) {
            Ok(c) => match c.observer_address() {
                Ok(addr) => {
                    info!(observer = %addr, "chain: live mode");
                    Some(c)
                }
                Err(e) => {
                    warn!(error = %e, "observer key does not parse — running in mock");
                    None
                }
            },
            Err(e) => {
                warn!(error = %e, "chain configuration incomplete — running in mock");
                None
            }
        },
        _ => {
            warn!("chain: mock mode — releases are not sent to the network");
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

/// Finishes what approval started: asks whether the lot's asset has been minted.
///
/// Issuance is asynchronous — the request goes out when an operator approves a
/// listing, and their side reviews and mints afterwards. Somebody has to come
/// back for the address, and the observer is already going round in a loop.
///
/// Nothing here is fatal. A pending request stays pending until the next round;
/// a refused one is logged and the request cleared, so the lot stops being asked
/// about. The listing itself is unaffected either way — a lot without an asset
/// still trades.
async fn finish_minting(db: &DbClient) {
    let Ok(constants) = db.get_constants().await else {
        return;
    };
    let Ok(Some(config)) = constants
        .get::<escrownad::cleanverse::types::CleanverseConfig>(
            escrownad::cleanverse::types::CLEANVERSE_CONSTANT,
        )
    else {
        return;
    };

    let pending = match db.list_deals_minting().await {
        Ok(deals) => deals,
        Err(e) => {
            warn!(error = %e, "could not list lots awaiting their asset");
            return;
        }
    };

    for mut deal in pending {
        let Some(request_id) = deal.asset_request.clone() else {
            continue;
        };
        match escrownad::cleanverse::core::issue_status(&config, &request_id).await {
            Ok(escrownad::cleanverse::types::IssueStatus::Issued(address)) => {
                info!(deal = %deal.del_hash, asset = %address, "asset issued");
                deal.asset_token = Some(address);
                deal.asset_request = None;
                if let Err(e) = db.save_deal(deal).await {
                    error!(error = %e, "could not store the asset address");
                }
            }
            Ok(escrownad::cleanverse::types::IssueStatus::Failed(reason)) => {
                warn!(deal = %deal.del_hash, %reason, "asset issuance failed — giving up on it");
                deal.asset_request = None;
                if let Err(e) = db.save_deal(deal).await {
                    error!(error = %e, "could not clear the issuance request");
                }
            }
            Ok(escrownad::cleanverse::types::IssueStatus::Pending) => {}
            Err(e) => warn!(deal = %deal.del_hash, error = %e, "could not check issuance status"),
        }
    }
}

/// Reads the chain settings from the constants table.
async fn load_chain_config(db: &DbClient) -> Option<ChainConfig> {
    let constants = match db.get_constants().await {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "could not read the constants — chain unavailable");
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
    // Asset issuance is asynchronous — finish what approval started.
    finish_minting(db).await;

    // Open deals: funded / awaiting_proof
    let deals = db
        .list_deals_listed()
        .await
        .map_err(|e| anyhow::anyhow!("list deals: {e}"))?;

    // The query already returns exactly these two; the filter is a second lock
    // on the same door, kept so that a change to the SQL cannot quietly widen
    // what the observer is willing to release money against.
    let open: Vec<_> = deals
        .into_iter()
        .filter(|d| matches!(d.del_status.as_str(), "funded" | "awaiting_proof"))
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
        // A transfer that happened before the deal existed is of no use:
        // the oracle waits for a NEW row against this network.
        let since = Some(deal.del_dat.to_dt().date());
        let hit = table.iter().find(|t| {
            observer::match_deal(
                &deal.prefix,
                since,
                deal.from_org.as_deref(),
                deal.to_org.as_deref(),
                t,
            )
        });
        // Second source: the registry queried on the network itself. The
        // transfer table lags, whereas RDAP answers immediately — and works
        // beyond RIPE. If the holder changed after the deal appeared, the
        // transfer happened, even while the table still shows nothing.
        let key = match hit {
            Some(t) => observer::match_key(kind, t),
            None => {
                let since = deal.del_dat.to_dt().date();
                match observer::rdap::lookup(&deal.prefix).await {
                    Ok(Some(record))
                        if record.changed_hands_since(since, deal.from_org.as_deref()) =>
                    {
                        info!(
                            deal = %deal.del_hash,
                            prefix = %deal.prefix,
                            holder = %record.holder,
                            "registry: the holder changed"
                        );
                        format!(
                            "RDAP|{}|{}|{}",
                            record
                                .last_changed
                                .map(|d| d.to_string())
                                .unwrap_or_default(),
                            record.range,
                            record.holder
                        )
                    }
                    Ok(_) => continue,
                    Err(e) => {
                        warn!(deal = %deal.del_hash, error = %e, "registry unreachable");
                        continue;
                    }
                }
            }
        };
        info!(
            deal = %deal.del_hash,
            prefix = %deal.prefix,
            key = %key,
            "matched a RIPE registry row"
        );

        let release_tx = match chain {
            Some(chain) => {
                // We do not trust the database: the money must really be in the
                // lock, and it must be about to go to the seller of this
                // listing. `fund` takes the seller as an argument, chosen by
                // whoever paid — so a mismatch means the money would land on a
                // stranger's address. Releasing then is worse than waiting.
                match chain.locked_deal(&deal.del_hash).await {
                    Ok(locked) if locked.state == LockState::Funded => {
                        let listed_seller = deal.seller_wallet.as_deref().unwrap_or_default();
                        if !locked.seller_is(listed_seller) {
                            error!(
                                deal = %deal.del_hash,
                                on_chain = %locked.seller,
                                listed = %listed_seller,
                                "the contract names a different seller — release refused"
                            );
                            continue;
                        }
                        info!(
                            deal = %deal.del_hash,
                            amount = locked.amount,
                            "lock is funded — releasing"
                        );
                    }
                    Ok(locked) => {
                        warn!(
                            deal = %deal.del_hash,
                            state = locked.state.as_str(),
                            "not funded on chain — release skipped"
                        );
                        continue;
                    }
                    Err(e) => {
                        error!(deal = %deal.del_hash, error = %e, "could not read the lock state");
                        continue;
                    }
                }
                match chain.release(&deal.del_hash, &key).await {
                    Ok(tx) => tx,
                    Err(e) => {
                        // leave the deal status alone — we will retry next round
                        error!(deal = %deal.del_hash, error = %e, "release failed");
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
