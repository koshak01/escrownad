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

/// Public block explorer for a Monad chain id.
///
/// Used on deal cards so Lock / Release / wallets open a checkable page
/// instead of a dead hex string. Unknown ids fall back to testnet — that is
/// where the product runs today.
pub fn explorer_base(chain_id: u64) -> &'static str {
    match chain_id {
        // Monad mainnet (not deployed yet; keep the canonical host ready).
        143 => "https://monadexplorer.com",
        // Monad testnet (chain id 10143) and anything else we might point at.
        _ => "https://testnet.monadexplorer.com",
    }
}

/// `https://…/tx/0x…` when `value` looks like a 32-byte transaction hash.
///
/// Mock stand-ins (`mock:…`) and bare deal fingerprints are rejected so the
/// card never links into a 404.
pub fn explorer_tx_url(chain_id: u64, value: &str) -> Option<String> {
    let v = value.trim();
    if !is_hex_word(v, 32) {
        return None;
    }
    Some(format!("{}/tx/{}", explorer_base(chain_id), v))
}

/// `https://…/address/0x…` when `value` looks like a 20-byte address.
pub fn explorer_address_url(chain_id: u64, value: &str) -> Option<String> {
    let v = value.trim();
    if !is_hex_word(v, 20) {
        return None;
    }
    Some(format!("{}/address/{}", explorer_base(chain_id), v))
}

/// True for `0x` + exactly `byte_len` bytes of hex (no mock prefixes).
fn is_hex_word(value: &str, byte_len: usize) -> bool {
    let Some(body) = value.strip_prefix("0x").or_else(|| value.strip_prefix("0X")) else {
        return false;
    };
    body.len() == byte_len * 2 && body.bytes().all(|b| b.is_ascii_hexdigit())
}

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
