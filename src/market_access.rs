//! Market access: wallet session + Cleanverse verified identity (CVI).
//!
//! Public pages (home, about, oracle) stay open. The deal board, deal cards,
//! listing form, cabinet and every deal mutation require both:
//!   1. a signed-in wallet (session), and
//!   2. a valid CVI on that wallet.
//!
//! Without CVI the UI points at Cleanverse's own flow (magic link) — we do not
//! run KYC ourselves.

use serde::Serialize;

use crate::app_context;
use crate::cleanverse::core::{MAGIC_LINK, is_verified};
use crate::cleanverse::types::{CLEANVERSE_CONSTANT, CleanverseConfig};

/// Who may see the market and act on deals.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MarketAccess {
    /// Session + valid CVI — board and actions are open.
    Allowed,
    /// No wallet session (or password-only account without a linked wallet).
    NeedConnect,
    /// Wallet is in, but Cleanverse reports no valid identity.
    NeedIdentity {
        /// Where the person obtains a CVI (Cleanverse self-service).
        verify_url: String,
    },
}

impl MarketAccess {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed)
    }

    /// Template-friendly flags (Tera has no rich match on enums).
    pub fn insert_template_flags(&self, out: &mut TemplateFlags) {
        *out = match self {
            Self::Allowed => TemplateFlags {
                market_allowed: true,
                need_connect: false,
                need_identity: false,
                verify_url: String::new(),
            },
            Self::NeedConnect => TemplateFlags {
                market_allowed: false,
                need_connect: true,
                need_identity: false,
                verify_url: String::new(),
            },
            Self::NeedIdentity { verify_url } => TemplateFlags {
                market_allowed: false,
                need_connect: false,
                need_identity: true,
                verify_url: verify_url.clone(),
            },
        };
    }
}

/// Booleans for Tera templates.
#[derive(Debug, Clone, Default, Serialize)]
pub struct TemplateFlags {
    pub market_allowed: bool,
    pub need_connect: bool,
    pub need_identity: bool,
    pub verify_url: String,
}

/// Resolves market access for the current session user.
///
/// # Parameters
/// * `usr_id` — `None` when anonymous
///
/// # Returns
/// * [`MarketAccess::Allowed`] — may see deals and act
/// * [`MarketAccess::NeedConnect`] — must connect a wallet first
/// * [`MarketAccess::NeedIdentity`] — must obtain a CVI (link included)
///
/// When Cleanverse is **not configured** at all (local mock without credentials),
/// access is allowed so the stack still runs for development. Production always
/// has the constant set.
pub async fn resolve(usr_id: Option<i64>) -> MarketAccess {
    let Some(uid) = usr_id else {
        return MarketAccess::NeedConnect;
    };

    let address = match app_context().db.wallet_address_for_user(uid).await {
        Ok(Some(a)) if !a.trim().is_empty() => a,
        _ => return MarketAccess::NeedConnect,
    };

    let config = match cleanverse_config().await {
        Some(c) if c.is_configured() => c,
        // No integration → do not brick local/dev.
        _ => return MarketAccess::Allowed,
    };

    let now = chrono::Utc::now().timestamp();
    match is_verified(&config, &address, now).await {
        Some(true) => MarketAccess::Allowed,
        // API down or no identity: same hard gate — market stays closed.
        Some(false) | None => MarketAccess::NeedIdentity {
            verify_url: MAGIC_LINK.to_string(),
        },
    }
}

/// For WS mutations: actor must be signed in with a CVI wallet.
///
/// # Errors
/// English strings for toasts — never Russian.
pub async fn require_cvi(usr_id: Option<i64>) -> Result<i64, String> {
    match resolve(usr_id).await {
        MarketAccess::Allowed => usr_id.ok_or_else(|| "authentication required".into()),
        MarketAccess::NeedConnect => Err(
            "Connect your wallet and sign the message to use the market.".into(),
        ),
        MarketAccess::NeedIdentity { .. } => Err(
            "A Cleanverse verified identity is required. Get one, then try again.".into(),
        ),
    }
}

async fn cleanverse_config() -> Option<CleanverseConfig> {
    let constants = app_context().db.get_constants().await.ok()?;
    constants.get(CLEANVERSE_CONSTANT).ok()?
}
