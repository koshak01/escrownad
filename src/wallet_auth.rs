//! EVM wallet sign-in (Monad / MetaMask).
//!
//! Flow (like tidex6, but ECDSA personal_sign):
//! 1. Client: eth_requestAccounts → address
//! 2. `wallet_challenge` → server message with nonce (bound to session + address)
//! 3. Client: personal_sign(message, address)
//! 4. `wallet_login` → ecrecover, find_or_create user, Redis session + conn auth
//!
//! Password login stays primary (forge modal). Wallet is the second path.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use forge_session::RedisClient;
use forge_ws::wsgate::WsConnAuth;
use k256::ecdsa::{RecoveryId, Signature as K256Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha3::{Digest, Keccak256};
use tokio::sync::Mutex;
use tracing::info;
use uuid::Uuid;

const CHALLENGE_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_PENDING: usize = 10_000;

// ── WS params / responses ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ChallengeParams {
    /// Checksum or lower-case 0x address from eth_requestAccounts.
    pub address: String,
}

#[derive(Debug, Serialize)]
pub struct ChallengeResp {
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct WalletLoginParams {
    pub address: String,
    /// 0x-prefixed 65-byte personal_sign signature (r||s||v).
    pub signature: String,
    #[serde(default)]
    pub redirect_after: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WalletLoginResp {
    pub ok: bool,
    pub redirect: String,
    pub address: String,
    pub is_new: bool,
    /// Does this wallet hold a valid verified identity?
    ///
    /// `None` — we could not ask. The interface then stays quiet rather than
    /// alarming anyone: the real refusal stands in the contract regardless,
    /// and that one cannot be bypassed.
    pub verified: Option<bool>,
    /// Where to go for an identity, when there is none.
    pub verify_url: Option<String>,
}

// ── Pending challenges ───────────────────────────────────────────────────────

struct Pending {
    message: String,
    address: String,
    issued: Instant,
}

pub struct WalletChallenges {
    pending: Mutex<HashMap<String, Pending>>,
}

impl WalletChallenges {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            pending: Mutex::new(HashMap::new()),
        })
    }

    pub async fn issue(&self, session_id: &str, address: &str) -> Result<String, String> {
        let address = normalize_address(address)?;
        let nonce = new_nonce()?;
        // Short ASCII only. Long multi-line challenges make Phantom EVM refuse
        // personal_sign with "invalid formatting" (the sign UI never opens).
        // Client still hex-encodes for Phantom; MetaMask accepts plain UTF-8.
        let message = format!("EscrowNad login {address} {nonce}");

        let mut map = self.pending.lock().await;
        prune(&mut map);
        if map.len() >= MAX_PENDING {
            return Err("too many sign-in attempts — try again later".into());
        }
        map.insert(
            session_id.to_string(),
            Pending {
                message: message.clone(),
                address,
                issued: Instant::now(),
            },
        );
        Ok(message)
    }

    /// Take pending challenge (one-shot). Returns (message, expected_address).
    pub async fn take(&self, session_id: &str) -> Result<(String, String), String> {
        let mut map = self.pending.lock().await;
        let pending = map
            .remove(session_id)
            .ok_or_else(|| "no sign-in in progress — request a challenge first".to_string())?;
        if pending.issued.elapsed() >= CHALLENGE_TTL {
            return Err("sign-in request expired — try again".into());
        }
        Ok((pending.message, pending.address))
    }
}

fn prune(map: &mut HashMap<String, Pending>) {
    map.retain(|_, p| p.issued.elapsed() < CHALLENGE_TTL);
}

fn new_nonce() -> Result<String, String> {
    use rand::RngCore;
    let mut buf = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    Ok(hex::encode(buf))
}

// ── Address + signature ──────────────────────────────────────────────────────

