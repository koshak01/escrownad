//! Listing review: a Telegram card with Approve / Decline buttons.
//!
//! A lot never reaches the board by itself. Once submitted it waits for an
//! operator, and the operator gets everything needed to decide in one Telegram
//! message: the listing's parameters, what the registry says about the network,
//! how much money is in the applicant's wallet, and whether we have seen that
//! wallet before.
//!
//! The text is English only: the product is English, and the operator reads
//! exactly what the users read.

use forge_fixed_n::FixedN;
use serde::Serialize;

use crate::app_context;
use crate::models::Deal;

/// Callback button prefix: `dm:<short hash>:<a|d>`.
///
/// Telegram caps `callback_data` at 64 bytes, and a full deal hash is 64
/// characters on its own. So the button carries the first 16 — enough to find
/// the deal unambiguously.
pub const CALLBACK_PREFIX: &str = "dm";

/// How many characters of the hash go into the button.
pub const SHORT_HASH_LEN: usize = 16;

/// The short form of a hash, for a callback button.
pub fn short_hash(del_hash: &str) -> String {
    del_hash.chars().take(SHORT_HASH_LEN).collect()
}

/// Template key in the `templates` table.
pub const TEMPLATE_CODE: &str = "deal_moderation";

/// Everything the operator sees in the message.
#[derive(Debug, Serialize)]
pub struct ModerationCard {
    pub deal_hash: String,
    /// Short hash — goes into the buttons' callback_data.
    pub deal_short: String,
    pub deal_no: String,
    pub side: String,
    pub asset_type: String,
    pub resource_kind: String,
    pub rir: String,
    pub prefix: String,
    pub addresses: String,
    pub geo: String,
    pub from_org: String,
    pub price: String,
    pub description: String,
    pub terms: String,
    pub wallet: String,
    pub wallet_short: String,
    /// The applicant's USDC balance, as a string with two decimals.
    pub wallet_usdc: String,
    /// The chain's native coin — what gas is paid in.
    pub wallet_native: String,
    /// We have not seen this wallet before.
    pub wallet_is_new: bool,
    /// How many deals this wallet has had already.
    pub wallet_deals: i64,
    /// What the registry says about this network right now.
    pub registry_holder: String,
    pub registry_type: String,
    pub registry_country: String,
    /// The registry's holder matches what the applicant claimed.
    pub registry_matches: bool,
    pub url: String,
}

fn short_wallet(addr: &str) -> String {
    if addr.len() > 14 {
        format!("{}…{}", &addr[..8], &addr[addr.len() - 4..])
    } else {
        addr.to_string()
    }
}

fn usdc_str(raw: alloy::primitives::U256) -> String {
    let units: u128 = raw.to::<u128>();
    format!("{}.{:02}", units / 1_000_000, (units % 1_000_000) / 10_000)
}

fn native_str(raw: alloy::primitives::U256) -> String {
    let units: u128 = raw.to::<u128>();
    let whole = units / 1_000_000_000_000_000_000;
    let frac = (units % 1_000_000_000_000_000_000) / 10_000_000_000_000_000;
    format!("{whole}.{frac:02}")
}

fn money(amount: FixedN<8>) -> String {
    let raw = amount.raw();
    format!(
        "{}.{:02}",
        raw / 100_000_000,
        (raw % 100_000_000) / 1_000_000
    )
}

/// Builds the review card for the operator.
///
/// The network is queried against the registry right here — the operator sees
/// what RIPE answers, not what the applicant typed. Balances are read from the
/// chain. If either is unreachable the fields stay empty: a decision is still
/// possible, just with less to go on.
pub async fn build_card(deal: &Deal) -> ModerationCard {
    let wallet = deal.seller_wallet.clone().unwrap_or_default();

    // how many deals this wallet had before — "new" means this is its first
    let wallet_deals = match deal.seller_usr_id {
        Some(uid) => app_context()
            .db
            .list_deals_for_user(uid)
            .await
            .map(|v| v.len() as i64)
            .unwrap_or(0),
        None => 0,
    };

    // balances from the chain
    let (mut usdc, mut native) = (String::new(), String::new());
    if !wallet.is_empty()
        && let Some(config) = chain_config().await
        && let Some(reader) = crate::chain::core::ChainReader::new(&config)
    {
        match reader.wallet_balances(&wallet).await {
            Ok((u, n)) => {
                usdc = usdc_str(u);
                native = native_str(n);
            }
            Err(e) => tracing::warn!(error = %e, "could not read the wallet balance"),
        }
    }

    // what the registry says about the network
    let (mut holder, mut rtype, mut country) = (String::new(), String::new(), String::new());
    if !deal.prefix.trim().is_empty() {
        match crate::observer::rdap::lookup(&deal.prefix).await {
            Ok(Some(record)) => {
                holder = record.holder;
                rtype = record.resource_type;
                country = record.country;
            }
            Ok(None) => holder = "NOT FOUND IN REGISTRY".into(),
            Err(e) => tracing::warn!(error = %e, "registry unreachable during review"),
        }
    }
    let claimed = deal.from_org.as_deref().unwrap_or("").trim().to_lowercase();
    let registry_matches =
        !claimed.is_empty() && !holder.is_empty() && holder.to_lowercase().contains(&claimed);

    ModerationCard {
        deal_no: deal.public_no(),
        side: deal.listing_side.clone(),
        asset_type: deal.asset_type.to_uppercase(),
        resource_kind: deal.resource_kind.clone(),
        rir: deal.rir.clone(),
        addresses: crate::models::deal::address_count(&deal.prefix)
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".into()),
        prefix: deal.prefix.clone(),
        geo: deal.geo.clone().unwrap_or_default(),
        from_org: deal.from_org.clone().unwrap_or_default(),
        price: money(deal.del_amount),
        description: crate::sanitize::plain_text(deal.del_title.as_deref().unwrap_or("")),
        terms: crate::sanitize::plain_text(deal.del_note.as_deref().unwrap_or("")),
        wallet_short: short_wallet(&wallet),
        wallet_usdc: usdc,
        wallet_native: native,
        wallet_is_new: wallet_deals <= 1,
        wallet_deals,
        registry_holder: holder,
        registry_type: rtype,
        registry_country: country,
        registry_matches,
        url: format!("https://escrownad.com/deals/{}/", deal.del_hash),
        wallet,
        deal_short: short_hash(&deal.del_hash),
        deal_hash: deal.del_hash.clone(),
    }
}

/// Chain settings out of the constants — a shared helper.
async fn chain_config() -> Option<crate::chain::types::ChainConfig> {
    let constants = app_context().db.get_constants().await.ok()?;
    let mut map = std::collections::HashMap::new();
    for key in constants.keys() {
        if let Ok(Some(v)) = constants.get::<serde_json::Value>(key) {
            map.insert(key.clone(), v);
        }
    }
    crate::chain::types::ChainConfig::from_constants(&map)
}

/// Sends the listing to the operator.
///
/// A delivery failure is not raised upward: the listing is already saved and
/// waiting for a decision, and Telegram being down is no reason to show the
/// user an error.
pub async fn notify(deal: &Deal) {
    let card = build_card(deal).await;
    if let Err(e) = app_context().notifier.send(TEMPLATE_CODE, &card).await {
        tracing::error!(deal = %deal.del_hash, error = %e, "could not send the listing for review");
    }
}
