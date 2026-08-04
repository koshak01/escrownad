//! Monad lock integration layer.
//!
//! v1: **mock** txs written to `deals.lock_tx` / `release_tx`.
//! Real path: call `EscrowLock` on Monad testnet (see `contracts/EscrowLock.sol`).
//!
//! When alloy/ethers is wired, replace mock bodies with RPC calls.
//! Keep mock as fallback via `CHAIN_MODE=mock` (default).

use forge_core::hash::sha256_hex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainMode {
    Mock,
    // Live, // later
}

impl ChainMode {
    pub fn from_env() -> Self {
        match std::env::var("CHAIN_MODE").ok().as_deref() {
            Some("live") | Some("monad") => {
                // Live not wired yet — fall back with log at call site.
                Self::Mock
            }
            _ => Self::Mock,
        }
    }
}

/// Derive on-chain deal id (bytes32 hex) from our `del_hash`.
pub fn deal_id_hex(del_hash: &str) -> String {
    // 0x + first 64 hex of sha256(del_hash) for a stable 32-byte id.
    let h = sha256_hex(del_hash.as_bytes());
    format!("0x{h}")
}

pub fn mock_fund_tx(del_hash: &str, buyer: &str) -> String {
    format!("mock:fund:{}:{}", &deal_id_hex(del_hash)[..18], &buyer[..buyer.len().min(10)])
}

pub fn mock_release_tx(ripe_key: &str) -> String {
    format!("mock:ripe:{ripe_key}")
}

pub fn mock_refund_tx(del_hash: &str) -> String {
    format!("mock:refund:{}", &deal_id_hex(del_hash)[..18])
}