/// Lowercase 0x + 40 hex.
pub fn normalize_address(raw: &str) -> Result<String, String> {
    let s = raw.trim();
    let hex_part = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    if hex_part.len() != 40 || !hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err("invalid Ethereum address".into());
    }
    Ok(format!("0x{}", hex_part.to_ascii_lowercase()))
}

fn keccak256(data: &[u8]) -> [u8; 32] {
    let mut h = Keccak256::new();
    h.update(data);
    h.finalize().into()
}

fn eth_signed_message_hash(message: &str) -> [u8; 32] {
    let prefix = format!("\x19Ethereum Signed Message:\n{}", message.len());
    let mut buf = prefix.into_bytes();
    buf.extend_from_slice(message.as_bytes());
    keccak256(&buf)
}

/// Recovers the address from a `personal_sign` signature.
pub fn recover_address(message: &str, signature_hex: &str) -> Result<String, String> {
    Ok(recover_address_and_pubkey(message, signature_hex)?.0)
}

/// Recovers both the address and the **public key** from a signature.
///
/// The public key is what data can be encrypted with for this person alone:
/// only the holder of the private key — the wallet itself — can decrypt it.
/// It cannot be derived from the address, which is a one-way hash, but it can
/// be recovered from any signature, which is what we do at sign-in. Nothing
/// extra is asked of the user: they sign to get in anyway.
///
/// # Parameters
/// * `message` — the signed message
/// * `signature_hex` — the `r||s||v` signature in hex
///
/// # Returns
/// * `Ok((address, public key))` — the key compressed, 33 bytes as hex
/// * `Err(_)` — the signature did not parse, or does not match the message
pub fn recover_address_and_pubkey(
    message: &str,
    signature_hex: &str,
) -> Result<(String, String), String> {
    let sig_raw = parse_sig_hex(signature_hex)?;
    let mut v = sig_raw[64];
    if v >= 27 {
        v -= 27;
    }
    if v > 1 {
        return Err("invalid signature v".into());
    }
    let recovery_id = RecoveryId::try_from(v).map_err(|_| "invalid recovery id".to_string())?;
    let sig = K256Signature::from_slice(&sig_raw[..64])
        .map_err(|_| "invalid signature r||s".to_string())?;

    let msg_hash = eth_signed_message_hash(message);
    let vk = VerifyingKey::recover_from_prehash(&msg_hash, &sig, recovery_id)
        .map_err(|_| "signature does not match this wallet".to_string())?;

    let point = vk.to_encoded_point(false);
    let uncompressed = point.as_bytes(); // 0x04 || x || y
    if uncompressed.len() != 65 {
        return Err("unexpected public key length".into());
    }
    let hash = keccak256(&uncompressed[1..]);
    let addr = &hash[12..];
    // compressed form (0x02/0x03 || x) — smaller, and what crypto libraries expect
    let compressed = vk.to_encoded_point(true);
    Ok((
        format!("0x{}", hex::encode(addr)),
        format!("0x{}", hex::encode(compressed.as_bytes())),
    ))
}

fn parse_sig_hex(signature_hex: &str) -> Result<[u8; 65], String> {
    let s = signature_hex.trim();
    let hex_part = s
        .strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .unwrap_or(s);
    let bytes = hex::decode(hex_part).map_err(|_| "signature is not valid hex".to_string())?;
    bytes
        .try_into()
        .map_err(|_| "signature must be 65 bytes".to_string())
}

// ── Login orchestration ──────────────────────────────────────────────────────

pub async fn wallet_challenge<C: WsConnAuth>(
    challenges: &WalletChallenges,
    conn: &C,
    params: ChallengeParams,
) -> Result<ChallengeResp, String> {
    let session_id = conn
        .session_id()
        .ok_or_else(|| "session_id not set".to_string())?
        .to_string();
    let message = challenges.issue(&session_id, &params.address).await?;
    Ok(ChallengeResp { message })
}

