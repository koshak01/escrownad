//! escrownad-observer — polls RIPE PA+PI, matches open deals, mock-releases.
//!
//! Order: database must be up (IPC). Env:
//!   OBSERVER_INTERVAL_SEC (default 300)
//!   OBSERVER_ONCE=1 — single pass then exit (for smoke)

use std::time::Duration;

use anyhow::{Context, Result};
use escrownad::observer::{self, ResourceKind};
use escrownad::{DbClient, sockets};
use tracing::{info, warn};

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

    loop {
        match tick(&db).await {
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

async fn tick(db: &DbClient) -> Result<u32> {
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
            id = deal.del_id,
            prefix = %deal.prefix,
            key = %key,
            "RIPE match — mock release"
        );
        deal.del_status = "released".into();
        deal.ripe_match_key = Some(key.clone());
        // CHAIN_MODE=mock (default): string tx. Live Monad EscrowLock later.
        deal.release_tx = Some(escrownad::chain::mock_release_tx(&key));
        db.save_deal(deal)
            .await
            .map_err(|e| anyhow::anyhow!("save deal: {e}"))?;
        matched += 1;
    }
    Ok(matched)
}
