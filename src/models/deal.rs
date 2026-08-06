//! Deal — proof-escrow. Asset v1: IPv4 (RIPE). Flow: list → want → accept → prepare → fund → proof.

use forge_core::Timestamp;
use forge_core::hash::sha256_hex;
use forge_db::sqlx::{FromRow, PgPool};
use forge_db::{DbModel, ListFilter};
use forge_fixed_n::FixedN;
use serde::{Deserialize, Serialize};

/// Allowed lifecycle statuses (product flow).
pub mod status {
    pub const DRAFT: &str = "draft";
    pub const VERIFIED: &str = "verified";
    pub const LISTED: &str = "listed";
    pub const REQUESTED: &str = "requested";
    pub const ACCEPTED: &str = "accepted";
    pub const PREPARING: &str = "preparing";
    pub const PREPARED: &str = "prepared";
    pub const FUNDED: &str = "funded";
    pub const AWAITING_PROOF: &str = "awaiting_proof";
    pub const RELEASED: &str = "released";
    pub const REFUNDED: &str = "refunded";
    pub const DISPUTE: &str = "dispute";
    pub const CANCELLED: &str = "cancelled";
}

#[derive(Debug, Clone, Default, FromRow, DbModel, Serialize, Deserialize)]
#[db(table = "deals", pk = "del_id")]
pub struct Deal {
    #[db(skip_insert)]
    pub del_id: i64,

    #[db(unique, hash)]
    pub del_hash: String,

    pub del_title: Option<String>,
    pub del_note: Option<String>,

    /// Offer type: ip | domain | property | work | other (drives type-specific fields later).
    pub asset_type: String,
    /// PA | PI (when asset_type = ip)
    pub resource_kind: String,
    pub prefix: String,

    /// `offer` = sell listing · `request` = buy demand (exchange).
    pub listing_side: String,
    /// Организация-владелец в реестре — по ней оракул ищет строку перехода.
    pub from_org: Option<String>,
    /// Организация-получатель. На публикации обычно пуста: покупатель
    /// известен только после оплаты.
    pub to_org: Option<String>,
    /// Региональный реестр: RIPE | ARIN | APNIC | LACNIC | AFRINIC.
    /// Автопроверка фактов сейчас есть только для RIPE.
    pub rir: String,
    /// Гео блока — витрина, в поиске факта не участвует.
    pub geo: Option<String>,

    pub seller_wallet: Option<String>,
    pub buyer_wallet: Option<String>,
    pub seller_usr_id: Option<i64>,
    pub buyer_usr_id: Option<i64>,
    pub broker_usr_id: Option<i64>,

    pub del_amount: FixedN<8>,
    pub chain_id: String,
    pub lock_tx: Option<String>,
    pub release_tx: Option<String>,

    pub del_status: String,
    pub deadline_ts: Option<Timestamp>,
    pub ripe_match_key: Option<String>,
    pub checklist_json: Option<String>,

    pub soft_verified: bool,
    pub contact_email: Option<String>,

    pub del_is_enable: bool,

    /// Created at (DB default). Used for “listed for” age on the market board.
    #[db(skip_insert)]
    pub del_dat: Timestamp,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ListFilter)]
#[list_filter(model = Deal)]
pub struct DealListFilter {
    #[filter(text, col = "prefix", label = "Prefix")]
    pub prefix: Option<String>,
    #[filter(text, col = "del_status", label = "Status")]
    pub status: Option<String>,
    #[filter(text, col = "resource_kind", label = "Kind")]
    pub kind: Option<String>,
    #[filter(text, col = "asset_type", label = "Asset")]
    pub asset_type: Option<String>,
}

/// Срок действия листинга по умолчанию — дней от публикации.
pub const DEFAULT_LISTING_DAYS: i64 = 31;

/// Региональные интернет-реестры.
///
/// Автоматическая проверка факта перехода сейчас работает только для
/// **RIPE** — только у него публикуются машиночитаемые таблицы трансферов
/// в нужном виде. Остальные реестры принимаются в листинг, но выпуск денег
/// по ним потребует ручного подтверждения.
pub const RIRS: &[&str] = &["RIPE", "ARIN", "APNIC", "LACNIC", "AFRINIC"];

/// Реестр, для которого работает автопроверка фактов.
pub const RIR_WITH_ORACLE: &str = "RIPE";

