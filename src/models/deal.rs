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
    /// Waiting for the operator's decision — not on the board yet.
    pub const MODERATION: &str = "moderation";
    /// A buyer has taken the lot: it leaves the board and is held for them
    /// for a limited time, until the seller confirms the deal.
    pub const RESERVED: &str = "reserved";
    /// The operator declined the listing.
    pub const REJECTED: &str = "rejected";
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
    #[db(rename = "del_asset_type")]
    #[sqlx(rename = "del_asset_type")]
    pub asset_type: String,
    /// PA | PI (when asset_type = ip)
    #[db(rename = "del_resource_kind")]
    #[sqlx(rename = "del_resource_kind")]
    pub resource_kind: String,
    #[db(rename = "del_prefix")]
    #[sqlx(rename = "del_prefix")]
    pub prefix: String,

    /// `offer` = sell listing · `request` = buy demand (exchange).
    #[db(rename = "del_listing_side")]
    #[sqlx(rename = "del_listing_side")]
    pub listing_side: String,
    /// Holder organisation in the registry — the oracle looks for a transfer row by it.
    #[db(rename = "del_from_org")]
    #[sqlx(rename = "del_from_org")]
    pub from_org: Option<String>,
    /// Receiving organisation. Usually empty at listing time: the buyer is
    /// only known once the deal is funded.
    #[db(rename = "del_to_org")]
    #[sqlx(rename = "del_to_org")]
    pub to_org: Option<String>,
    /// Regional registry: RIPE | ARIN | APNIC | LACNIC | AFRINIC.
    /// Automatic fact checking currently exists for RIPE only.
    #[db(rename = "del_rir")]
    #[sqlx(rename = "del_rir")]
    pub rir: String,
    /// Where the block is — display only; takes no part in matching the fact.
    #[db(rename = "del_geo")]
    #[sqlx(rename = "del_geo")]
    pub geo: Option<String>,

    #[db(rename = "del_seller_wallet")]
    #[sqlx(rename = "del_seller_wallet")]
    pub seller_wallet: Option<String>,
    #[db(rename = "del_buyer_wallet")]
    #[sqlx(rename = "del_buyer_wallet")]
    pub buyer_wallet: Option<String>,
    pub seller_usr_id: Option<i64>,
    pub buyer_usr_id: Option<i64>,
    pub broker_usr_id: Option<i64>,

    pub del_amount: FixedN<8>,
    #[db(rename = "del_chain_id")]
    #[sqlx(rename = "del_chain_id")]
    pub chain_id: String,
    #[db(rename = "del_lock_tx")]
    #[sqlx(rename = "del_lock_tx")]
    pub lock_tx: Option<String>,
    #[db(rename = "del_release_tx")]
    #[sqlx(rename = "del_release_tx")]
    pub release_tx: Option<String>,

    pub del_status: String,
    #[db(rename = "del_deadline_ts")]
    #[sqlx(rename = "del_deadline_ts")]
    pub deadline_ts: Option<Timestamp>,
    #[db(rename = "del_ripe_match_key")]
    #[sqlx(rename = "del_ripe_match_key")]
    pub ripe_match_key: Option<String>,
    #[db(rename = "del_checklist_json")]
    #[sqlx(rename = "del_checklist_json")]
    pub checklist_json: Option<String>,

    #[db(rename = "del_soft_verified")]
    #[sqlx(rename = "del_soft_verified")]
    pub soft_verified: bool,
    #[db(rename = "del_contact_email")]
    #[sqlx(rename = "del_contact_email")]
    pub contact_email: Option<String>,

    /// Until when the lot stays held for the buyer.
    #[db(rename = "del_reserved_until")]
    #[sqlx(rename = "del_reserved_until")]
    pub reserved_until: Option<Timestamp>,
    /// How many times the seller backed out after their lot was taken.
    #[db(rename = "del_declines")]
    #[sqlx(rename = "del_declines")]
    pub declines: i16,

    pub del_is_enable: bool,

    /// Created at (DB default). Used for “listed for” age on the market board.
    #[db(skip_insert)]
    pub del_dat: Timestamp,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, ListFilter)]
