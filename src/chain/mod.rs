//! Расчётный слой: USDC-замок EscrowLock на Monad.
//!
//! Деньги покупателя физически уходят с его кошелька на контракт в момент
//! «я хочу» и лежат там до развязки. Забрать их обратно нельзя ни ему, ни
//! нам: только `release` продавцу (по факту RIPE), `refund` покупателю или
//! `refundAfterDeadline` им самим по истечении срока.
//!
//! Контракт — общий пул: USDC всех сделок лежат на одном адресе, учёт
//! внутри по `dealId`. Наружу уходит только отпечаток сделки — ни сети,
//! ни организаций, ни описания в цепи нет.
//!
//! `CHAIN_MODE=mock` (умолчание) — режим без цепи для локальной работы.

pub mod core;
pub mod types;

use forge_core::hash::sha256_hex;

/// Идентификатор сделки в цепи как hex-строка — для логов и UI.
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
