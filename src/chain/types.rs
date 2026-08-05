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
    pub fn is_live(self) -> bool {
        self == Self::Live
    }
}

/// Код константы в таблице `constants`, где лежат настройки цепи.
///
/// Всё хранится в базе и правится через `/admin/constants/` — как
/// telegram-креды и прочие секреты проекта. Ни окружения, ни файлов.
pub const CHAIN_CONSTANT: &str = "chain";

/// Параметры подключения к сети и адреса контрактов.
///
/// Значение константы `chain` — объект:
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
    /// `live` — работать с настоящей цепью, иначе mock.
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub rpc: String,
    #[serde(default)]
    pub chain_id: u64,
    /// Адрес USDC (6 знаков).
    #[serde(default)]
    pub usdc: String,
    /// Адрес EscrowLock.
    #[serde(default)]
    pub lock: String,
    /// Приватный ключ наблюдателя — им подписываются release/refund.
    /// Нужен только процессу-наблюдателю.
    #[serde(default)]
    pub observer_key: String,
}

impl ChainConfig {
    /// Достаёт настройки цепи из словаря констант.
    ///
    /// # Параметры
    /// * `constants` — снимок таблицы `constants` (код → значение)
    ///
    /// # Возвращает
    /// * `Some(_)` — константа `chain` есть и разбирается
    /// * `None` — константы нет или её формат не читается
    pub fn from_constants(
        constants: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Option<Self> {
        let raw = constants.get(CHAIN_CONSTANT)?;
        match serde_json::from_value::<Self>(raw.clone()) {
            Ok(c) => Some(c),
            Err(e) => {
                tracing::warn!(error = %e, "константа `chain` не разбирается — работаем в mock");
                None
            }
        }
    }

    /// Режим по содержимому константы.
    ///
    /// Живой режим требует адрес замка: читать состояние и отдавать браузеру
    /// параметры оплаты можно без ключей. Ключ проверяет только тот, кто
    /// подписывает, — см. [`ChainConfig::require_observer_key`].
    pub fn mode(&self) -> ChainMode {
        let wants_live = self.mode.trim().eq_ignore_ascii_case("live");
        if wants_live && !self.lock.trim().is_empty() {
            ChainMode::Live
        } else {
            ChainMode::Mock
        }
    }

    /// Проверяет, что есть всё для подписи транзакций наблюдателем.
    pub fn require_observer_key(&self) -> Result<(), ChainError> {
        if self.observer_key.trim().is_empty() {
            return Err(ChainError::Config(
                "в константе `chain` пуст observer_key — заполни через /admin/constants/".into(),
            ));
        }
        if self.rpc.trim().is_empty() {
            return Err(ChainError::Config("в константе `chain` пуст rpc".into()));
        }
        Ok(())
    }
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
