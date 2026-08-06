//! Cleanverse client: checking a wallet's identity and its right to transfer.
//!
//! Their API answers `HTTP 200` on refusals as well — the real outcome sits in
//! the `code` field of the body. So decisions are made on the structure of the
//! reply, never on the text of `message`: wording changes, codes do not.

use std::time::Duration;

use serde_json::Value;
use tracing::warn;

use crate::cleanverse::types::{
    ApassRecord, CHAIN_NAME, CleanverseConfig, CleanverseError, TransferVerdict,
};

/// Success in their protocol.
const CODE_OK: &str = "0000";

/// A refusal on the merits — for instance, the wallet holds no identity.
const CODE_BUSINESS_FAIL: &str = "0002";

/// Where to send someone who has no identity yet.
///
/// One link for everyone, pointing at their self-service page: a person goes
/// there with their own wallet and obtains the identity themselves. No
/// personal data reaches us in any form — we learn only the fact of
/// verification.
pub const MAGIC_LINK: &str = "https://test-magiclink.cleanverse.com/";

fn client() -> Result<reqwest::Client, CleanverseError> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| CleanverseError::Http(e.to_string()))
}

/// Request identifier for their tracing.
fn new_request_id() -> String {
    use rand::Rng;
    let mut bytes = [0u8; 16];
    rand::thread_rng().fill(&mut bytes);
    hex::encode(bytes)
}

/// Sends a request and unwraps the `{code, message, data}` envelope.
///
/// # Parameters
/// * `config` — credentials and API address
/// * `path` — path after the base address, e.g. `/query_apass`
/// * `body` — request body as plain JSON
///
/// # Returns
/// * `Ok(Some(data))` — code `0000`, the useful part of the reply
/// * `Ok(None)` — a refusal on the merits (code `0002`): an ordinary outcome
///   rather than a failure — for example, the wallet simply has no identity
/// * `Err(_)` — network trouble, an unparsable reply, or any other code
async fn post_plain(
    config: &CleanverseConfig,
    path: &str,
    body: Value,
) -> Result<Option<Value>, CleanverseError> {
    if !config.is_configured() {
        return Err(CleanverseError::NotConfigured);
    }
    let url = format!("{}{path}", config.base_url());
    let resp = client()?
        .post(&url)
        .header("Content-Type", "application/json")
        .header("api-id", config.api_id.trim())
        .header("X-Request-ID", new_request_id())
        .json(&body)
        .send()
        .await
        .map_err(|e| CleanverseError::Http(format!("{path}: {e}")))?;

    let status = resp.status();
    let payload: Value = resp
        .json()
        .await
        .map_err(|e| CleanverseError::BadResponse(format!("{path} ({status}): {e}")))?;

    let code = payload
        .get("code")
        .and_then(|c| c.as_str())
        .unwrap_or_default()
        .to_string();
    let message = payload
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or_default()
        .to_string();

    match code.as_str() {
        CODE_OK => Ok(payload.get("data").cloned()),
        CODE_BUSINESS_FAIL => Ok(None),
        _ => Err(CleanverseError::Api { code, message }),
    }
}

/// Asks whether a wallet holds an identity.
///
/// # Parameters
/// * `config` — credentials and API address
/// * `address` — wallet address
///
/// # Returns
/// * `Ok(Some(_))` — an identity exists; whether it is valid is answered by
///   [`ApassRecord::is_valid_at`]
/// * `Ok(None)` — no identity; an ordinary answer, not an error
pub async fn query_apass(
    config: &CleanverseConfig,
    address: &str,
) -> Result<Option<ApassRecord>, CleanverseError> {
    let body = serde_json::json!({ "chain": CHAIN_NAME, "address": address });
    let Some(data) = post_plain(config, "/query_apass", body).await? else {
        return Ok(None);
    };
    match serde_json::from_value::<ApassRecord>(data) {
        Ok(record) => Ok(Some(record)),
        Err(e) => Err(CleanverseError::BadResponse(format!("query_apass: {e}"))),
    }
}

/// Is this wallet's identity valid right now?
///
/// A convenience wrapper for places where only yes-or-no matters: signing in,
/// rendering the payment button. Their API being unreachable is treated as
/// "unknown" rather than "forbidden" — otherwise an outage on their side would
/// shut our site down. The real refusal stands in the contract anyway, and
/// that one cannot be bypassed.
///
/// # Parameters
/// * `config` — credentials and API address
/// * `address` — wallet address
/// * `now` — the moment to check against, unix seconds
///
/// # Returns
/// * `Some(true)` — an identity exists and is valid
/// * `Some(false)` — no identity, or it is not valid
/// * `None` — we could not ask
pub async fn is_verified(config: &CleanverseConfig, address: &str, now: i64) -> Option<bool> {
    match query_apass(config, address).await {
        Ok(Some(record)) => Some(record.is_valid_at(now)),
        Ok(None) => Some(false),
        Err(e) => {
            warn!(error = %e, address, "Cleanverse: could not check the wallet's identity");
            None
        }
    }
}

