//! RIPE PA+PI oracle matcher for deals.

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

/// Match deal against a transfer row.
/// - prefix must appear in original_block or transferred_blocks
/// - if from_org / to_org set on deal, require case-insensitive contains
pub fn match_deal(
    prefix: &str,
    from_org: Option<&str>,
    to_org: Option<&str>,
    t: &RipeTransfer,
) -> bool {
    let p = prefix.trim();
    if p.is_empty() {
        return false;
    }
    let block_ok = t.original_block.contains(p) || t.transferred_blocks.contains(p);
    if !block_ok {
        return false;
    }
    if let Some(f) = from_org.filter(|s| !s.is_empty()) {
        if !t.from.to_lowercase().contains(&f.to_lowercase()) {
            return false;
        }
    }
    if let Some(to) = to_org.filter(|s| !s.is_empty()) {
        if !t.to.to_lowercase().contains(&to.to_lowercase()) {
            return false;
        }
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
