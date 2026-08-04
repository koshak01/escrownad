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

    /// ip | domain | work (v1 = ip)
    pub asset_type: String,
    /// PA | PI (when asset_type = ip)
    pub resource_kind: String,
    pub prefix: String,
    pub from_org: Option<String>,
    pub to_org: Option<String>,

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

impl Deal {
    pub async fn save(&mut self, pool: &PgPool) -> forge_db::sqlx::Result<i64> {
        let key = format!(
            "{}|{}|{}|{}",
            self.asset_type,
            self.resource_kind,
            self.prefix,
            self.seller_wallet.as_deref().unwrap_or("")
        );
        self.del_hash = sha256_hex(key.as_bytes());
        if self.chain_id.is_empty() {
            self.chain_id = "monad".into();
        }
        if self.asset_type.is_empty() {
            self.asset_type = "ip".into();
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
                self.del_status = LISTED.into();
            }
            "request" => {
                if self.del_status != LISTED {
                    return Err("request only on listed lot".into());
                }
                self.buyer_usr_id = actor_usr_id;
                self.buyer_wallet = buyer_wallet;
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