/// Сколько адресов в блоке по его записи CIDR.
///
/// Покупатель смотрит на количество адресов, а не на длину префикса:
/// `/22` ему ни о чём не говорит, `1024 addresses` — говорит.
///
/// # Параметры
/// * `prefix` — сеть в виде `194.246.124.0/23` либо без маски
///
/// # Возвращает
/// * `Some(число)` — адресов в блоке · `None` — маска не распознана
pub fn address_count(prefix: &str) -> Option<u64> {
    let bits: u32 = prefix.rsplit_once('/')?.1.trim().parse().ok()?;
    if bits > 32 {
        return None;
    }
    Some(1u64 << (32 - bits))
}

/// Публичный номер лота `ddd-ddd` — производная от `del_hash`.
///
/// Наружу (URL, таблица, карточка) уходят только хэш и этот номер;
/// `del_id` не показывается нигде.
///
/// # Параметры
/// * `del_hash` — hex-хэш сделки
///
/// # Возвращает
/// * `String` — номер вида `472-118`
pub fn public_no(del_hash: &str) -> String {
    let head = del_hash.get(..6).unwrap_or("000000");
    let n = u32::from_str_radix(head, 16).unwrap_or(0) % 1_000_000;
    format!("{:03}-{:03}", n / 1000, n % 1000)
}

impl Deal {
    /// Публичный номер лота — см. [`public_no`].
    pub fn public_no(&self) -> String {
        public_no(&self.del_hash)
    }

    /// Срок действия истёк (для листингов на доске).
    pub fn is_expired(&self) -> bool {
        self.deadline_ts
            .map(|d| d.raw() <= Timestamp::now().raw())
            .unwrap_or(false)
    }

    /// Проставляет срок действия по умолчанию, если создатель его не задал.
    ///
    /// Вызывается при выходе лота на доску (`list` / `publish`), чтобы на
    /// рынке не висели бессрочные предложения.
    fn ensure_deadline(&mut self) {
        if self.deadline_ts.is_none() {
            let secs = DEFAULT_LISTING_DAYS * 86_400;
            self.deadline_ts = Some(Timestamp(Timestamp::now().raw() + secs));
        }
    }

    /// Присваивает постоянный хэш сделки — идемпотентно.
    ///
    /// Хэш это публичный идентификатор: по нему строится адрес
    /// `/deals/<hash>/` и номер лота. Считается ОДИН раз при создании и
    /// дальше не меняется — иначе ссылка протухала бы при каждой смене
    /// сторон (появился `buyer_wallet` — новый хэш — 404 по старому адресу).
    ///
    /// Вызывается в ws-хендлере до отправки в database (чтобы знать, куда
    /// редиректить) и в [`Deal::save`] как страховка для остальных путей.
    pub fn assign_hash(&mut self) {
        if !self.del_hash.is_empty() {
            return;
        }
        let key = format!(
            "{}|{}|{}|{}|{}|{}",
            self.listing_side,
            self.asset_type,
            self.resource_kind,
            self.prefix,
            self.seller_wallet.as_deref().unwrap_or(""),
            forge_core::time::now_millis()
        );
        self.del_hash = sha256_hex(key.as_bytes());
    }

    pub async fn save(&mut self, pool: &PgPool) -> forge_db::sqlx::Result<i64> {
        self.assign_hash();
        if self.chain_id.is_empty() {
            self.chain_id = "monad".into();
        }
        if self.asset_type.is_empty() {
            self.asset_type = "ip".into();
        }
        if self.listing_side.is_empty() {
            self.listing_side = "offer".into();
        }
        if self.del_status.is_empty() {
            self.del_status = status::DRAFT.into();
        }
        if self.del_id > 0 {
            self.update(pool).await?;
            Ok(self.del_id)
        } else {
            let new_id = self.insert(pool).await?;
            self.del_id = new_id;
            Ok(new_id)
        }
    }

    /// Observer: only after USDC in lock.
    pub async fn list_open(pool: &PgPool) -> forge_db::sqlx::Result<Vec<Self>> {
        forge_db::sqlx::query_as::<_, Deal>(include_str!("../../sqls/deals_list_open.sql"))
            .fetch_all(pool)
            .await
    }

    /// Public search: lots in market + active deals + settled.
    pub async fn list_board(pool: &PgPool) -> forge_db::sqlx::Result<Vec<Self>> {
        forge_db::sqlx::query_as::<_, Deal>(include_str!("../../sqls/deals_list_board.sql"))
            .fetch_all(pool)
            .await
    }

