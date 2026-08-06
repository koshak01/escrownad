//! Settlement layer: the EscrowLock USDC lock on Monad.
//!
//! The buyer's money physically leaves their wallet for the contract at the
//! moment they commit, and sits there until the deal resolves. Neither they
//! nor we can pull it back: only `release` to the seller (against the registry
//! fact), `refund` to the buyer, or `refundAfterDeadline` taken by the buyer
//! themselves once the deadline has passed.
//!
//! The contract is a shared pool: USDC from every deal sits at one address,
//! accounted internally by `dealId`. Only the deal's fingerprint goes on
//! chain — no network, no organisations, no description.
//!
//! Mock mode (the default) runs without a chain, for local work.

pub mod core;
pub mod types;

use forge_core::hash::sha256_hex;

/// The deal's on-chain identifier as a hex string — for logs and the UI.
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
