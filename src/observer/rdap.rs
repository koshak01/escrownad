//! Querying the registry about one network over RDAP — the second source of fact.
//!
//! The transfer table says "a transfer happened on this date", but a row does
//! not appear there instantly. RDAP answers about a specific network here and
//! now: who holds it, what kind of resource it is, which country, and when the
//! record was last touched.
//!
//! Two uses:
//! 1. **at listing time** — pull the holder, the kind and the country straight
//!    from the registry instead of trusting what the seller typed in;
//! 2. **in the oracle** — spot a change of holder sooner than it reaches the
//!    transfer table.

use serde::Deserialize;

/// Address of the RIPE RDAP service.
pub const RIPE_RDAP: &str = "https://rdap.db.ripe.net/ip";

/// What the registry knows about a network right now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRecord {
    /// Range exactly as the registry returned it: `194.246.124.0 - 194.246.125.255`.
    pub range: String,
    /// Registry resource type: `ASSIGNED PI`, `ALLOCATED PA` and so on.
    pub resource_type: String,
    /// Our own resource code: `PI` | `PA` | empty when unrecognised.
    pub kind: String,
    /// Country code.
    pub country: String,
    /// Holder — the registrant organisation.
    pub holder: String,
    /// When the record was last changed.
    pub last_changed: Option<chrono::NaiveDate>,
}

#[derive(Debug, Deserialize)]
struct RdapResponse {
    #[serde(default)]
    handle: String,
    #[serde(default, rename = "type")]
    resource_type: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    entities: Vec<RdapEntity>,
    #[serde(default)]
    events: Vec<RdapEvent>,
}

#[derive(Debug, Deserialize)]
struct RdapEntity {
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default, rename = "vcardArray")]
    vcard: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RdapEvent {
    #[serde(default, rename = "eventAction")]
    action: String,
    #[serde(default, rename = "eventDate")]
    date: String,
}

/// Asks the registry about a network.
///
/// # Parameters
/// * `prefix` — the network, e.g. `194.246.124.0/23`
///
/// # Returns
/// * `Ok(Some(_))` — the registry knows this network
/// * `Ok(None)` — no such network (404); not a connectivity failure
/// * `Err(_)` — the network is unreachable, or the reply did not parse
pub async fn lookup(prefix: &str) -> Result<Option<NetworkRecord>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("{RIPE_RDAP}/{}", prefix.trim());
    let resp = client
        .get(&url)
        .header("Accept", "application/rdap+json")
        .send()
        .await
        .map_err(|e| format!("RDAP: {e}"))?;

    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(format!("RDAP status {}", resp.status()));
    }

    let body: RdapResponse = resp.json().await.map_err(|e| format!("RDAP JSON: {e}"))?;
    Ok(Some(NetworkRecord {
        range: body.handle,
        kind: parse_kind(&body.resource_type),
        resource_type: body.resource_type,
        country: body.country,
        holder: holder_name(&body.entities),
        last_changed: last_changed(&body.events),
    }))
}

/// Maps a registry type onto our own code: `ASSIGNED PI` → `PI`.
fn parse_kind(resource_type: &str) -> String {
    let upper = resource_type.to_uppercase();
    if upper.contains("PI") {
        "PI".into()
    } else if upper.contains("PA") {
        "PA".into()
    } else {
        String::new()
    }
}

/// Name of the registrant organisation, taken from the vCard.
///
/// In RDAP the holder sits in an entity with the `registrant` role, and its
/// name is the `fn` field of that entity's vCard. There can be several such
/// organisations — registry maintainers are registrants too — so we take the
/// first one that reads like an actual company name.
fn holder_name(entities: &[RdapEntity]) -> String {
    entities
        .iter()
        .filter(|e| e.roles.iter().any(|r| r == "registrant"))
        .filter_map(|e| vcard_fn(e.vcard.as_ref()?))
        .find(|name| !name.starts_with("MNT-") && !name.contains("RIPE-NCC"))
        .unwrap_or_default()
}

/// Pulls `fn` (the full name) out of a vcardArray.
fn vcard_fn(vcard: &serde_json::Value) -> Option<String> {
    let entries = vcard.as_array()?.get(1)?.as_array()?;
    for entry in entries {
        let parts = entry.as_array()?;
        if parts.first()?.as_str()? == "fn" {
            return parts.get(3)?.as_str().map(str::to_string);
        }
    }
    None
}

/// Date the record was last changed.
fn last_changed(events: &[RdapEvent]) -> Option<chrono::NaiveDate> {
    events
        .iter()
        .find(|e| e.action == "last changed")
        .or_else(|| events.iter().find(|e| e.action == "registration"))
        .and_then(|e| {
            chrono::DateTime::parse_from_rfc3339(e.date.trim())
                .ok()
                .map(|dt| dt.date_naive())
        })
}

impl NetworkRecord {
    /// Has the holder changed since the given date? That is the signature of a
    /// completed transfer which has not yet reached the transfer table.
    ///
    /// # Parameters
    /// * `since` — the day the deal appeared
    /// * `previous_holder` — who held the resource when the lot was listed
    pub fn changed_hands_since(
        &self,
        since: chrono::NaiveDate,
        previous_holder: Option<&str>,
    ) -> bool {
        let Some(changed) = self.last_changed else {
            return false;
        };
        if changed < since {
            return false;
        }
        match previous_holder.map(str::trim).filter(|s| !s.is_empty()) {
            // we know who held it, and it is somebody else now
            Some(prev) => !self.holder.eq_ignore_ascii_case(prev),
            // nothing to compare against — a date alone proves nothing
            None => false,
        }
    }
}
