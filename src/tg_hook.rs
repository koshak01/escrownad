//! Приём нажатий кнопок из Telegram.
//!
//! Оператор получает заявку карточкой с кнопками Approve / Decline. Нажатие
//! прилетает сюда вебхуком, мы меняем статус сделки и тут же отвечаем
//! Telegram, чтобы у оператора не крутился спиннер.
//!
//! Адрес вебхука содержит секрет из константы `telegrams.webhook_secret` —
//! чужой POST по этому пути не пройдёт. Дополнительно Telegram присылает
//! заголовок `X-Telegram-Bot-Api-Secret-Token`, его тоже сверяем.

use axum::Json;
use axum::extract::Path;
use axum::http::StatusCode;
use serde::Deserialize;
use serde_json::Value;
use tracing::{info, warn};

use crate::app_context;
use crate::moderation::{CALLBACK_PREFIX, SHORT_HASH_LEN};

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
    pub from: Option<Value>,
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

/// Вебхук Telegram.
///
/// Отвечаем `200` всегда, кроме неверного секрета: Telegram при ошибке
/// повторяет доставку, а нам не нужен шторм повторов из-за нашей же
/// внутренней проблемы.
pub async fn handle(Path(secret): Path<String>, Json(update): Json<TgUpdate>) -> StatusCode {
    let expected = webhook_secret().await;
    if expected.is_empty() || secret != expected {
        warn!("вебхук: неверный секрет в адресе");
        return StatusCode::NOT_FOUND;
    }

    let Some(query) = update.callback_query else {
        return StatusCode::OK;
    };
    let Some(data) = query.data.clone() else {
        return StatusCode::OK;
    };
    let Some(decision) = parse_decision(&data) else {
        warn!(data = %data, "вебхук: неизвестная кнопка");
        return StatusCode::OK;
    };

    let answer = match apply(&decision).await {
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
async fn apply(decision: &Decision) -> Result<String, String> {
    let deal = app_context()
        .db
        .get_deal_by_hash_prefix(decision.short_hash.clone())
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Listing not found".to_string())?;

    let mut deal = deal;
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

    info!(deal = %hash, action, "модерация: решение оператора");
    Ok(if decision.approve {
        format!("Listing {no} is live")
    } else {
        format!("Listing {no} declined")
    })
}

/// Секрет вебхука из константы `telegrams.webhook_secret`.
async fn webhook_secret() -> String {
    let Ok(constants) = app_context().db.get_constants().await else {
        return String::new();
    };
    constants
        .get::<Value>("telegrams")
        .ok()
        .flatten()
        .and_then(|v| {
            v.get("webhook_secret")
                .and_then(|s| s.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default()
}
