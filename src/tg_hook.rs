//! Receiving button presses from Telegram.
//!
//! The operator gets a listing as a card with Approve / Decline buttons. A
//! press arrives here as a webhook; we change the deal's status and answer
//! Telegram immediately, so the operator is not left watching a spinner.
//!
//! Three lines of defence, each closing a different hole:
//!
//! 1. **A secret header**, `X-Telegram-Bot-Api-Secret-Token` — only Telegram
//!    sends it, because we set the value when registering the webhook. The
//!    secret does NOT go in the URL: the path lands in nginx's `access_log` in
//!    clear text, so the secret would leak into the logs.
//! 2. **A constant-time comparison** — an ordinary `==` bails out at the first
//!    mismatched byte, and the response timing lets the secret be guessed one
//!    byte at a time.
//! 3. **Group membership** — the button lives in a closed operators' group.
//!    Presence in it *is* the right to decide: add someone and they are a
//!    moderator, remove them and they are not. There is deliberately no
//!    separate list, which would be a second source of truth bound to drift.
//!    Two conditions are checked: the press came from our review channel, and
//!    the person pressing belongs to it.

use axum::Json;
use axum::http::{HeaderMap, StatusCode};
use serde::Deserialize;
use serde_json::Value;
use tracing::{info, warn};

use crate::app_context;
use crate::moderation::{CALLBACK_PREFIX, SHORT_HASH_LEN};

/// The header by which Telegram proves a request is its own.
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

/// A parsed press: which deal, and which decision.
struct Decision {
    short_hash: String,
    approve: bool,
}

/// Parses `callback_data` of the form `dm:<short hash>:<a|d>`.
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

/// Constant-time comparison: the running time does not depend on where the
/// strings diverged. An ordinary `==` lets the secret be recovered by timing.
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

/// The Telegram webhook.
///
/// Anything that passes authentication gets a `200`: Telegram retries on
/// errors, and we do not want a storm of retries caused by a problem of our
/// own. An unauthenticated request gets a `404` — from outside that is
/// indistinguishable from "no such address".
pub async fn handle(headers: HeaderMap, Json(update): Json<TgUpdate>) -> StatusCode {
    let settings = telegram_settings().await;

    let given = headers
        .get(SECRET_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default();
    if !secret_eq(given, &settings.webhook_secret) {
        warn!("webhook: request without a valid secret header");
        return StatusCode::NOT_FOUND;
    }

    let Some(query) = update.callback_query else {
        return StatusCode::OK;
    };

    // Who pressed, and from where. The right to decide comes from being in the
    // operators' group: the press must come from it, and the presser must belong.
    let actor = query.from.as_ref().map(|u| u.id).unwrap_or_default();
    let chat = query
        .message
        .as_ref()
        .and_then(|m| m.chat.as_ref())
        .map(|c| c.id)
        .unwrap_or_default();

    if settings.moderation_chat == 0 || chat != settings.moderation_chat {
        warn!(chat, "webhook: press did not come from the operators' group");
        return StatusCode::OK;
    }
    if !is_member(&settings.token, chat, actor).await {
        warn!(
            actor,
            username = query.from.as_ref().and_then(|u| u.username.as_deref()),
            "webhook: presser is not in the group — rejected"
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
        warn!(data = %data, "webhook: unknown button");
        return StatusCode::OK;
    };

    let answer = match apply(&decision, actor).await {
        Ok(text) => text,
        Err(e) => {
            warn!(error = %e, "webhook: the decision did not apply");
            e
        }
    };

    // The button's spinner lives 15 seconds — answer at once.
    if let Err(e) = app_context()
        .notifier
        .answer_callback(query.id, Some(answer), false)
        .await
    {
        warn!(error = %e, "webhook: failed to answer the press");
    }
    StatusCode::OK
}

/// Changes the deal's status according to the operator's decision.
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

    // Approval is the moment the lot becomes a verified asset: the token is
    // issued with its transfer rules attached, before anyone can see the lot.
    // If issuance fails the listing still goes up — an asset that did not mint
    // is worth a retry, not a lot held hostage.
    if decision.approve && deal.asset_token.is_none() && deal.asset_request.is_none() {
        deal.asset_request = crate::moderation::issue_asset(&deal).await;
    }

    app_context()
        .db
        .save_deal(deal)
        .await
        .map_err(|e| e.to_string())?;

    info!(deal = %hash, action, moderator = actor, "review: operator decision");
    Ok(if decision.approve {
        format!("Listing {no} is live")
    } else {
        format!("Listing {no} declined")
    })
}

/// Is this person a member of the operators' group?
///
/// We ask Telegram directly: the membership list is its business, not ours.
/// Leave the group and the right to decide is gone that same second, with no
/// database edit.
///
/// # Returns
/// * `true` — a member, an administrator, or the creator
/// * `false` — left, banned, never joined, or Telegram is unreachable
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
            warn!(error = %e, "webhook: could not build the http client");
            return false;
        }
    };
    // parameters in the URL itself: reqwest's `query` feature is off in our build
    let url = format!("{url}?chat_id={chat_id}&user_id={user_id}");
    let resp = client.get(&url).send().await;
    let body: Value = match resp {
        Ok(r) => match r.json().await {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "webhook: getChatMember did not parse");
                return false;
            }
        },
        Err(e) => {
            warn!(error = %e, "webhook: getChatMember unreachable");
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

/// Bot settings from the `telegrams` constant.
#[derive(Debug, Default)]
struct TelegramSettings {
    token: String,
    webhook_secret: String,
    /// The operators' group chat — decisions are accepted only from there.
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
