//! Приём нажатий кнопок из Telegram.
//!
//! Оператор получает заявку карточкой с кнопками Approve / Decline. Нажатие
//! прилетает сюда вебхуком, мы меняем статус сделки и тут же отвечаем
//! Telegram, чтобы у оператора не крутился спиннер.
//!
//! Три рубежа проверки, и каждый закрывает свою дыру:
//!
//! 1. **Секретный заголовок** `X-Telegram-Bot-Api-Secret-Token` — его шлёт
//!    только Telegram, потому что мы задали значение при регистрации
//!    вебхука. В адрес секрет НЕ кладём: путь попадает в `access_log`
//!    nginx открытым текстом, то есть секрет утёк бы в логи.
//! 2. **Сравнение за постоянное время** — обычное `==` выходит из цикла на
//!    первом несовпавшем байте, и по времени ответа секрет подбирается
//!    побайтово.
//! 3. **Список модераторов** — кнопка живёт в групповом чате, и нажать её
//!    может любой участник. У группы публичный юзернейм, то есть вступить
//!    может посторонний. Решение принимает только тот, чей Telegram-ID есть
//!    в константе `telegrams.moderators`.

use axum::Json;
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::Value;
use tracing::{info, warn};

use crate::app_context;
use crate::moderation::{CALLBACK_PREFIX, SHORT_HASH_LEN};

/// Заголовок, которым Telegram доказывает, что запрос от него.
pub const SECRET_HEADER: &str = "x-telegram-bot-api-secret-token";

#[derive(Debug, Deserialize)]
pub struct TgUpdate {
    #[serde(default)]
    pub callback_query: Option<CallbackQuery>,
}

#[derive(Debug, Deserialize)]
pub struct CallbackQuery {
    pub id: String,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub from: Option<TgUser>,
}

#[derive(Debug, Deserialize)]
pub struct TgUser {
    pub id: i64,
    #[serde(default)]
    pub username: Option<String>,
}

/// Разобранное нажатие: какая сделка и какое решение.
struct Decision {
    short_hash: String,
    approve: bool,
}

/// Разбирает `callback_data` вида `dm:<короткий хэш>:<a|d>`.
fn parse_decision(data: &str) -> Option<Decision> {
    let mut parts = data.split(':');
    if parts.next()? != CALLBACK_PREFIX {
        return None;
    }
    let short_hash = parts.next()?.to_string();
    if short_hash.len() != SHORT_HASH_LEN || !short_hash.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let approve = match parts.next()? {
        "a" => true,
        "d" => false,
        _ => return None,
    };
    Some(Decision {
        short_hash,
        approve,
    })
}

/// Сравнение за постоянное время: время работы не зависит от того, где
/// строки разошлись. Обычное `==` даёт подобрать секрет по таймингам.
fn secret_eq(given: &str, expected: &str) -> bool {
    let (a, b) = (given.as_bytes(), expected.as_bytes());
    if a.len() != b.len() || b.is_empty() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Вебхук Telegram.
///
/// Отвечаем `200` на всё, что прошло проверку подлинности: Telegram при
/// ошибке повторяет доставку, а нам не нужен шторм повторов из-за нашей же
/// внутренней проблемы. Неподлинный запрос получает `404` — снаружи это
/// неотличимо от «такого адреса нет».
pub async fn handle(headers: HeaderMap, Json(update): Json<TgUpdate>) -> StatusCode {
    let settings = telegram_settings().await;

    let given = headers
        .get(SECRET_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !secret_eq(given, &settings.webhook_secret) {
        warn!("вебхук: запрос без верного секретного заголовка");
        return StatusCode::NOT_FOUND;
    }

    let Some(query) = update.callback_query else {
        return StatusCode::OK;
    };

    // Кто нажал. Кнопка висит в групповом чате, нажать может любой участник —
    // поэтому решение принимает только модератор из списка.
    let actor = query.from.as_ref().map(|u| u.id).unwrap_or_default();
    if !settings.moderators.contains(&actor) {
        warn!(
            actor,
            username = query.from.as_ref().and_then(|u| u.username.as_deref()),
            "вебхук: решение от постороннего — отклонено"
        );
        let _ = app_context()
            .notifier
            .answer_callback(
                query.id,
                Some("You are not a moderator of this board".into()),
                true,
            )
            .await;
        return StatusCode::OK;
    }

    let Some(data) = query.data.clone() else {
        return StatusCode::OK;
    };
    let Some(decision) = parse_decision(&data) else {
        warn!(data = %data, "вебхук: неизвестная кнопка");
        return StatusCode::OK;
    };

    let answer = match apply(&decision, actor).await {
        Ok(text) => text,
        Err(e) => {
            warn!(error = %e, "вебхук: решение не применилось");
            e
        }
    };

    // Спиннер на кнопке живёт 15 секунд — отвечаем сразу.
    if let Err(e) = app_context()
        .notifier
        .answer_callback(query.id, Some(answer), false)
        .await
    {
        warn!(error = %e, "вебхук: не ответил на нажатие");
    }
    StatusCode::OK
}

/// Меняет статус сделки по решению оператора.
async fn apply(decision: &Decision, actor: i64) -> Result<String, String> {
    let mut deal = app_context()
        .db
        .get_deal_by_hash_prefix(decision.short_hash.clone())
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Listing not found".to_string())?;

    let action = if decision.approve {
        "approve"
    } else {
        "decline"
    };
    deal.apply_action(action, None, None)?;
    let no = deal.public_no();
    let hash = deal.del_hash.clone();

    app_context()
        .db
        .save_deal(deal)
        .await
        .map_err(|e| e.to_string())?;

    info!(deal = %hash, action, moderator = actor, "модерация: решение оператора");
    Ok(if decision.approve {
        format!("Listing {no} is live")
    } else {
        format!("Listing {no} declined")
    })
}

/// Настройки бота из константы `telegrams`.
#[derive(Debug, Default)]
struct TelegramSettings {
    webhook_secret: String,
    /// Telegram-ID тех, кому позволено решать судьбу заявок.
    moderators: Vec<i64>,
}

async fn telegram_settings() -> TelegramSettings {
    let Ok(constants) = app_context().db.get_constants().await else {
        return TelegramSettings::default();
    };
    let Ok(Some(value)) = constants.get::<Value>("telegrams") else {
        return TelegramSettings::default();
    };
    TelegramSettings {
        webhook_secret: value
            .get("webhook_secret")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        moderators: value
            .get("moderators")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_i64()).collect())
            .unwrap_or_default(),
    }
}