/// May this wallet take part in a transfer of a specific asset?
///
/// Unlike [`query_apass`], this also accounts for the asset's own rules: an
/// identity may exist and still fall short of the required tier.
///
/// # Parameters
/// * `config` — credentials and API address
/// * `atoken` — the asset's address in their registry
/// * `address` — wallet address
pub async fn verify_apass(
    config: &CleanverseConfig,
    atoken: &str,
    address: &str,
) -> Result<TransferVerdict, CleanverseError> {
    let body = serde_json::json!({
        "chain": CHAIN_NAME,
        "atoken": atoken,
        "address": address,
    });
    let Some(data) = post_plain(config, "/verify_apass", body).await? else {
        return Ok(TransferVerdict::NoApass);
    };
    let code = data.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    Ok(TransferVerdict::from_code(code))
}

/// Sends a request whose body their API requires to be encrypted.
///
/// Same envelope as [`post_plain`] on the way back — only the way out differs.
async fn post_encrypted(
    config: &CleanverseConfig,
    path: &str,
    body: &impl serde::Serialize,
) -> Result<Option<Value>, CleanverseError> {
    if !config.is_configured() {
        return Err(CleanverseError::NotConfigured);
    }
    let plaintext = serde_json::to_string(body)
        .map_err(|e| CleanverseError::BadResponse(format!("{path}: cannot serialise: {e}")))?;
    let ciphertext = crate::cleanverse::crypto::encrypt_body(&config.api_key, &plaintext)?;
    post_plain(config, path, serde_json::json!({ "data": ciphertext })).await
}

/// Issues a lot as a verified asset.
///
/// Called the moment an operator approves a listing — that is the point of it.
/// Their track asks for compliance embedded *from the issuance stage*, and this
/// is that stage: the asset exists with its transfer rules attached before the
/// lot is visible to anyone.
///
/// # Parameters
/// * `config` — credentials and API address
/// * `launch` — what to issue and who may hold it
///
/// # Returns
/// * `Ok(String)` — the request id to poll with [`issue_status`]
/// * `Err(_)` — refused outright
pub async fn launch_asset(
    config: &CleanverseConfig,
    launch: &crate::cleanverse::types::AssetLaunch,
) -> Result<String, CleanverseError> {
    let Some(data) = post_encrypted(config, "/atoken/launch", launch).await? else {
        return Err(CleanverseError::Api {
            code: CODE_BUSINESS_FAIL.into(),
            message: "issuance was refused".into(),
        });
    };
    data.get("requestId")
        .and_then(|r| r.as_str())
        .map(str::to_string)
        .ok_or_else(|| CleanverseError::BadResponse("launch: no requestId in reply".into()))
}

/// Where an issuance request has got to.
///
/// Issuance is not instant: they review the application, then mint. Poll this
/// until it stops being pending.
///
/// # Parameters
/// * `config` — credentials and API address
/// * `request_id` — what [`launch_asset`] returned
pub async fn issue_status(
    config: &CleanverseConfig,
    request_id: &str,
) -> Result<crate::cleanverse::types::IssueStatus, CleanverseError> {
    use crate::cleanverse::types::IssueStatus;

    if !config.is_configured() {
        return Err(CleanverseError::NotConfigured);
    }
    let url = format!(
        "{}/atoken/query_apply_status/{}",
        config.base_url(),
        request_id.trim()
    );
    let resp = client()?
        .get(&url)
        .header("api-id", config.api_id.trim())
        .header("X-Request-ID", new_request_id())
        .send()
        .await
        .map_err(|e| CleanverseError::Http(format!("query_apply_status: {e}")))?;

    let payload: Value = resp
        .json()
        .await
        .map_err(|e| CleanverseError::BadResponse(format!("query_apply_status: {e}")))?;

    let code = payload.get("code").and_then(|c| c.as_str()).unwrap_or("");
    if code != CODE_OK {
        return Ok(IssueStatus::Failed(
            payload
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown")
                .to_string(),
        ));
    }

    let data = payload.get("data").cloned().unwrap_or_default();
    let status = data
        .get("applyStatus")
        .and_then(|s| s.as_str())
        .unwrap_or_default();
    match status {
        "ISSUED" => {
            let address = data
                .get("atokenAddress")
                .and_then(|a| a.as_str())
                .unwrap_or_default();
            if address.is_empty() {
                Err(CleanverseError::BadResponse(
                    "issued, but no asset address in the reply".into(),
                ))
            } else {
                Ok(IssueStatus::Issued(address.to_string()))
            }
        }
        "REJECTED" | "ISSUE_FAILED" => Ok(IssueStatus::Failed(status.to_string())),
        _ => Ok(IssueStatus::Pending),
    }
}

/// Is our contract registered as a compliance pool?
///
/// # Parameters
/// * `config` — credentials and API address
/// * `contract` — contract address
pub async fn is_pool_registered(
    config: &CleanverseConfig,
    contract: &str,
) -> Result<bool, CleanverseError> {
    let body = serde_json::json!({ "chain": CHAIN_NAME, "contract_address": contract });
    let Some(data) = post_plain(config, "/validator/is_register", body).await? else {
        return Ok(false);
    };
    Ok(data
        .get("registered")
        .and_then(|r| r.as_bool())
        .unwrap_or(false))
}