#[list_filter(model = Deal)]
pub struct DealListFilter {
    #[filter(text, col = "del_prefix", label = "Prefix")]
    pub prefix: Option<String>,
    #[filter(text, col = "del_status", label = "Status")]
    pub status: Option<String>,
    #[filter(text, col = "del_resource_kind", label = "Kind")]
    pub kind: Option<String>,
    #[filter(text, col = "del_asset_type", label = "Asset")]
    pub asset_type: Option<String>,
}

/// Default listing lifetime, in days from publication.
pub const DEFAULT_LISTING_DAYS: i64 = 31;

/// How long a lot stays held for the buyer after they take it.
///
/// Within this window the seller must confirm the deal. If they do not, the
/// lot returns to the board and somebody else can take it. The point of the
/// deadline is that a taken lot should not hang there dead while the seller
/// stays silent.
pub const RESERVE_HOURS: i64 = 6;

/// Regional internet registries.
///
/// Automatic confirmation of a transfer currently works for **RIPE** only —
/// it is the one registry publishing machine-readable transfer tables in a
/// usable form. Listings from the others are accepted, but releasing money
/// against them takes manual confirmation.
pub const RIRS: &[&str] = &["RIPE", "ARIN", "APNIC", "LACNIC", "AFRINIC"];

/// The registry for which automatic fact checking works.
pub const RIR_WITH_ORACLE: &str = "RIPE";

/// The prefix with the network address hidden — what is visible before funding.
///
/// The exact network *is* the subject of the deal: knowing it, a buyer can
/// look the block up and go straight to the holder, around the platform. So
/// outsiders get the size only; the address opens to whoever funded.
///
/// # Parameters
/// * `prefix` — the network as `194.246.124.0/23`
///
/// # Returns
/// * `String` — for example `•••.•••.•••.• /23`
pub fn masked_prefix(prefix: &str) -> String {
    match prefix.rsplit_once('/') {
        Some((_, bits)) => format!("•••.•••.•••.•/{}", bits.trim()),
        None => "•••.•••.•••.•".to_string(),
    }
}

/// How many addresses a block holds, from its CIDR notation.
///
/// Buyers think in address counts, not prefix lengths: `/22` tells them
/// nothing, `1024 addresses` tells them everything.
///
/// # Parameters
/// * `prefix` — the network as `194.246.124.0/23`, or without a mask
///
/// # Returns
/// * `Some(count)` — addresses in the block · `None` — mask not recognised
pub fn address_count(prefix: &str) -> Option<u64> {
    let bits: u32 = prefix.rsplit_once('/')?.1.trim().parse().ok()?;
    if bits > 32 {
        return None;
    }
    Some(1u64 << (32 - bits))
}

/// Public lot number `ddd-ddd`, derived from `del_hash`.
///
/// Only the hash and this number ever leave the system — in URLs, tables and
/// cards. `del_id` is shown nowhere.
///
/// # Parameters
/// * `del_hash` — the deal's hex hash
///
/// # Returns
/// * `String` — a number like `472-118`
pub fn public_no(del_hash: &str) -> String {
    let head = del_hash.get(..6).unwrap_or("000000");
    let n = u32::from_str_radix(head, 16).unwrap_or(0) % 1_000_000;
    format!("{:03}-{:03}", n / 1000, n % 1000)
}

impl Deal {
    /// Public lot number — see [`public_no`].
    pub fn public_no(&self) -> String {
        public_no(&self.del_hash)
    }

    /// Who may be shown the exact network.
    ///
    /// Only the person who listed it and the buyer who funded it. Everyone
    /// else sees the mask: the network opens for a deposit, not for a glance.
    ///
    /// # Parameters
    /// * `actor` — the current user, when signed in
    ///
    /// # Returns
    /// * `true` — the full `prefix` may be shown
    pub fn may_see_prefix(&self, actor: Option<i64>) -> bool {
        if actor.is_none() {
            return false;
        }
        let creator = if self.listing_side == "request" {
            self.buyer_usr_id
        } else {
            self.seller_usr_id
        };
        actor == creator || (actor == self.buyer_usr_id && self.is_paid())
    }

