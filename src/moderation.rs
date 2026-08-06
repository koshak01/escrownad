//! Модерация заявок: карточка в Telegram с кнопками Approve / Decline.
//!
//! Лот не попадает на доску сам по себе. После отправки он ждёт решения
//! оператора, а оператор получает в Telegram всё, что нужно для решения:
//! параметры лота, что говорит о сети реестр, сколько денег на кошельке
//! заявителя и впервые ли этот кошелёк у нас.
//!
//! Тексты — только английские: продукт англоязычный, оператор читает то же,
//! что и пользователи.

use forge_fixed_n::FixedN;
use serde::Serialize;

use crate::app_context;
use crate::models::Deal;

/// Префикс callback-кнопок: `dm:<короткий хэш>:<a|d>`.
///
/// Telegram ограничивает `callback_data` 64 байтами, а полный хэш сделки —
/// сам по себе 64 символа. Поэтому в кнопку уходит первые 16 символов: этого
/// хватает, чтобы найти сделку однозначно.
pub const CALLBACK_PREFIX: &str = "dm";

/// Сколько символов хэша уходит в кнопку.
pub const SHORT_HASH_LEN: usize = 16;

/// Короткий вид хэша для callback-кнопки.
pub fn short_hash(del_hash: &str) -> String {
    del_hash.chars().take(SHORT_HASH_LEN).collect()
}

/// Код шаблона в таблице `templates`.
pub const TEMPLATE_CODE: &str = "deal_moderation";

/// Всё, что оператор видит в сообщении.
#[derive(Debug, Serialize)]
pub struct ModerationCard {
    pub deal_hash: String,
    /// Короткий хэш — уходит в callback_data кнопок.
    pub deal_short: String,
    pub deal_no: String,
    pub side: String,
    pub asset_type: String,
    pub resource_kind: String,
    pub rir: String,
    pub prefix: String,
    pub addresses: String,
    pub geo: String,
    pub from_org: String,
    pub price: String,
    pub description: String,
    pub terms: String,
    pub wallet: String,
    pub wallet_short: String,
    /// Баланс USDC заявителя, как строка с двумя знаками.
    pub wallet_usdc: String,
    /// Родная монета сети — на неё платится газ.
    pub wallet_native: String,
    /// Кошелёк раньше у нас не встречался.
    pub wallet_is_new: bool,
    /// Сколько сделок уже было у этого кошелька.
    pub wallet_deals: i64,
    /// Что реестр отвечает по этой сети прямо сейчас.
    pub registry_holder: String,
    pub registry_type: String,
    pub registry_country: String,
    /// Держатель в реестре совпал с тем, что указал заявитель.
    pub registry_matches: bool,
    pub url: String,
}

fn short_wallet(addr: &str) -> String {
    if addr.len() > 14 {
        format!("{}…{}", &addr[..8], &addr[addr.len() - 4..])
    } else {
        addr.to_string()
    }
}

fn usdc_str(raw: alloy::primitives::U256) -> String {
    let units: u128 = raw.to::<u128>();
    format!("{}.{:02}", units / 1_000_000, (units % 1_000_000) / 10_000)
}

fn native_str(raw: alloy::primitives::U256) -> String {
    let units: u128 = raw.to::<u128>();
    let whole = units / 1_000_000_000_000_000_000;
    let frac = (units % 1_000_000_000_000_000_000) / 10_000_000_000_000_000;
    format!("{whole}.{frac:02}")
}

fn money(amount: FixedN<8>) -> String {
    let raw = amount.raw();
    format!("{}.{:02}", raw / 100_000_000, (raw % 100_000_000) / 1_000_000)
}

