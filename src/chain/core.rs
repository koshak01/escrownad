//! Working with EscrowLock on Monad: signing and sending release and refund,
//! reading deal state, converting amounts.
//!
//! Funding is done by the buyer from the browser with their own wallet — this
//! file holds only what the observer signs.

use std::str::FromStr;

use alloy::primitives::{Address, B256, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::signers::local::PrivateKeySigner;
use alloy::sol;
use forge_core::hash::sha256_hex;
use forge_fixed_n::FixedN;
use tracing::{info, instrument, warn};

use crate::chain::types::{
    ChainConfig, ChainError, FIXED_N_TO_USDC_DIVISOR, LockState, LockedDeal,
    REQUIRED_CONFIRMATIONS,
};

sol! {
    #[sol(rpc)]
    interface IERC20 {
        function balanceOf(address account) external view returns (uint256);
    }
}

sol! {
    #[sol(rpc)]
    interface IEscrowLock {
        function release(bytes32 dealId, bytes32 ripeKey) external;
        function refund(bytes32 dealId) external;
        function deals(bytes32 dealId) external view returns (
            address seller,
            address buyer,
            uint256 amount,
            uint256 fee,
            uint256 insuranceFee,
            uint64 deadline,
            uint8 state,
            bytes32 conditionHash
        );
        function quote(uint256 amount) external view returns (
            uint256 total,
            uint256 fee,
            uint256 insuranceFee
        );
        function feeBps() external view returns (uint16);
    }
}

/// The deal's identifier in the contract — `bytes32` derived from its permanent hash.
///
/// Only the fingerprint goes on chain: no network prefix, no organisation
/// names, no listing text.
pub fn deal_id(del_hash: &str) -> B256 {
    B256::from_slice(&hex_to_bytes32(&sha256_hex(del_hash.as_bytes())))
}

/// Fingerprint of the deal's condition — what exactly the registry must show.
///
/// Derived from the subject of the deal (resource, kind, parties to the
/// transfer). Only the hash lives on chain; the data itself stays in the
/// database.
pub fn condition_hash(prefix: &str, kind: &str, from_org: &str, to_org: &str) -> B256 {
    let key = format!(
        "{}|{}|{}|{}",
        prefix.trim().to_lowercase(),
        kind.trim().to_uppercase(),
        from_org.trim().to_lowercase(),
        to_org.trim().to_lowercase()
    );
    B256::from_slice(&hex_to_bytes32(&sha256_hex(key.as_bytes())))
}

/// The registry fact key as `bytes32` — written into the `Released` event as
/// proof of which registry row the money was released against.
pub fn ripe_key_hash(ripe_key: &str) -> B256 {
    B256::from_slice(&hex_to_bytes32(&sha256_hex(ripe_key.as_bytes())))
}

fn hex_to_bytes32(hex: &str) -> [u8; 32] {
    let mut out = [0u8; 32];
    for (i, slot) in out.iter_mut().enumerate() {
        let at = i * 2;
        *slot = hex
            .get(at..at + 2)
            .and_then(|pair| u8::from_str_radix(pair, 16).ok())
            .unwrap_or(0);
    }
    out
}

/// Converts an application amount (`FixedN<8>`) into USDC base units (6 decimals).
///
/// Money is never rounded silently: a value finer than one USDC base unit
/// (0.000001) returns an error. A refusal beats quietly losing fractions.
///
/// # Parameters
/// * `amount` — the deal amount in the application's money type
///
/// # Returns
/// * `Ok(U256)` — the amount in USDC base units
/// * `Err(_)` — a negative amount, or a loss of precision
pub fn usdc_units(amount: FixedN<8>) -> Result<U256, ChainError> {
    let raw = amount.raw();
    if raw < 0 {
        return Err(ChainError::NegativeAmount(raw));
    }
    if raw % FIXED_N_TO_USDC_DIVISOR != 0 {
        return Err(ChainError::SubUnitAmount(raw));
    }
    Ok(U256::from(raw / FIXED_N_TO_USDC_DIVISOR))
}

/// Reading lock state without a private key.
///
/// Lives in the ws daemon, which only needs to confirm that the money really
/// is in the lock. The observer's key never reaches ws — there is nothing to
/// sign there.
pub struct ChainReader {
    rpc_url: String,
    lock: Address,
    usdc: Option<Address>,
    insurance: Option<Address>,
}

impl ChainReader {
    /// Builds a reader from the chain settings (the `chain` constant).
    /// `None` when the chain is not live, or the lock address does not parse.
    pub fn new(config: &ChainConfig) -> Option<Self> {
        if !config.mode().is_live() {
            return None;
        }
        let lock = Address::from_str(config.lock.trim()).ok()?;
        if config.rpc.trim().is_empty() {
            return None;
        }
        Some(Self {
            rpc_url: config.rpc.clone(),
            lock,
            usdc: Address::from_str(config.usdc.trim()).ok(),
            insurance: Address::from_str(config.insurance.trim()).ok(),
        })
    }

    /// Balance of any address: USDC and the chain's native coin.
    ///
    /// Needed by the operator during review: it shows what the person holds
    /// and whether the wallet looks like an empty throwaway.
    ///
    /// # Returns
    /// * `(usdc, native)` in base units: USDC has 6 decimals, MON has 18
    pub async fn wallet_balances(&self, who: &str) -> Result<(U256, U256), ChainError> {
        let addr = Address::from_str(who.trim()).map_err(|_| ChainError::BadAddress {
            what: "wallet",
            value: who.to_string(),
        })?;
        let usdc = self
            .usdc
            .ok_or_else(|| ChainError::Config("the `chain` constant has no usdc address".into()))?;
        let url = self
            .rpc_url
            .parse()
            .map_err(|e| ChainError::Config(format!("chain.rpc does not parse: {e}")))?;
        let provider = ProviderBuilder::new().connect_http(url);
        let token = IERC20::new(usdc, &provider);
        let usdc_balance = token
            .balanceOf(addr)
            .call()
            .await
            .map_err(|e| ChainError::Rpc(format!("balanceOf: {e}")))?;
        let native = provider
            .get_balance(addr)
            .await
            .map_err(|e| ChainError::Rpc(format!("getBalance: {e}")))?;
        Ok((usdc_balance, native))
    }

    /// Insurance fund balance in USDC, as it stands on chain.
    ///
    /// The fund sits at a separate address, so the figure is not "according
    /// to our records" but checkable: anyone can open the explorer and see the
    /// same number.
    ///
    /// # Returns
    /// * `Ok(U256)` — the balance in USDC base units
    /// * `Err(_)` — the network is unreachable, or the addresses are unset
    pub async fn insurance_balance(&self) -> Result<U256, ChainError> {
        let (usdc, fund) = match (self.usdc, self.insurance) {
            (Some(u), Some(f)) => (u, f),
            _ => {
                return Err(ChainError::Config(
                    "the `chain` constant has no usdc or insurance address".into(),
                ));
            }
        };
        let url = self
            .rpc_url
            .parse()
            .map_err(|e| ChainError::Config(format!("chain.rpc does not parse: {e}")))?;
        let provider = ProviderBuilder::new().connect_http(url);
        let token = IERC20::new(usdc, &provider);
        let balance = token
            .balanceOf(fund)
            .call()
            .await
            .map_err(|e| ChainError::Rpc(format!("balanceOf: {e}")))?;
        Ok(balance)
    }

    /// Deal state in the lock: status and amount.
    pub async fn deal_state(&self, del_hash: &str) -> Result<(LockState, U256), ChainError> {
        let d = self.locked_deal(del_hash).await?;
        Ok((d.state, U256::from(d.amount)))
    }

    /// The whole record the contract holds, parties included.
    ///
    /// Use this wherever a decision depends on **who** paid or **who** gets
    /// paid — the state and the amount alone cannot answer that.
    pub async fn locked_deal(&self, del_hash: &str) -> Result<LockedDeal, ChainError> {
        let url = self
            .rpc_url
            .parse()
            .map_err(|e| ChainError::Config(format!("chain.rpc does not parse: {e}")))?;
        let provider = ProviderBuilder::new().connect_http(url);
        let contract = IEscrowLock::new(self.lock, &provider);
        let d = contract
            .deals(deal_id(del_hash))
            .call()
            .await
            .map_err(|e| ChainError::Rpc(format!("deals: {e}")))?;
        Ok(LockedDeal {
            state: LockState::from_u8(d.state),
            amount: d.amount.to::<u128>(),
            seller: format!("{:#x}", d.seller),
            buyer: format!("{:#x}", d.buyer),
        })
    }
}

/// Chain client holding the observer's key.
pub struct ObserverChain {
    config: ChainConfig,
    lock: Address,
}

impl ObserverChain {
    /// Builds a signing client from the chain settings.
    ///
    /// # Parameters
    /// * `config` — the value of the `chain` constant
    ///
    /// # Returns
    /// * `Ok(_)` — both the lock address and the observer key are present
    /// * `Err(_)` — something is missing, naming what
    pub fn new(config: ChainConfig) -> Result<Self, ChainError> {
        config.require_observer_key()?;
        let lock = parse_address("chain.lock", &config.lock)?;
        Ok(Self { config, lock })
    }

    /// The observer's address, derived from the private key.
    pub fn observer_address(&self) -> Result<Address, ChainError> {
        Ok(self.signer()?.address())
    }

    fn signer(&self) -> Result<PrivateKeySigner, ChainError> {
        PrivateKeySigner::from_str(self.config.observer_key.trim()).map_err(|e| {
            // the key material never reaches the log
            ChainError::Config(format!("chain.observer_key does not parse: {e}"))
        })
    }

    fn provider(&self) -> Result<impl Provider, ChainError> {
        let signer = self.signer()?;
        let url = self
            .config
            .rpc
            .parse()
            .map_err(|e| ChainError::Config(format!("chain.rpc does not parse: {e}")))?;
        Ok(ProviderBuilder::new().wallet(signer).connect_http(url))
    }

    /// Releases the money to the seller once the registry fact matched.
    ///
    /// Waits `REQUIRED_CONFIRMATIONS` deep and checks the execution status:
    /// on Monad, landing in a block does not yet mean success.
    ///
    /// # Parameters
    /// * `del_hash` — the deal's permanent hash
    /// * `ripe_key` — key of the registry row that matched
    ///
    /// # Returns
    /// * `Ok(String)` — hash of the confirmed transaction
    /// * `Err(_)` — the network is unreachable, or the transaction reverted
    #[instrument(skip(self), fields(deal = %del_hash))]
    pub async fn release(&self, del_hash: &str, ripe_key: &str) -> Result<String, ChainError> {
        let provider = self.provider()?;
        let contract = IEscrowLock::new(self.lock, &provider);
        let id = deal_id(del_hash);

        let pending = contract
            .release(id, ripe_key_hash(ripe_key))
            .send()
            .await
            .map_err(|e| ChainError::Rpc(format!("release: {e}")))?;

        let receipt = pending
            .with_required_confirmations(REQUIRED_CONFIRMATIONS)
            .get_receipt()
            .await
            .map_err(|e| ChainError::Rpc(format!("release receipt: {e}")))?;

        let tx_hash = receipt.transaction_hash.to_string();
        if !receipt.status() {
            warn!(tx = %tx_hash, "release reverted during execution");
            return Err(ChainError::TxReverted { tx_hash });
        }
        info!(tx = %tx_hash, gas = receipt.gas_used, "release confirmed");
        Ok(tx_hash)
    }

    /// Returns the money to the buyer (no fact, or a grey area).
    #[instrument(skip(self), fields(deal = %del_hash))]
    pub async fn refund(&self, del_hash: &str) -> Result<String, ChainError> {
        let provider = self.provider()?;
        let contract = IEscrowLock::new(self.lock, &provider);

        let pending = contract
            .refund(deal_id(del_hash))
            .send()
            .await
            .map_err(|e| ChainError::Rpc(format!("refund: {e}")))?;

        let receipt = pending
            .with_required_confirmations(REQUIRED_CONFIRMATIONS)
            .get_receipt()
            .await
            .map_err(|e| ChainError::Rpc(format!("refund receipt: {e}")))?;

        let tx_hash = receipt.transaction_hash.to_string();
        if !receipt.status() {
            return Err(ChainError::TxReverted { tx_hash });
        }
        info!(tx = %tx_hash, "refund confirmed");
        Ok(tx_hash)
    }

    /// Reads the deal's state from the contract.
    ///
    /// Needed before releasing: the money must actually be in the lock, not
    /// merely so according to our database.
    #[instrument(skip(self), fields(deal = %del_hash))]
    pub async fn deal_state(&self, del_hash: &str) -> Result<(LockState, U256), ChainError> {
        let d = self.locked_deal(del_hash).await?;
        Ok((d.state, U256::from(d.amount)))
    }

    /// The whole record the contract holds, parties included.
    ///
    /// The observer checks the seller against it before releasing: `fund` takes
    /// the seller as an argument, so a buyer could name any address at all.
    /// Paying out to whoever the payer chose, without comparing against the
    /// listing, would hand the money to a stranger.
    #[instrument(skip(self), fields(deal = %del_hash))]
    pub async fn locked_deal(&self, del_hash: &str) -> Result<LockedDeal, ChainError> {
        let provider = self.provider()?;
        let contract = IEscrowLock::new(self.lock, &provider);
        let d = contract
            .deals(deal_id(del_hash))
            .call()
            .await
            .map_err(|e| ChainError::Rpc(format!("deals: {e}")))?;
        Ok(LockedDeal {
            state: LockState::from_u8(d.state),
            amount: d.amount.to::<u128>(),
            seller: format!("{:#x}", d.seller),
            buyer: format!("{:#x}", d.buyer),
        })
    }
}

fn parse_address(what: &'static str, value: &str) -> Result<Address, ChainError> {
    Address::from_str(value.trim()).map_err(|_| ChainError::BadAddress {
        what,
        value: value.to_string(),
    })
}