    /// The money is already in the lock (or further along).
    pub fn is_paid(&self) -> bool {
        matches!(
            self.del_status.as_str(),
            status::FUNDED
                | status::PREPARING
                | status::PREPARED
                | status::AWAITING_PROOF
                | status::RELEASED
                | status::DISPUTE
        )
    }

    /// The buyer's hold has expired — time to put the lot back on the board.
    pub fn reserve_expired(&self) -> bool {
        self.del_status == status::RESERVED
            && self
                .reserved_until
                .map(|t| t.raw() <= Timestamp::now().raw())
                .unwrap_or(true)
    }

    /// The listing has expired (for lots on the board).
    pub fn is_expired(&self) -> bool {
        self.deadline_ts
            .map(|d| d.raw() <= Timestamp::now().raw())
            .unwrap_or(false)
    }

    /// Sets the default expiry when the lister did not choose one.
    ///
    /// Called as the lot reaches the board (`list` / `publish`), so that no
    /// listing hangs on the market forever.
    fn ensure_deadline(&mut self) {
        if self.deadline_ts.is_none() {
            let secs = DEFAULT_LISTING_DAYS * 86_400;
            self.deadline_ts = Some(Timestamp(Timestamp::now().raw() + secs));
        }
    }

    /// Assigns the deal's permanent hash — idempotently.
    ///
    /// The hash is the public identifier: the `/deals/<hash>/` URL and the lot
    /// number are built from it. It is computed ONCE at creation and never
    /// again — otherwise the link would rot every time a party changed (a
    /// `buyer_wallet` appears → new hash → 404 on the old address).
    ///
    /// Called in the ws handler before going to the database (so we know where
    /// to redirect) and in [`Deal::save`] as a backstop for every other path.
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
            // The buyer takes the lot: it leaves the board and is held for
            // them for a set time. Nobody else can take it meanwhile — the
            // queue is settled by whoever got there first.
            "take" => {
                if self.del_status != LISTED {
                    return Err("this lot is not available".into());
                }
                if self.is_expired() {
                    return Err("listing has expired".into());
                }
                self.buyer_usr_id = actor_usr_id;
                self.buyer_wallet = buyer_wallet;
                self.reserved_until =
                    Some(Timestamp(Timestamp::now().raw() + RESERVE_HOURS * 3600));
                self.del_status = RESERVED.into();
            }
            // The seller confirms — the buyer may now pay.
            "confirm_deal" => {
                if self.del_status != RESERVED {
                    return Err("nothing to confirm".into());
                }
                self.del_status = ACCEPTED.into();
            }
            // The seller backed out. The lot returns to the board and the
            // refusal is counted: somebody who routinely backs out after a
            // hold wastes buyers' time, and that should be visible.
            "decline_deal" => {
                if self.del_status != RESERVED {
                    return Err("nothing to decline".into());
                }
                self.declines = self.declines.saturating_add(1);
                self.buyer_usr_id = None;
                self.buyer_wallet = None;
                self.reserved_until = None;
                self.del_status = LISTED.into();
            }
            // The hold expired — the lot returns to the board, no mark against the seller.
            "release_reserve" => {
                if self.del_status != RESERVED {
                    return Err("lot is not reserved".into());
                }
                self.buyer_usr_id = None;
                self.buyer_wallet = None;
                self.reserved_until = None;
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
            // Submitted for review: a lot never reaches the board by itself.
            // An operator looks at it and decides — approve or decline, from Telegram.
            "publish" => {
                if !matches!(self.del_status.as_str(), DRAFT | VERIFIED | REJECTED) {
                    return Err("publish only from draft/verified".into());
                }
                self.soft_verified = true;
                self.ensure_deadline();
                self.del_status = MODERATION.into();
            }
            // The operator's decision.
            "approve" => {
                if self.del_status != MODERATION {
                    return Err("approve only from moderation".into());
                }
                self.ensure_deadline();
                self.del_status = LISTED.into();
            }
            "decline" => {
                if self.del_status != MODERATION {
                    return Err("decline only from moderation".into());
                }
                self.del_status = REJECTED.into();
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
                if matches!(self.del_status.as_str(), RELEASED | REFUNDED | CANCELLED) {
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
