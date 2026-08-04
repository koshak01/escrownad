//! Monad lock integration — **USDC (ERC-20)** settlement.
//!
//! Mock txs on deals until live EscrowLock deploy.
//! Real path: `contracts/EscrowLock.sol` + USDC (or MockUSDC).
//! `CHAIN_MODE=mock` default.

use forge_core::hash::sha256_hex;

/// USDC uses 6 decimals. Deal amounts in DB are still FixedN<8> raw for forge
/// money canon; display layer treats product currency as USDC.
pub const USDC_DECIMALS: u8 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainMode {
    Mock,
}

impl ChainMode {
    pub fn from_env() -> Self {
        Self::Mock
    }
}

pub fn deal_id_hex(del_hash: &str) -> String {
    let h = sha256_hex(del_hash.as_bytes());
    format!("0x{h}")
}

pub fn mock_fund_tx(del_hash: &str, buyer: &str) -> String {
    format!(
        "mock:usdc:fund:{}:{}",
        &deal_id_hex(del_hash)[..18],
        &buyer[..buyer.len().min(10)]
    )
}

pub fn mock_release_tx(ripe_key: &str) -> String {
    format!("mock:usdc:ripe:{ripe_key}")
}

pub fn mock_refund_tx(del_hash: &str) -> String {
    format!("mock:usdc:refund:{}", &deal_id_hex(del_hash)[..18])
}