/// Собирает карточку заявки для оператора.
///
/// Сеть спрашивается у реестра здесь же — оператор видит не то, что вписал
/// заявитель, а то, что отвечает RIPE. Балансы читаются из цепи. Если сеть
/// или цепь недоступны, поля остаются пустыми: решение всё равно можно
/// принять, просто с меньшим знанием.
pub async fn build_card(deal: &Deal) -> ModerationCard {
    let wallet = deal.seller_wallet.clone().unwrap_or_default();

    // сколько сделок было у этого кошелька раньше — «новый» значит первый лот
    let wallet_deals = match deal.seller_usr_id {
        Some(uid) => app_context()
            .db
            .list_deals_for_user(uid)
            .await
            .map(|v| v.len() as i64)
            .unwrap_or(0),
        None => 0,
    };

    // балансы из цепи
    let (mut usdc, mut native) = (String::new(), String::new());
    if !wallet.is_empty() {
        if let Some(config) = chain_config().await {
            if let Some(reader) = crate::chain::core::ChainReader::new(&config) {
                match reader.wallet_balances(&wallet).await {
                    Ok((u, n)) => {
                        usdc = usdc_str(u);
                        native = native_str(n);
                    }
                    Err(e) => tracing::warn!(error = %e, "не прочитал баланс кошелька"),
                }
            }
        }
    }

    // что реестр говорит о сети
    let (mut holder, mut rtype, mut country) = (String::new(), String::new(), String::new());
    if !deal.prefix.trim().is_empty() {
        match crate::observer::rdap::lookup(&deal.prefix).await {
            Ok(Some(record)) => {
                holder = record.holder;
                rtype = record.resource_type;
                country = record.country;
            }
            Ok(None) => holder = "NOT FOUND IN REGISTRY".into(),
            Err(e) => tracing::warn!(error = %e, "реестр недоступен при модерации"),
        }
    }
    let claimed = deal.from_org.as_deref().unwrap_or("").trim().to_lowercase();
    let registry_matches =
        !claimed.is_empty() && !holder.is_empty() && holder.to_lowercase().contains(&claimed);

    ModerationCard {
        deal_no: deal.public_no(),
        side: deal.listing_side.clone(),
        asset_type: deal.asset_type.to_uppercase(),
        resource_kind: deal.resource_kind.clone(),
        rir: deal.rir.clone(),
        addresses: crate::models::deal::address_count(&deal.prefix)
            .map(|n| n.to_string())
            .unwrap_or_else(|| "—".into()),
        prefix: deal.prefix.clone(),
        geo: deal.geo.clone().unwrap_or_default(),
        from_org: deal.from_org.clone().unwrap_or_default(),
        price: money(deal.del_amount),
        description: crate::sanitize::plain_text(deal.del_title.as_deref().unwrap_or("")),
        terms: crate::sanitize::plain_text(deal.del_note.as_deref().unwrap_or("")),
        wallet_short: short_wallet(&wallet),
        wallet_usdc: usdc,
        wallet_native: native,
        wallet_is_new: wallet_deals <= 1,
        wallet_deals,
        registry_holder: holder,
        registry_type: rtype,
        registry_country: country,
        registry_matches,
        url: format!("https://escrownad.com/deals/{}/", deal.del_hash),
        wallet,
        deal_short: short_hash(&deal.del_hash),
        deal_hash: deal.del_hash.clone(),
    }
}

/// Настройки цепи из констант — общий помощник.
async fn chain_config() -> Option<crate::chain::types::ChainConfig> {
    let constants = app_context().db.get_constants().await.ok()?;
    let mut map = std::collections::HashMap::new();
    for key in constants.keys() {
        if let Ok(Some(v)) = constants.get::<serde_json::Value>(key) {
            map.insert(key.clone(), v);
        }
    }
    crate::chain::types::ChainConfig::from_constants(&map)
}

/// Отправляет заявку оператору.
///
/// Ошибку отправки не поднимаем наверх: заявка уже сохранена и ждёт
/// решения, а недоступный Telegram не повод показывать пользователю сбой.
pub async fn notify(deal: &Deal) {
    let card = build_card(deal).await;
    if let Err(e) = app_context().notifier.send(TEMPLATE_CODE, &card).await {
        tracing::error!(deal = %deal.del_hash, error = %e, "не отправил заявку на модерацию");
    }
}
