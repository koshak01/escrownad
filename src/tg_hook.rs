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
//! 3. **Членство в группе** — кнопка живёт в закрытой группе операторов.
//!    Право решать даёт само присутствие в ней: добавили человека — он
//!    модератор, убрали — перестал. Отдельного списка нет намеренно, иначе
//!    было бы два источника правды, которые рано или поздно разойдутся.
//!    Проверяем два условия: нажатие пришло именно из нашего канала
//!    модерации и нажавший в нём состоит.

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
    #[serde(default)]
    pub message: Option<TgMessage>,
}

#[derive(Debug, Deserialize)]
pub struct TgMessage {
    #[serde(default)]
    pub chat: Option<TgChat>,
}

#[derive(Debug, Deserialize)]
pub struct TgChat {
    pub id: i64,
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

    // Кто и откуда нажал. Право решать даёт присутствие в группе операторов:
    // нажатие должно прийти из неё, и нажавший должен в ней состоять.
    let actor = query.from.as_ref().map(|u| u.id).unwrap_or_default();
    let chat = query
        .message
        .as_ref()
        .and_then(|m| m.chat.as_ref())
        .map(|c| c.id)
        .unwrap_or_default();

    if settings.moderation_chat == 0 || chat != settings.moderation_chat {
        warn!(chat, "вебхук: нажатие пришло не из группы операторов");
        return StatusCode::OK;
    }
    if !is_member(&settings.token, chat, actor).await {
        warn!(
            actor,
            username = query.from.as_ref().and_then(|u| u.username.as_deref()),
            "вебхук: нажавший не состоит в группе — отклонено"
        );
        let _ = app_context()
            .notifier
            .answer_callback(
                query.id,
                Some("Only members of the operators group can decide".into()),
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

/// Состоит ли человек в группе операторов.
///
/// Спрашиваем у Telegram напрямую: список участников — его дело, а не наше.
/// Вышел из группы — право решать пропало в ту же секунду, без правок в базе.
///
/// # Возвращает
/// * `true` — участник, администратор или создатель
/// * `false` — вышел, изгнан, никогда не состоял, либо Telegram недоступен
async fn is_member(token: &str, chat_id: i64, user_id: i64) -> bool {
    if token.is_empty() || user_id == 0 {
        return false;
    }
    let url = format!("https://api.telegram.org/bot{token}/getChatMember");
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            warn!(error = %e, "вебхук: не собрал http-клиент");
            return false;
        }
    };
    // параметры в самом адресе: фича `query` у reqwest в нашей сборке выключена
    let url = format!("{url}?chat_id={chat_id}&user_id={user_id}");
    let resp = client.get(&url).send().await;
    let body: Value = match resp {
        Ok(r) => match r.json().await {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "вебхук: getChatMember не разобрался");
                return false;
            }
        },
        Err(e) => {
            warn!(error = %e, "вебхук: getChatMember недоступен");
            return false;
        }
    };
    let status = body
        .get("result")
        .and_then(|r| r.get("status"))
        .and_then(|s| s.as_str())
        .unwrap_or_default();
    matches!(status, "creator" | "administrator" | "member")
}

/// Настройки бота из константы `telegrams`.
#[derive(Debug, Default)]
struct TelegramSettings {
    token: String,
    webhook_secret: String,
    /// Чат группы операторов — только оттуда принимаем решения.
    moderation_chat: i64,
}

async fn telegram_settings() -> TelegramSettings {
    let Ok(constants) = app_context().db.get_constants().await else {
        return TelegramSettings::default();
    };
    let Ok(Some(value)) = constants.get::<Value>("telegrams") else {
        return TelegramSettings::default();
    };
    TelegramSettings {
        token: value
            .get("token")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        webhook_secret: value
            .get("webhook_secret")
            .and_then(|s| s.as_str())
            .unwrap_or_default()
            .to_string(),
        moderation_chat: value
            .get("moderation_chat")
            .and_then(|v| v.as_i64())
            .unwrap_or_default(),
    }
}
