//! Опрос сети в реестре через RDAP — второй источник факта.
//!
//! Таблица трансферов говорит «переход состоялся такого-то числа», но
//! появляется в ней запись не мгновенно. RDAP отвечает по конкретной сети
//! здесь и сейчас: кто держатель, какой тип ресурса, какая страна и когда
//! запись меняли в последний раз.
//!
//! Два применения:
//! 1. **при публикации** — подтянуть владельца, тип и страну из реестра,
//!    вместо того чтобы верить тому, что вписал продавец;
//! 2. **в оракуле** — увидеть смену держателя быстрее, чем она доедет до
//!    таблицы трансферов.

use serde::Deserialize;

/// Адрес RDAP-сервиса RIPE.
pub const RIPE_RDAP: &str = "https://rdap.db.ripe.net/ip";

/// Что реестр знает о сети прямо сейчас.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRecord {
    /// Диапазон, как его отдал реестр: `194.246.124.0 - 194.246.125.255`.
    pub range: String,
    /// Тип ресурса реестра: `ASSIGNED PI`, `ALLOCATED PA` и т.п.
    pub resource_type: String,
    /// Наш код вида ресурса: `PI` | `PA` | пусто, если не распознали.
    pub kind: String,
    /// Код страны.
    pub country: String,
    /// Держатель — организация-регистрант.
    pub holder: String,
    /// Когда запись меняли последний раз.
    pub last_changed: Option<chrono::NaiveDate>,
}

#[derive(Debug, Deserialize)]
struct RdapResponse {
    #[serde(default)]
    handle: String,
    #[serde(default, rename = "type")]
    resource_type: String,
    #[serde(default)]
    country: String,
    #[serde(default)]
    entities: Vec<RdapEntity>,
    #[serde(default)]
    events: Vec<RdapEvent>,
}

#[derive(Debug, Deserialize)]
struct RdapEntity {
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default, rename = "vcardArray")]
    vcard: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct RdapEvent {
    #[serde(default, rename = "eventAction")]
    action: String,
    #[serde(default, rename = "eventDate")]
    date: String,
}

/// Спрашивает реестр о сети.
///
/// # Параметры
/// * `prefix` — сеть, например `194.246.124.0/23`
///
/// # Возвращает
/// * `Ok(Some(_))` — реестр знает такую сеть
/// * `Ok(None)` — сети нет (404), это не ошибка связи
/// * `Err(_)` — сеть недоступна или ответ не разобрался
pub async fn lookup(prefix: &str) -> Result<Option<NetworkRecord>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("{RIPE_RDAP}/{}", prefix.trim());
    let resp = client
        .get(&url)
        .header("Accept", "application/rdap+json")
        .send()
        .await
        .map_err(|e| format!("RDAP: {e}"))?;

    if resp.status().as_u16() == 404 {
        return Ok(None);
    }
    if !resp.status().is_success() {
        return Err(format!("RDAP status {}", resp.status()));
    }

    let body: RdapResponse = resp.json().await.map_err(|e| format!("RDAP JSON: {e}"))?;
    Ok(Some(NetworkRecord {
        range: body.handle,
        kind: parse_kind(&body.resource_type),
        resource_type: body.resource_type,
        country: body.country,
        holder: holder_name(&body.entities),
        last_changed: last_changed(&body.events),
    }))
}

/// Приводит тип реестра к нашему коду: `ASSIGNED PI` → `PI`.
fn parse_kind(resource_type: &str) -> String {
    let upper = resource_type.to_uppercase();
    if upper.contains("PI") {
        "PI".into()
    } else if upper.contains("PA") {
        "PA".into()
    } else {
        String::new()
    }
}

/// Имя организации-регистранта из vCard.
///
/// В RDAP держатель лежит в сущности с ролью `registrant`, а имя — в поле
/// `fn` её vCard. Организаций может быть несколько (мейнтейнеры реестра
/// тоже регистранты), поэтому берём первую с человеческим названием.
fn holder_name(entities: &[RdapEntity]) -> String {
    entities
        .iter()
        .filter(|e| e.roles.iter().any(|r| r == "registrant"))
        .filter_map(|e| vcard_fn(e.vcard.as_ref()?))
        .find(|name| !name.starts_with("MNT-") && !name.contains("RIPE-NCC"))
        .unwrap_or_default()
}

/// Достаёт `fn` (полное имя) из vcardArray.
fn vcard_fn(vcard: &serde_json::Value) -> Option<String> {
    let entries = vcard.as_array()?.get(1)?.as_array()?;
    for entry in entries {
        let parts = entry.as_array()?;
        if parts.first()?.as_str()? == "fn" {
            return parts.get(3)?.as_str().map(str::to_string);
        }
    }
    None
}

/// Дата последнего изменения записи.
fn last_changed(events: &[RdapEvent]) -> Option<chrono::NaiveDate> {
    events
        .iter()
        .find(|e| e.action == "last changed")
        .or_else(|| events.iter().find(|e| e.action == "registration"))
        .and_then(|e| {
            chrono::DateTime::parse_from_rfc3339(e.date.trim())
                .ok()
                .map(|dt| dt.date_naive())
        })
}

impl NetworkRecord {
    /// Держатель сменился после указанной даты — признак состоявшегося
    /// перехода, который ещё не доехал до таблицы трансферов.
    ///
    /// # Параметры
    /// * `since` — день появления сделки
    /// * `previous_holder` — держатель на момент публикации лота
    pub fn changed_hands_since(
        &self,
        since: chrono::NaiveDate,
        previous_holder: Option<&str>,
    ) -> bool {
        let Some(changed) = self.last_changed else {
            return false;
        };
        if changed < since {
            return false;
        }
        match previous_holder.map(str::trim).filter(|s| !s.is_empty()) {
            // держатель известен и он изменился
            Some(prev) => !self.holder.eq_ignore_ascii_case(prev),
            // сравнивать не с чем — одной даты мало для вывода
            None => false,
        }
    }
}
