//! Работа с EscrowLock на Monad: подпись и отправка release/refund,
//! чтение состояния сделки, конверсия сумм.
//!
//! Fund делает покупатель из браузера своим кошельком — здесь только то,
//! что подписывает наблюдатель.

use std::str::FromStr;

use alloy::primitives::{Address, B256, U256};
use alloy::providers::{Provider, ProviderBuilder};
use alloy::signers::local::PrivateKeySigner;
use alloy::sol;
use forge_core::hash::sha256_hex;
use forge_fixed_n::FixedN;
use tracing::{info, instrument, warn};

use crate::chain::types::{
    ChainConfig, ChainError, FIXED_N_TO_USDC_DIVISOR, LockState, REQUIRED_CONFIRMATIONS,
};

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

/// Идентификатор сделки в контракте — `bytes32` из постоянного хэша.
///
/// В цепь уходит только отпечаток: ни префикса сети, ни названий
/// организаций, ни описания лота там нет.
pub fn deal_id(del_hash: &str) -> B256 {
    B256::from_slice(&hex_to_bytes32(&sha256_hex(del_hash.as_bytes())))
}

/// Отпечаток условия сделки — что именно должен показать реестр.
///
/// Считается из предмета сделки (ресурс, вид, стороны перехода). В цепи
/// лежит только хэш, сами данные остаются в базе.
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

/// Ключ факта RIPE в виде `bytes32` — пишется в событие `Released`
/// как доказательство того, по какой строке реестра выпущены деньги.
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

/// Переводит сумму приложения (`FixedN<8>`) в базовые единицы USDC (6 знаков).
///
/// Деньги не округляются молча: если значение мельче одной базовой единицы
/// USDC (0.000001), возвращается ошибка — лучше отказ, чем тихо потерянные
/// доли.
///
/// # Параметры
/// * `amount` — сумма сделки в денежном типе приложения
///
/// # Возвращает
/// * `Ok(U256)` — сумма в базовых единицах USDC
/// * `Err(_)` — отрицательная сумма или потеря точности
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

/// Чтение состояния замка без приватного ключа.
///
/// Живёт в ws-демоне: тому нужно лишь убедиться, что деньги действительно
/// в замке. Ключ наблюдателя в ws не попадает — подписывать там нечего.
pub struct ChainReader {
    rpc_url: String,
    lock: Address,
}

impl ChainReader {
    /// Собирает читателя из настроек цепи (константа `chain` в базе).
    /// `None`, если цепь не в живом режиме или адрес замка не разобрался.
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
        })
    }

    /// Состояние сделки в замке: статус и сумма.
    pub async fn deal_state(&self, del_hash: &str) -> Result<(LockState, U256), ChainError> {
        let url = self
            .rpc_url
            .parse()
            .map_err(|e| ChainError::Config(format!("chain.rpc не разбирается: {e}")))?;
        let provider = ProviderBuilder::new().connect_http(url);
        let contract = IEscrowLock::new(self.lock, &provider);
        let d = contract
            .deals(deal_id(del_hash))
            .call()
            .await
            .map_err(|e| ChainError::Rpc(format!("deals: {e}")))?;
        Ok((LockState::from_u8(d.state), d.amount))
    }
}

/// Клиент цепи с ключом наблюдателя.
pub struct ObserverChain {
    config: ChainConfig,
    lock: Address,
}

impl ObserverChain {
    /// Собирает клиента подписи из настроек цепи.
    ///
    /// # Параметры
    /// * `config` — значение константы `chain` из базы
    ///
    /// # Возвращает
    /// * `Ok(_)` — есть и адрес замка, и ключ наблюдателя
    /// * `Err(_)` — чего-то не хватает, с указанием что именно
    pub fn new(config: ChainConfig) -> Result<Self, ChainError> {
        config.require_observer_key()?;
        let lock = parse_address("chain.lock", &config.lock)?;
        Ok(Self { config, lock })
    }

    /// Адрес наблюдателя, выведенный из приватного ключа.
    pub fn observer_address(&self) -> Result<Address, ChainError> {
        Ok(self.signer()?.address())
    }

    fn signer(&self) -> Result<PrivateKeySigner, ChainError> {
        PrivateKeySigner::from_str(self.config.observer_key.trim()).map_err(|e| {
            // текст ключа в лог не попадает
            ChainError::Config(format!("chain.observer_key не разбирается: {e}"))
        })
    }

    fn provider(&self) -> Result<impl Provider, ChainError> {
        let signer = self.signer()?;
        let url = self
            .config
            .rpc
            .parse()
            .map_err(|e| ChainError::Config(format!("chain.rpc не разбирается: {e}")))?;
        Ok(ProviderBuilder::new().wallet(signer).connect_http(url))
    }

    /// Выпускает деньги продавцу после совпадения с фактом RIPE.
    ///
    /// Ждёт подтверждения на глубину `REQUIRED_CONFIRMATIONS` и проверяет
    /// статус исполнения: на Monad попадание в блок ещё не значит успех.
    ///
    /// # Параметры
    /// * `del_hash` — постоянный хэш сделки
    /// * `ripe_key` — ключ найденной строки реестра
    ///
    /// # Возвращает
    /// * `Ok(String)` — хэш подтверждённой транзакции
    /// * `Err(_)` — сеть недоступна или транзакция откатилась
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
            warn!(tx = %tx_hash, "release отклонён при исполнении");
            return Err(ChainError::TxReverted { tx_hash });
        }
        info!(tx = %tx_hash, gas = receipt.gas_used, "release подтверждён");
        Ok(tx_hash)
    }

    /// Возвращает деньги покупателю (факта нет, серая зона).
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
        info!(tx = %tx_hash, "refund подтверждён");
        Ok(tx_hash)
    }

    /// Читает состояние сделки в контракте.
    ///
    /// Нужно перед выпуском: деньги должны реально лежать в замке, а не
    /// «по мнению базы».
    #[instrument(skip(self), fields(deal = %del_hash))]
    pub async fn deal_state(&self, del_hash: &str) -> Result<(LockState, U256), ChainError> {
        let provider = self.provider()?;
        let contract = IEscrowLock::new(self.lock, &provider);
        let d = contract
            .deals(deal_id(del_hash))
            .call()
            .await
            .map_err(|e| ChainError::Rpc(format!("deals: {e}")))?;
        Ok((LockState::from_u8(d.state), d.amount))
    }
}

fn parse_address(what: &'static str, value: &str) -> Result<Address, ChainError> {
    Address::from_str(value.trim()).map_err(|_| ChainError::BadAddress {
        what,
        value: value.to_string(),
    })
}
