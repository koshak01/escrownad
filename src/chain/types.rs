//! Chain-layer types: operating mode, network settings, errors, deal state.

use thiserror::Error;

/// USDC everywhere: 6 decimal places.
pub const USDC_DECIMALS: u8 = 6;

/// The application's money type carries 8 decimals, USDC carries 6.
/// The difference is exactly a factor of 100.
pub const FIXED_N_TO_USDC_DIVISOR: i64 = 100;

/// How many blocks to wait before treating a result as final.
///
/// On Monad execution is decoupled from consensus: a receipt appears while the
/// block is merely proposed, and execution is guaranteed once the block is
/// `D = 3` deep. We wait exactly that long — otherwise a "success" can turn
/// out to be a revert.
pub const REQUIRED_CONFIRMATIONS: u64 = 3;

/// Settlement mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainMode {
    /// No chain: transactions are stand-in strings. For local work.
    Mock,
    /// A real chain: we sign and send transactions.
    Live,
}

impl ChainMode {
    pub fn is_live(self) -> bool {
        self == Self::Live
    }
}

/// Key in the `constants` table holding the chain settings.
///
/// Everything lives in the database and is edited through `/admin/constants/`
/// — like the Telegram credentials and every other secret of this project.
/// No environment variables, no files.
pub const CHAIN_CONSTANT: &str = "chain";

/// Network connection parameters and contract addresses.
///
/// The `chain` constant holds an object:
/// ```json
/// {
///   "mode": "live",
///   "rpc": "https://testnet-rpc.monad.xyz",
///   "chain_id": 10143,
///   "usdc": "0x534b2f3A21130d7a60830c2Df862319e593943A3",
///   "lock": "0x3CB2C5EA954C7711EfF621A784CD096E4E580be5",
///   "observer_key": "0x..."
/// }
/// ```
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ChainConfig {
    /// `live` — work against a real chain; anything else means mock.
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub rpc: String,
    #[serde(default)]
    pub chain_id: u64,
    /// USDC address (6 decimals).
    #[serde(default)]
    pub usdc: String,
    /// EscrowLock address.
    #[serde(default)]
    pub lock: String,
    /// Platform treasury — where the fee goes.
    #[serde(default)]
    pub treasury: String,
    /// Insurance fund — a separate address whose balance we show on the site.
    #[serde(default)]
    pub insurance: String,
    /// The observer's private key — it signs release and refund.
    /// Only the observer process needs it.
    #[serde(default)]
    pub observer_key: String,
}

impl ChainConfig {
    /// Pulls the chain settings out of the constants map.
    ///
    /// # Parameters
    /// * `constants` — snapshot of the `constants` table (key → value)
    ///
    /// # Returns
    /// * `Some(_)` — the `chain` constant exists and parses
    /// * `None` — no such constant, or its format is unreadable
    pub fn from_constants(
        constants: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Option<Self> {
        let raw = constants.get(CHAIN_CONSTANT)?;
        match serde_json::from_value::<Self>(raw.clone()) {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::warn!(error = %e, "the `chain` constant does not parse — falling back to mock");
                None
            }
        }
    }

    /// Mode implied by the constant's contents.
    ///
    /// Live mode needs the lock address: reading state and handing payment
    /// parameters to the browser requires no keys at all. Only the party that
    /// signs checks for a key — see [`ChainConfig::require_observer_key`].
    pub fn mode(&self) -> ChainMode {
        let wants_live = self.mode.trim().eq_ignore_ascii_case("live");
        if wants_live && !self.lock.trim().is_empty() {
            ChainMode::Live
        } else {
            ChainMode::Mock
        }
    }

    /// Checks that everything needed to sign as the observer is present.
    pub fn require_observer_key(&self) -> Result<(), ChainError> {
        if self.observer_key.trim().is_empty() {
            return Err(ChainError::Config(
                "observer_key is empty in the `chain` constant — fill it in via /admin/constants/".into(),
            ));
        }
        if self.rpc.trim().is_empty() {
            return Err(ChainError::Config("rpc is empty in the `chain` constant".into()));
        }
        Ok(())
    }
}

/// What the contract holds for a deal.
///
/// The parties are read back deliberately. `fund` takes the seller as an
/// argument, so whoever paid decided who gets the money — and our database
/// must not simply believe that the two agree. Both are compared before a deal
/// is marked paid and before anything is released.
#[derive(Debug, Clone)]
pub struct LockedDeal {
    pub state: LockState,
    /// Deal price in USDC base units, without the fees.
    pub amount: u128,
    /// Who the contract will pay on release, lower-case hex.
    pub seller: String,
    /// Who actually funded it, lower-case hex.
    pub buyer: String,
}

impl LockedDeal {
    /// Is this address the one the contract recorded as the payer?
    pub fn buyer_is(&self, address: &str) -> bool {
        !self.buyer.is_empty() && self.buyer.eq_ignore_ascii_case(address.trim())
    }

    /// Is this address the one the contract will pay?
    pub fn seller_is(&self, address: &str) -> bool {
        !self.seller.is_empty() && self.seller.eq_ignore_ascii_case(address.trim())
    }
}

/// Deal state in the contract — mirrors `enum State` from EscrowLock.sol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockState {
    None,
    Funded,
    Released,
    Refunded,
}

impl LockState {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Funded,
            2 => Self::Released,
            3 => Self::Refunded,
            _ => Self::None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Funded => "funded",
            Self::Released => "released",
            Self::Refunded => "refunded",
        }
    }
}

#[derive(Debug, Error)]
pub enum ChainError {
    #[error("chain configuration: {0}")]
    Config(String),

    #[error("malformed {what} address: {value}")]
    BadAddress { what: &'static str, value: String },

    #[error("RPC: {0}")]
    Rpc(String),

    /// The transaction made it into a block, but execution reverted.
    /// On Monad this is ordinary: inclusion in a block is not success.
    #[error("transaction {tx_hash} reverted during execution")]
    TxReverted { tx_hash: String },

    #[error("amount is negative: {0}")]
    NegativeAmount(i64),

    #[error(
        "amount {0} is not representable in USDC: below 0.000001 USDC (we would lose the remainder in transfer)"
    )]
    SubUnitAmount(i64),

    #[error("amount {0} does not fit in 256 bits")]
    AmountOverflow(i64),
}