    pub async fn list_for_user(pool: &PgPool, usr_id: i64) -> forge_db::sqlx::Result<Vec<Self>> {
        forge_db::sqlx::query_as::<_, Deal>(include_str!("../../sqls/deals_list_for_user.sql"))
            .bind(usr_id)
            .fetch_all(pool)
            .await
    }

    pub async fn list_listed(pool: &PgPool) -> forge_db::sqlx::Result<Vec<Self>> {
        Self::list_open(pool).await
    }

    /// Apply product action. Returns error string if illegal.
    pub fn apply_action(
        &mut self,
        action: &str,
        actor_usr_id: Option<i64>,
        buyer_wallet: Option<String>,
    ) -> Result<(), String> {
        use status::*;
        match action {
            "soft_verify" => {
                // IP: soft check done (email stub)
                if self.del_status != DRAFT && self.del_status != VERIFIED {
                    return Err("soft_verify only from draft".into());
                }
                self.soft_verified = true;
                self.del_status = VERIFIED.into();
            }
            "list" => {
                if !matches!(self.del_status.as_str(), DRAFT | VERIFIED) {
                    return Err("list only from draft/verified".into());
                }
                if self.asset_type == "ip" && !self.soft_verified {
                    return Err("IP lot needs soft_verify first".into());
                }
                self.ensure_deadline();
                self.del_status = LISTED.into();
            }
            "request" => {
                // Offer board: buyer wants to buy. Request board: seller responds to demand.
                if self.del_status != LISTED {
                    return Err("respond only on listed lot".into());
                }
                if self.listing_side == "request" {
                    // Seller taking a buy-request (seller already set by handler)
                    if self.seller_usr_id.is_none() {
                        self.seller_usr_id = actor_usr_id;
                    }
                    if self.seller_wallet.is_none() {
                        self.seller_wallet = buyer_wallet;
                    }
                } else {
                    self.buyer_usr_id = actor_usr_id;
                    self.buyer_wallet = buyer_wallet;
                }
                self.del_status = REQUESTED.into();
            }
            "accept" => {
                if self.del_status != REQUESTED {
                    return Err("accept only when requested".into());
                }
                self.del_status = ACCEPTED.into();
            }
            // Product order: accept → buyer funds → seller prepares → ready → confirm → wait oracle
            "fund" => {
                if self.del_status != ACCEPTED {
                    return Err("fund only after seller accepted".into());
                }
                let hash = self.del_hash.clone();
                let buyer = self
                    .buyer_wallet
                    .clone()
                    .or(buyer_wallet)
                    .unwrap_or_else(|| "0xBuyer".into());
                self.lock_tx = Some(crate::chain::mock_fund_tx(&hash, &buyer));
                self.del_status = FUNDED.into();
            }
            "start_prepare" => {
                if self.del_status != FUNDED {
                    return Err("prepare only after USDC is funded".into());
                }
                self.del_status = PREPARING.into();
            }
            "mark_prepared" => {
                if self.del_status != PREPARING {
                    return Err("mark ready only while preparing".into());
                }
                self.del_status = PREPARED.into();
            }
            "confirm_intent" => {
                if self.del_status != PREPARED {
                    return Err("confirm only after seller marked ready".into());
                }
                self.del_status = AWAITING_PROOF.into();
            }
            // Publish = soft verify + list (seller one-shot)
            "publish" => {
                if !matches!(self.del_status.as_str(), DRAFT | VERIFIED) {
                    return Err("publish only from draft/verified".into());
                }
                self.soft_verified = true;
                self.ensure_deadline();
                self.del_status = LISTED.into();
            }
            "open_dispute" => {
                if !matches!(
                    self.del_status.as_str(),
                    FUNDED | AWAITING_PROOF | PREPARED | PREPARING
                ) {
                    return Err("dispute not allowed in this status".into());
                }
                self.del_status = DISPUTE.into();
            }
            "cancel" => {
                if matches!(
                    self.del_status.as_str(),
                    RELEASED | REFUNDED | CANCELLED
                ) {
                    return Err("already terminal".into());
                }
                if matches!(self.del_status.as_str(), AWAITING_PROOF | FUNDED) {
                    // mock refund path
                    self.release_tx = Some(crate::chain::mock_refund_tx(&self.del_hash));
                    self.del_status = REFUNDED.into();
                } else {
                    self.del_status = CANCELLED.into();
                }
            }
            other => return Err(format!("unknown action: {other}")),
        }
        Ok(())
    }
}