pub async fn wallet_login<C: WsConnAuth>(
    challenges: &WalletChallenges,
    redis: &RedisClient,
    conn: &C,
    params: WalletLoginParams,
) -> Result<WalletLoginResp, String> {
    let session_id = conn
        .session_id()
        .ok_or_else(|| "session_id not set".to_string())?
        .to_string();

    let address = normalize_address(&params.address)?;
    let (message, expected) = challenges.take(&session_id).await?;
    if expected != address {
        return Err("this sign-in was started for a different wallet".into());
    }

    let (recovered, pubkey) = recover_address_and_pubkey(&message, &params.signature)?;
    if recovered != address {
        return Err("signature does not match this wallet".into());
    }

    let user = crate::app_context()
        .db
        .wallet_find_or_create(address.clone(), pubkey.clone())
        .await
        .map_err(|e| format!("wallet account: {e}"))?;

    if !user.usr_is_enable {
        return Err("account disabled".into());
    }

    let roles = forge_admin::env()
        .list_user_role_codes(user.usr_id)
        .await
        .unwrap_or_default();

    redis
        .update_user(
            &session_id,
            user.usr_id,
            &user.usr_hash,
            user.usr_is_staff,
            &roles,
        )
        .await
        .map_err(|e| format!("redis: {e}"))?;
    conn.set_auth(user.usr_id, user.usr_hash, user.usr_is_staff, roles);

    let redirect = sanitize_redirect(params.redirect_after.as_deref());

    // We ask about the identity but do not gate sign-in on it: a person should
    // get inside and see the requirement, rather than hit a blank wall. The
    // refusal happens where the money is — in the contract.
    let verified = check_identity(&address).await;

    info!(usr_id = user.usr_id, %address, is_new = user.is_new, ?verified, "wallet signed in");
    Ok(WalletLoginResp {
        ok: true,
        redirect,
        address,
        is_new: user.is_new,
        verified,
        verify_url: match verified {
            Some(true) => None,
            _ => Some(crate::cleanverse::core::MAGIC_LINK.to_string()),
        },
    })
}

/// Does this wallet hold a valid verified identity?
///
/// # Parameters
/// * `address` — wallet address
///
/// # Returns
/// * `Some(true)` / `Some(false)` — the identity is valid, or it is not
/// * `None` — the integration is not configured, or we could not ask
async fn check_identity(address: &str) -> Option<bool> {
    let constants = crate::app_context().db.get_constants().await.ok()?;
    let config: crate::cleanverse::types::CleanverseConfig = constants
        .get(crate::cleanverse::types::CLEANVERSE_CONSTANT)
        .ok()??;
    let now = chrono::Utc::now().timestamp();
    crate::cleanverse::core::is_verified(&config, address, now).await
}

/// Only same-origin relative paths (block open redirect //evil.com, https://…).
fn sanitize_redirect(raw: Option<&str>) -> String {
    let fallback = "/deals/".to_string();
    let Some(s) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return fallback;
    };
    if !s.starts_with('/') || s.starts_with("//") || s.contains('\\') {
        return fallback;
    }
    // No scheme-relative or control chars
    if s.contains("://") || s.chars().any(|c| c.is_control()) {
        return fallback;
    }
    s.to_string()
}

/// IPC row for find_or_create wallet user.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WalletUserRow {
    pub usr_id: i64,
    pub usr_hash: String,
    pub usr_is_staff: bool,
    pub usr_is_enable: bool,
    pub is_new: bool,
}

pub fn new_usr_hash() -> String {
    Uuid::new_v4().simple().to_string()
}

pub fn wallet_descr(address: &str) -> String {
    format!("wallet:{address}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_ok() {
        assert_eq!(
            normalize_address("0xAbCDEF0123456789AbCDEF0123456789aBcDef01").unwrap(),
            "0xabcdef0123456789abcdef0123456789abcdef01"
        );
    }

    #[test]
    fn normalize_bad() {
        assert!(normalize_address("not-an-address").is_err());
    }
}
