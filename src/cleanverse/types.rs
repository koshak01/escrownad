//! Cleanverse integration types: settings, a wallet's identity record, errors.

use thiserror::Error;

/// Key in the `constants` table holding the Cleanverse credentials.
///
/// Like `chain`, everything lives in the database and is edited through
/// `/admin/constants/`. No environment variables, no files — that is the
/// convention of this project.
pub const CLEANVERSE_CONSTANT: &str = "cleanverse";

/// Our chain as their API names it. Case does not matter, but we spell it as
/// the documentation does.
pub const CHAIN_NAME: &str = "monad";

/// Hackathon sandbox. Production is `https://api.cleanverse.com/api/cooperate`.
pub const SANDBOX_URL: &str = "https://uatapi.cleanverse.com/api/cooperate";

/// Credentials and API address.
///
/// The `cleanverse` constant holds an object:
/// ```json
/// {
///   "api_id":  "APP2026...",
///   "api_key": "<base64 encryption key; never leaves this machine>",
///   "base_url": "https://uatapi.cleanverse.com/api/cooperate"
/// }
/// ```
///
/// `api_id` goes out as a header and identifies the application. `api_key`
/// is **never transmitted**: it encrypts the bodies of those requests that
/// require it.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CleanverseConfig {
    #[serde(default)]
    pub api_id: String,
    #[serde(default)]
    pub api_key: String,
    /// API address; empty means the sandbox.
    #[serde(default)]
    pub base_url: String,
}

impl CleanverseConfig {
    /// API address: from the constant, or the sandbox by default.
    pub fn base_url(&self) -> &str {
        let url = self.base_url.trim();
        if url.is_empty() { SANDBOX_URL } else { url }
    }

    /// Is the integration configured at all?
    pub fn is_configured(&self) -> bool {
        !self.api_id.trim().is_empty()
    }
}

/// What the identity registry knows about a wallet.
///
/// Comes from `query_apass`. Fields are named as they are on their side —
/// `tier` and friends arrive as strings even though they are numbers.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct ApassRecord {
    /// Trust tier, 0–99. A string, because that is how the API sends it.
    #[serde(default)]
    pub tier: String,
    #[serde(default)]
    pub group: String,
    #[serde(default, rename = "subTier")]
    pub sub_tier: serde_json::Value,
    #[serde(default, rename = "subGroup")]
    pub sub_group: String,
    /// 1 — the identity is active. Anything else means frozen or revoked.
    #[serde(default)]
    pub status: i64,
    /// Valid until, unix seconds.
    #[serde(default, rename = "expirationTime")]
    pub expiration_time: i64,
    /// Fingerprint of the identity check that was passed.
    #[serde(default, rename = "currentKycHash")]
    pub kyc_hash: Option<String>,
    /// Countries bound to the identity.
    #[serde(default)]
    pub countries: Vec<String>,
}

impl ApassRecord {
    /// Is the identity valid right now?
    ///
    /// Decided on the structure of the reply, never on the text of a message:
    /// an active status and an unexpired deadline. Wording in `message`
    /// changes; fields do not.
    ///
    /// # Parameters
    /// * `now` — the moment to check against, unix seconds
    pub fn is_valid_at(&self, now: i64) -> bool {
        self.status == 1 && (self.expiration_time == 0 || self.expiration_time > now)
    }

    /// Trust tier as a number; an unreadable value counts as zero.
    pub fn tier_num(&self) -> u8 {
        self.tier.trim().parse::<u8>().unwrap_or(0)
    }
}

/// May a wallet take part in a transfer of a specific asset?
///
/// The reply of `verify_apass`. The numeric code is more reliable than the
/// text, so that is what we branch on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferVerdict {
    /// No such asset.
    UnknownAsset,
    /// The wallet holds no identity.
    NoApass,
    /// An identity exists, but the transfer is not allowed — expired, frozen,
    /// or failing the asset's own rule.
    NotAllowed,
    /// Allowed.
    Allowed,
    /// A code we do not know — treat it as a refusal.
    Unknown(i64),
}

impl TransferVerdict {
    pub fn from_code(code: i64) -> Self {
        match code {
            1 => Self::UnknownAsset,
            2 => Self::NoApass,
            3 => Self::NotAllowed,
            4 => Self::Allowed,
            other => Self::Unknown(other),
        }
    }

    pub fn is_allowed(self) -> bool {
        self == Self::Allowed
    }
}

#[derive(Debug, Error)]
pub enum CleanverseError {
    #[error("integration not configured: fill in the `cleanverse` constant via /admin/constants/")]
    NotConfigured,

    #[error("HTTP: {0}")]
    Http(String),

    #[error("reply did not parse: {0}")]
    BadResponse(String),

    /// Their API answers HTTP 200 on refusals too — the real outcome is in `code`.
    #[error("Cleanverse refused: code {code}, {message}")]
    Api { code: String, message: String },

    #[error("encrypting the request body: {0}")]
    Crypto(String),
}
