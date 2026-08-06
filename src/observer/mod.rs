//! Oracle: confirming that a network changed hands, from registry data.
//!
//! Two independent sources of fact:
//! * the RIPE transfer tables (this file) — they say "a transfer happened on
//!   this date";
//! * RDAP on the network itself ([`rdap`]) — it says who holds it right now.

pub mod rdap;

use serde::Deserialize;

pub const RIPE_PI: &str =
    "https://www-static.ripe.net/dynamic/table-of-transfers/ipv4/transfers-assignments.json";
pub const RIPE_PA: &str =
    "https://www-static.ripe.net/dynamic/table-of-transfers/ipv4/transfers-allocations.json";

#[derive(Debug, Clone, Deserialize)]
pub struct RipeTransfer {
    pub original_block: String,
    pub transferred_blocks: String,
    pub from: String,
    pub to: String,
    pub date: String,
    #[serde(rename = "transferType")]
    pub transfer_type: String,
    #[serde(rename = "transferStatus")]
    pub transfer_status: String,
}

#[derive(Debug, Deserialize)]
struct Payload {
    transfers: Vec<RipeTransfer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Pa,
    Pi,
}

impl ResourceKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "PA" => Some(Self::Pa),
            "PI" => Some(Self::Pi),
            _ => None,
        }
    }

    pub fn url(self) -> &'static str {
        match self {
            Self::Pa => RIPE_PA,
            Self::Pi => RIPE_PI,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pa => "PA",
            Self::Pi => "PI",
        }
    }
}

pub async fn fetch_transfers(kind: ResourceKind) -> Result<Vec<RipeTransfer>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(kind.url())
        .send()
        .await
        .map_err(|e| format!("HTTP {}: {e}", kind.as_str()))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} status {}", kind.as_str(), resp.status()));
    }
    let payload: Payload = resp
        .json()
        .await
        .map_err(|e| format!("JSON {}: {e}", kind.as_str()))?;
    Ok(payload.transfers)
}

/// Date of a registry row. The RIPE tables use `05/08/2026`.
pub fn transfer_date(t: &RipeTransfer) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(t.date.trim(), "%d/%m/%Y").ok()
}

/// Does the network in this registry row match the one we are watching?
///
/// Compared on boundaries rather than as a substring: otherwise `10.1.1.0`
/// would match `110.1.1.0`, and the money would be released against somebody
/// else's transfer.
fn block_matches(prefix: &str, t: &RipeTransfer) -> bool {
    let needle = prefix.trim();
    if needle.is_empty() {
        return false;
    }
    let listed = |field: &str| {
        field
            .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .any(|block| block.eq_ignore_ascii_case(needle))
    };
    listed(&t.original_block) || listed(&t.transferred_blocks)
}

/// Does this registry row confirm the transfer our deal is waiting for?
///
/// The oracle sits on one specific network and waits for a **new** row to show
/// up against it:
///
/// 1. the network matches exactly (on boundaries, not as a substring);
/// 2. the row is dated **no earlier** than the deal itself — otherwise we would
///    count somebody else's old transfer that happened before the listing even
///    existed;
/// 3. the organisations, when given, act as an extra check — but the seller is
///    not required to fill them in: who handed what to whom is read off the
///    matched row itself.
///
/// # Parameters
/// * `prefix` — the network being sold
/// * `since` — the day before which a transfer is of no interest to us
/// * `from_org` / `to_org` — optional narrowing by party
/// * `t` — one row of the transfer table
///
/// # Returns
/// * `true` — this is our transfer
pub fn match_deal(
    prefix: &str,
    since: Option<chrono::NaiveDate>,
    from_org: Option<&str>,
    to_org: Option<&str>,
    t: &RipeTransfer,
) -> bool {
    if !block_matches(prefix, t) {
        return false;
    }
    if let Some(since) = since {
        match transfer_date(t) {
            Some(d) if d >= since => {}
            // unparsable date, or a transfer older than the deal — not ours
            _ => return false,
        }
    }
    if let Some(f) = from_org.filter(|s| !s.trim().is_empty())
        && !t.from.to_lowercase().contains(&f.trim().to_lowercase())
    {
        return false;
    }
    if let Some(to) = to_org.filter(|s| !s.trim().is_empty())
        && !t.to.to_lowercase().contains(&to.trim().to_lowercase())
    {
        return false;
    }
    true
}

pub fn match_key(kind: ResourceKind, t: &RipeTransfer) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}",
        kind.as_str(),
        t.date,
        t.original_block,
        t.transferred_blocks,
        t.from,
        t.to
    )
}
