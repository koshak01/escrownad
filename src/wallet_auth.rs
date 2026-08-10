//! EVM wallet sign-in (Phantom / MetaMask / any EIP-1193).
//!
//! Flow:
//! 1. Client: eth_requestAccounts → address
//! 2. `wallet_challenge` → nonce (bound to session + address)
//! 3. Client prefers `eth_signTypedData_v4` (EIP-712 Login); falls back to
//!    `personal_sign` of the nonce. Phantom EVM often refuses personal_sign
//!    with "invalid formatting" while typed data works.
//! 4. `wallet_login` → ecrecover, find_or_create user, Redis session + conn auth
//!
//! We never auto-redirect to Cleanverse magiclink after login — that page has
//! its own wallet UI we do not control. CVI is checked here; the market gate
//! shows a link if the identity is missing.

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
    /// Plain nonce string — also the personal_sign payload when that path is used.
    pub message: String,
    /// Same nonce, named explicitly for EIP-712 clients.
    pub nonce: String,
    /// Suggested primary method for the client: `typed` (EIP-712).
    pub prefer: &'static str,
    /// Monad testnet chain id — used in the EIP-712 domain.
    pub chain_id: u64,
}

/// Default chain id in the EIP-712 domain (Monad testnet).
pub const LOGIN_CHAIN_ID: u64 = 10143;

#[derive(Debug, Deserialize)]
pub struct WalletLoginParams {
    pub address: String,
    /// 0x-prefixed 65-byte signature (r||s||v).
    pub signature: String,
    /// `typed` = eth_signTypedData_v4 (EIP-712 Login); anything else = personal_sign.
    #[serde(default)]
    pub sign_kind: Option<String>,
    /// chainId the client put in the EIP-712 domain (default LOGIN_CHAIN_ID).
    #[serde(default)]
    pub chain_id: Option<u64>,
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
        // Pure hex nonce only. Phantom EVM is extremely picky about personal_sign
        // display payloads ("invalid formatting" / Chinese 登录失败). Multi-line
        // and even short prose with spaces have failed in the field; a 32-char
        // hex string is the safest message body. Client tries hex + plain
        // encodings; server ecrecover always uses this exact UTF-8 string.
        let message = nonce;

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

/// EIP-712 digest for our Login typed data (must match the client JSON exactly).
///
/// ```text
/// domain: { name: "EscrowNad", version: "1", chainId, verifyingContract: 0x0 }
/// types:  Login(address wallet, string nonce)
/// ```
fn eip712_login_digest(wallet: &str, nonce: &str, chain_id: u64) -> Result<[u8; 32], String> {
    let wallet = normalize_address(wallet)?;
    let addr_hex = wallet
        .strip_prefix("0x")
        .ok_or_else(|| "bad wallet".to_string())?;
    let addr_bytes = hex::decode(addr_hex).map_err(|_| "bad wallet hex".to_string())?;
    if addr_bytes.len() != 20 {
        return Err("wallet must be 20 bytes".into());
    }

    // type hashes — field order is part of the hash
    let domain_type_hash = keccak256(
        b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)",
    );
    let login_type_hash = keccak256(b"Login(address wallet,string nonce)");

    // domain separator
    let mut domain_data = Vec::with_capacity(32 * 5);
    domain_data.extend_from_slice(&domain_type_hash);
    domain_data.extend_from_slice(&keccak256(b"EscrowNad"));
    domain_data.extend_from_slice(&keccak256(b"1"));
    let mut chain_pad = [0u8; 32];
    chain_pad[24..].copy_from_slice(&chain_id.to_be_bytes());
    domain_data.extend_from_slice(&chain_pad);
    // verifyingContract = address(0)
    domain_data.extend_from_slice(&[0u8; 32]);
    let domain_sep = keccak256(&domain_data);

    // struct hash
    let mut struct_data = Vec::with_capacity(32 * 3);
    struct_data.extend_from_slice(&login_type_hash);
    let mut addr_pad = [0u8; 32];
    addr_pad[12..].copy_from_slice(&addr_bytes);
    struct_data.extend_from_slice(&addr_pad);
    struct_data.extend_from_slice(&keccak256(nonce.as_bytes()));
    let struct_hash = keccak256(&struct_data);

    // final digest
    let mut buf = Vec::with_capacity(2 + 32 + 32);
    buf.extend_from_slice(&[0x19, 0x01]);
    buf.extend_from_slice(&domain_sep);
    buf.extend_from_slice(&struct_hash);
    Ok(keccak256(&buf))
}

/// Recover address + pubkey from an EIP-712 Login signature.
fn recover_typed_login(
    wallet: &str,
    nonce: &str,
    chain_id: u64,
    signature_hex: &str,
) -> Result<(String, String), String> {
    let digest = eip712_login_digest(wallet, nonce, chain_id)?;
    recover_from_prehash(&digest, signature_hex)
}

fn recover_from_prehash(
    msg_hash: &[u8; 32],
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

    let vk = VerifyingKey::recover_from_prehash(msg_hash, &sig, recovery_id)
        .map_err(|_| "signature does not match this wallet".to_string())?;

    let point = vk.to_encoded_point(false);
    let uncompressed = point.as_bytes();
    if uncompressed.len() != 65 {
        return Err("unexpected public key length".into());
    }
    let hash = keccak256(&uncompressed[1..]);
    let addr = &hash[12..];
    let compressed = vk.to_encoded_point(true);
    Ok((
        format!("0x{}", hex::encode(addr)),
        format!("0x{}", hex::encode(compressed.as_bytes())),
    ))
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
    let msg_hash = eth_signed_message_hash(message);
    recover_from_prehash(&msg_hash, signature_hex)
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
    Ok(ChallengeResp {
        nonce: message.clone(),
        message,
        prefer: "typed",
        chain_id: LOGIN_CHAIN_ID,
    })
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

    let kind = params
        .sign_kind
        .as_deref()
        .unwrap_or("personal")
        .trim()
        .to_ascii_lowercase();
    let (recovered, pubkey) = if kind == "typed" || kind == "eip712" {
        let chain_id = params.chain_id.unwrap_or(LOGIN_CHAIN_ID);
        recover_typed_login(&address, &message, chain_id, &params.signature)?
    } else {
        recover_address_and_pubkey(&message, &params.signature)?
    };
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
