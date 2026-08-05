//! Типы слоя цепи: режим работы, настройки сети, ошибки, состояние сделки.

use thiserror::Error;

/// USDC everywhere: 6 знаков после запятой.
pub const USDC_DECIMALS: u8 = 6;

/// Денежный тип приложения хранится с 8 знаками, USDC — с 6.
/// Разница ровно в 100 раз.
pub const FIXED_N_TO_USDC_DIVISOR: i64 = 100;

/// Сколько блоков ждать, прежде чем считать результат окончательным.
///
/// На Monad исполнение отделено от консенсуса: квитанция появляется, когда
/// блок только предложен, а исполнение гарантировано, когда блок ушёл на
/// `D = 3` вглубь. Ждём именно столько — иначе «успех» может оказаться
/// откатом.
pub const REQUIRED_CONFIRMATIONS: u64 = 3;

/// Режим расчётов.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainMode {
    /// Ончейна нет: транзакции подменяются строками. Для локальной работы.
    Mock,
    /// Настоящая цепь: подписываем и отправляем транзакции.
    Live,
}

impl ChainMode {
    /// Читает режим из окружения (`CHAIN_MODE=live|mock`).
    ///
    /// Живой режим требует только адрес контракта: читать состояние замка и
    /// отдавать браузеру параметры оплаты можно без всяких ключей. Приватный
    /// ключ нужен единственному процессу — наблюдателю, и его наличие
    /// проверяет [`ChainConfig::from_env`], а не этот метод.
    pub fn from_env() -> Self {
        let requested = std::env::var("CHAIN_MODE")
            .map(|v| v.trim().eq_ignore_ascii_case("live"))
            .unwrap_or(false);
        if !requested {
            return Self::Mock;
        }
        if env_non_empty("ESCROW_LOCK_ADDRESS") {
            Self::Live
        } else {
            tracing::warn!("CHAIN_MODE=live, но не задан ESCROW_LOCK_ADDRESS — работаем в mock");
            Self::Mock
        }
    }

    pub fn is_live(self) -> bool {
        self == Self::Live
    }
}

fn env_non_empty(key: &str) -> bool {
    std::env::var(key)
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false)
}

/// Параметры подключения к сети и адреса контрактов.
#[derive(Debug, Clone)]
pub struct ChainConfig {
    pub rpc_url: String,
    pub chain_id: u64,
    /// Адрес USDC (6 знаков).
    pub usdc: String,
    /// Адрес EscrowLock.
    pub escrow_lock: String,
    /// Приватный ключ наблюдателя — им подписываются release/refund.
    pub observer_key: String,
}

impl ChainConfig {
    /// Собирает настройки из окружения.
    ///
    /// # Возвращает
    /// * `Ok(_)` — все обязательные переменные заданы
    /// * `Err(_)` — чего-то не хватает, с указанием имени переменной
    pub fn from_env() -> Result<Self, ChainError> {
        Ok(Self {
            rpc_url: require_env("MONAD_RPC")?,
            chain_id: require_env("MONAD_CHAIN_ID")?
                .parse()
                .map_err(|_| ChainError::Config("MONAD_CHAIN_ID: ожидается число".into()))?,
            usdc: require_env("USDC_ADDRESS")?,
            escrow_lock: require_env("ESCROW_LOCK_ADDRESS")?,
            observer_key: require_env("OBSERVER_PRIVATE_KEY")?,
        })
    }
}

fn require_env(key: &str) -> Result<String, ChainError> {
    std::env::var(key)
        .map(|v| v.trim().to_string())
        .ok()
        .filter(|v| !v.is_empty())
        .ok_or_else(|| ChainError::Config(format!("не задана переменная {key}")))
}

/// Состояние сделки в контракте — зеркало `enum State` из EscrowLock.sol.
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
    #[error("конфигурация цепи: {0}")]
    Config(String),

    #[error("некорректный адрес {what}: {value}")]
    BadAddress { what: &'static str, value: String },

    #[error("RPC: {0}")]
    Rpc(String),

    /// Транзакция попала в блок, но исполнение завершилось откатом.
    /// На Monad это штатная ситуация: включение в блок ≠ успех.
    #[error("транзакция {tx_hash} отклонена при исполнении")]
    TxReverted { tx_hash: String },

    #[error("сумма отрицательная: {0}")]
    NegativeAmount(i64),

    #[error(
        "сумма {0} не представима в USDC: младше 0.000001 USDC (потеряли бы копейки при переводе)"
    )]
    SubUnitAmount(i64),

    #[error("сумма {0} не помещается в 256 бит")]
    AmountOverflow(i64),
}
