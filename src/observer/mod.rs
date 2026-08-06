//! RIPE PA+PI oracle matcher for deals.

use serde::Deserialize;

pub const RIPE_PI: &str =
    "https://www-static.ripe.net/dynamic/table-of-transfers/ipv4/transfers-assignments.json";
pub const RIPE_PA: &str =
    "https://www-static.ripe.net/dynamic/table-of-transfers/ipv4/transfers-allocations.json";

#[derive(Debug, Clone, Deserialize)]
pub struct RipeTransfer {
    pub original_block: String,
    pub transferred_blocks: String,
    pub from: String,
    pub to: String,
    pub date: String,
    #[serde(rename = "transferType")]
    pub transfer_type: String,
    #[serde(rename = "transferStatus")]
    pub transfer_status: String,
}

#[derive(Debug, Deserialize)]
struct Payload {
    transfers: Vec<RipeTransfer>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceKind {
    Pa,
    Pi,
}

impl ResourceKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "PA" => Some(Self::Pa),
            "PI" => Some(Self::Pi),
            _ => None,
        }
    }

    pub fn url(self) -> &'static str {
        match self {
            Self::Pa => RIPE_PA,
            Self::Pi => RIPE_PI,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pa => "PA",
            Self::Pi => "PI",
        }
    }
}

pub async fn fetch_transfers(kind: ResourceKind) -> Result<Vec<RipeTransfer>, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .get(kind.url())
        .send()
        .await
        .map_err(|e| format!("HTTP {}: {e}", kind.as_str()))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {} status {}", kind.as_str(), resp.status()));
    }
    let payload: Payload = resp
        .json()
        .await
        .map_err(|e| format!("JSON {}: {e}", kind.as_str()))?;
    Ok(payload.transfers)
}

/// Дата строки реестра. В таблицах RIPE формат `05/08/2026`.
pub fn transfer_date(t: &RipeTransfer) -> Option<chrono::NaiveDate> {
    chrono::NaiveDate::parse_from_str(t.date.trim(), "%d/%m/%Y").ok()
}

/// Сеть из строки реестра совпадает с искомой.
///
/// Сравнение по границам, а не подстрокой: иначе `10.1.1.0` совпал бы
/// с `110.1.1.0`, и деньги ушли бы по чужому переходу.
fn block_matches(prefix: &str, t: &RipeTransfer) -> bool {
    let needle = prefix.trim();
    if needle.is_empty() {
        return false;
    }
    let listed = |field: &str| {
        field
            .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .any(|block| block.eq_ignore_ascii_case(needle))
    };
    listed(&t.original_block) || listed(&t.transferred_blocks)
}

/// Строка реестра подтверждает переход по нашей сделке.
///
/// Оракул садится на конкретную сеть и ждёт, когда по ней появится
/// **новая** запись:
///
/// 1. сеть совпадает точно (по границам, не подстрокой);
/// 2. запись датирована **не раньше** появления сделки — иначе засчитали бы
///    старый чужой переход, случившийся до того, как лот вообще возник;
/// 3. организации, если заданы, служат дополнительной проверкой — но
///    продавец их не обязан вписывать: кто кому передал, мы узнаём из самой
///    найденной строки.
///
/// # Параметры
/// * `prefix` — сеть сделки
/// * `since` — день, раньше которого переход нам не подходит
/// * `from_org` / `to_org` — необязательное уточнение сторон
/// * `t` — строка таблицы трансферов
///
/// # Возвращает
/// * `true` — это наш переход
pub fn match_deal(
    prefix: &str,
    since: Option<chrono::NaiveDate>,
    from_org: Option<&str>,
    to_org: Option<&str>,
    t: &RipeTransfer,
) -> bool {
    if !block_matches(prefix, t) {
        return false;
    }
    if let Some(since) = since {
        match transfer_date(t) {
            Some(d) if d >= since => {}
            // дата не разобралась или переход старше сделки — не наш случай
            _ => return false,
        }
    }
    if let Some(f) = from_org.filter(|s| !s.trim().is_empty()) {
        if !t.from.to_lowercase().contains(&f.trim().to_lowercase()) {
            return false;
        }
    }
    if let Some(to) = to_org.filter(|s| !s.trim().is_empty()) {
        if !t.to.to_lowercase().contains(&to.trim().to_lowercase()) {
            return false;
        }
    }
    true
}

pub fn match_key(kind: ResourceKind, t: &RipeTransfer) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}",
        kind.as_str(),
        t.date,
        t.original_block,
        t.transferred_blocks,
        t.from,
        t.to
    )
}
