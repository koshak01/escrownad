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

/// A lot, expressed as a verified asset.
///
/// One lot is one token: it stands for the right to a specific block, and it is
/// issued the moment an operator approves the listing — before the lot ever
/// reaches the board. Every transfer of it is gated by the identity check
/// built into the token itself, so the asset cannot move to an unverified
/// wallet even outside our platform.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AssetLaunch {
    /// Chain name as their API spells it.
    pub chain: String,
    /// Human-readable name, e.g. `EscrowNad IP 194.246.124.0/23`.
    pub token_name: String,
    /// Short symbol, at most 12 characters.
    pub token_symbol: String,
    /// Six, to match every other token on this chain.
    pub decimals: u8,
    /// Wallet that will hold the admin role on the issued token.
    pub admin_address: String,
    /// Who is allowed to hold it — the same shape as a pool rule.
    pub rule: AssetRule,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// Issues the settlement token itself as a compliance-enforcing wrapper.
///
/// Where [`AssetLaunch`] mints the lot as a verified asset, this wraps an
/// existing ERC-20 — our settlement USDC — into a Wrapped A-Token that carries
/// the same transfer rule. The point is that settlement then lands as a token
/// whose every move is checked against the rule, not only the entry into a
/// deal: the compliance follows the money after release, not just up to it.
///
/// Their endpoint is `/atoken/launch_wrapped_atoken`, and the origin token is
/// named `origin_token_address` — distinct from the plain `/atoken/launch`,
/// which rejects an origin field because it mints a standalone asset.
#[derive(Debug, Clone, serde::Serialize)]
pub struct WrappedAssetLaunch {
    /// Chain name as their API spells it.
    pub chain: String,
    /// Display name of the wrapped A-Token.
    pub token_name: String,
    /// Symbol of the wrapper itself, not of the origin token.
    pub token_symbol: String,
    /// Six, to match the origin USDC.
    pub decimals: u8,
    /// Wallet that will hold the admin role on the wrapped token.
    pub admin_address: String,
    /// Who may hold or receive it — the same shape as an asset rule.
    pub rule: AssetRule,
    /// The origin (native) token being wrapped, e.g. Circle USDC on Monad.
    pub origin_token_address: String,
    /// URL of the origin token's icon.
    pub origin_token_icon: String,
    /// URL of the wrapped token's icon.
    pub icon: String,
}

/// Who may hold or receive an asset.
///
/// Field names are theirs: the API layer is snake_case while the on-chain
/// struct is camelCase, and the two mean the same thing.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AssetRule {
    pub allowed_group: String,
    pub allowed_sub_group: String,
    /// Minimum trust tier. Their check is *greater than*, so 1 means "any
    /// valid identity" rather than "tier at least 1".
    pub min_tier: u8,
    pub min_sub_tier: u8,
    /// Deprecated on their side; always false.
    pub is_black_list: bool,
    /// ISO country codes; empty means no restriction.
    pub countries: Vec<String>,
}

impl AssetRule {
    /// Any wallet holding a valid identity, from anywhere.
    ///
    /// This market is international and the resource itself carries no
    /// nationality — restricting by country would exclude legitimate holders
    /// for no gain.
    pub fn any_valid_identity() -> Self {
        Self {
            allowed_group: String::new(),
            allowed_sub_group: String::new(),
            min_tier: 1,
            min_sub_tier: 0,
            is_black_list: false,
            countries: Vec::new(),
        }
    }
}

/// Where an issuance request has got to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueStatus {
    /// Still being reviewed or minted.
    Pending,
    /// Done — the asset exists at this address.
    Issued(String),
    /// Refused or failed; the string carries their wording.
    Failed(String),
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
