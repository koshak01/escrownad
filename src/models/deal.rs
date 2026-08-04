//! Deal — proof-escrow сделка. Актив v1: IPv4 (RIPE PA|PI).

use forge_core::Timestamp;
use forge_core::hash::sha256_hex;
use forge_db::sqlx::{FromRow, PgPool};
use forge_db::{DbModel, ListFilter};
use forge_fixed_n::FixedN;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, FromRow, DbModel, Serialize, Deserialize)]
#[db(table = "deals", pk = "del_id")]
pub struct Deal {
    #[db(skip_insert)]
    pub del_id: i64,

    #[db(unique, hash)]
    pub del_hash: String,

    pub del_title: Option<String>,
    pub del_note: Option<String>,

    /// PA | PI
    pub resource_kind: String,
    /// e.g. 176.120.88.0/21
    pub prefix: String,
    pub from_org: Option<String>,
    pub to_org: Option<String>,

    pub seller_wallet: Option<String>,
    pub buyer_wallet: Option<String>,
    pub seller_usr_id: Option<i64>,
    pub buyer_usr_id: Option<i64>,

    /// Price raw × 10^8
    pub del_amount: FixedN<8>,
    pub chain_id: String,
    pub lock_tx: Option<String>,
    pub release_tx: Option<String>,

    /// draft|listed|funded|awaiting_proof|released|refunded|dispute|cancelled
    pub del_status: String,
    pub deadline_ts: Option<Timestamp>,
    pub ripe_match_key: Option<String>,
    /// Checklist JSON as string (sqlx Json later).
    pub checklist_json: Option<String>,

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
}

impl Deal {
    pub async fn save(&mut self, pool: &PgPool) -> forge_db::sqlx::Result<i64> {
        let key = format!(
            "{}|{}|{}",
            self.resource_kind,
            self.prefix,
            self.seller_wallet.as_deref().unwrap_or("")
        );
        self.del_hash = sha256_hex(key.as_bytes());
        if self.chain_id.is_empty() {
            self.chain_id = "monad".into();
        }
        if self.del_status.is_empty() {
            self.del_status = "draft".into();
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

    /// Open deals for observer (not terminal).
    pub async fn list_open(pool: &PgPool) -> forge_db::sqlx::Result<Vec<Self>> {
        forge_db::sqlx::query_as::<_, Deal>(
            r#"
            SELECT *
            FROM deals
            WHERE del_is_enable
              AND del_status IN ('listed', 'funded', 'awaiting_proof')
            ORDER BY del_id DESC
            LIMIT 100
            "#,
        )
        .fetch_all(pool)
        .await
    }

    /// Public board: open + recently settled (released/refunded/dispute).
    pub async fn list_board(pool: &PgPool) -> forge_db::sqlx::Result<Vec<Self>> {
        forge_db::sqlx::query_as::<_, Deal>(
            r#"
            SELECT *
            FROM deals
            WHERE del_is_enable
              AND del_status IN (
                'listed', 'funded', 'awaiting_proof',
                'released', 'refunded', 'dispute'
              )
            ORDER BY del_id DESC
            LIMIT 100
            "#,
        )
        .fetch_all(pool)
        .await
    }

    /// Legacy name used by observer + older pages.
    pub async fn list_listed(pool: &PgPool) -> forge_db::sqlx::Result<Vec<Self>> {
        Self::list_open(pool).await
    }
}
