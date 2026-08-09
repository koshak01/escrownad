//! Market board — exchange: offers (sell) + requests (buy demand).

use async_trait::async_trait;
use forge_core::Timestamp;
use forge_ws::{Page, RequestContext, WsError, WsResult};
use serde::Serialize;

use crate::app_context;
use crate::models::Deal;

pub const OFFER_TYPES: &[&str] = &["ip", "domain", "property", "work", "other"];

#[derive(Debug, Serialize)]
pub struct OfferRow {
    /// The permanent hash — the card's address `/deals/<hash>/`.
    del_hash: String,
    /// Public lot number `ddd-ddd` (the id is never shown outside).
    deal_no: String,
    /// offer | request
    listing_side: String,
    offer_type: String,
    /// Registry: RIPE | ARIN | APNIC | LACNIC | AFRINIC.
    rir: String,
    /// Addresses in the block — what a buyer reads, rather than the mask.
    ///
    /// Signed on purpose: the template formats it with the platform's number
    /// filter, which takes an i64. An IPv4 block cannot exceed 2^32 addresses,
    /// so nothing is lost.
    addresses: Option<i64>,
    description: String,
    /// Raw FixedN<8> — formatted by a filter in the template. Price never
    /// leaves as an f64: money is integers only.
    price_raw: i64,
    listed_at: String,
    listed_for: String,
    expires_at: String,
    /// Creator verified (seller on offer, buyer on request).
    party_verified: bool,
    del_status: String,
}

/// Which set of deals the board shows.
///
/// - `all` — the market: live listings only (`listed`, not expired);
/// - `settled` — completed deals, where money was released against a registry
///   fact. Shown to everyone, signed in or not: it is the proof the mechanism
///   works, and every row can be re-checked in RIPE's public tables;
/// - `mine` — listed by me, or where I am a party, in any status;
/// - `arbitration` — disputes where I am a party.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardScope {
    All,
    Settled,
    Mine,
    Arbitration,
}

impl BoardScope {
    pub fn parse(raw: Option<&str>) -> Self {
        match raw.map(str::trim).unwrap_or("") {
            "settled" => Self::Settled,
            "mine" => Self::Mine,
            "arbitration" => Self::Arbitration,
            _ => Self::All,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Settled => "settled",
            Self::Mine => "mine",
            Self::Arbitration => "arbitration",
        }
    }

    /// In personal sets the status carries meaning — it is the stage. On the
    /// public board every row is live, so a status column would say nothing.
    fn shows_status(self) -> bool {
        !matches!(self, Self::All | Self::Settled)
    }
}

/// Board selection parameters — the same for the page and for live search.
#[derive(Debug, Default, Serialize)]
pub struct BoardFilter {
    pub side: String,
    pub query: String,
    pub scope: String,
}

impl BoardFilter {
    /// Builds the filter from the page's query parameters.
    fn from_query(ctx: &RequestContext) -> Self {
        let side = ctx
            .query
            .get("side")
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| s == "offer" || s == "request")
            .unwrap_or_default();
        Self {
            side,
            query: ctx
                .query
                .get("q")
                .map(|s| s.trim().to_string())
                .unwrap_or_default(),
            scope: BoardScope::parse(ctx.query.get("scope").map(|s| s.as_str()))
                .as_str()
                .to_string(),
        }
    }

    /// Does this row match the filter (side plus text search)?
    ///
    /// `may_search_prefix` — whether to search the network itself. Not for
    /// anonymous visitors: otherwise a hidden network could be brute-forced
    /// through the search box.
    fn matches(&self, deal: &Deal, may_search_prefix: bool) -> bool {
        if !self.side.is_empty() && deal.listing_side != self.side {
            return false;
        }
        if !self.query.is_empty() {
            let needle = self.query.to_lowercase();
            let haystack = format!(
                "{} {} {} {} {}",
                if may_search_prefix {
                    deal.prefix.as_str()
                } else {
                    ""
                },
                crate::sanitize::plain_text(deal.del_title.as_deref().unwrap_or("")),
                deal.geo.as_deref().unwrap_or(""),
                deal.rir,
                deal.from_org.as_deref().unwrap_or("")
            )
            .to_lowercase();
            if !haystack.contains(&needle) {
                return false;
            }
        }
        true
    }
}

/// Does this deal belong to the selected set?
fn in_scope(deal: &Deal, scope: BoardScope, actor: Option<i64>) -> bool {
    let is_party = actor.is_some() && (deal.seller_usr_id == actor || deal.buyer_usr_id == actor);
    match scope {
        // The market: live listings only. Drafts, deals in progress and
        // finished ones do not hang on the public board.
        // A taken lot leaves the board: while the hold lasts nobody else can
        // touch it, so showing it as available would be a lie.
        BoardScope::All => deal.del_status == "listed" && !deal.is_expired(),
        // The record of completed deals is public; no sign-in needed.
        BoardScope::Settled => deal.del_status == "released",
        BoardScope::Mine => is_party,
        BoardScope::Arbitration => is_party && deal.del_status == "dispute",
    }
}

/// Builds the board's rows — one path for the page and for live search.
///
/// # Parameters
/// * `filter` — side and query string
/// * `scope` — which set to show
/// * `actor` — the current user (for "mine" and "arbitration")
///
/// # Returns
/// * `(rows, total in the set)`
pub async fn board_rows(
    filter: &BoardFilter,
    scope: BoardScope,
    actor: Option<i64>,
) -> Result<(Vec<OfferRow>, usize), String> {
    let deals = app_context()
        .db
        .list_deals_board()
        .await
        .map_err(|e| format!("list_deals_board: {e}"))?;
    let verified_users = app_context()
        .db
        .list_verified_sellers()
        .await
        .unwrap_or_default();

    let scoped: Vec<&Deal> = deals.iter().filter(|d| in_scope(d, scope, actor)).collect();
    let total = scoped.len();
    let rows = scoped
        .into_iter()
        .filter(|d| filter.matches(d, actor.is_some()))
        .map(|d| to_offer_row(d, &verified_users))
        .collect();
    Ok((rows, total))
}

fn listed_for(from: Timestamp) -> String {
    let now = Timestamp::now().raw();
    let then = from.raw();
    let secs = (now - then).max(0);
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        format!("{}m", secs / 60)
    } else if secs < 86400 {
        format!("{}h", secs / 3600)
    } else {
        format!("{}d", secs / 86400)
    }
}

/// Plain one-line text for market board (wysiwyg stores HTML).
fn strip_html_one_line(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    let t = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() { "—".into() } else { t }
}

fn description(d: &Deal) -> String {
    d.del_title
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| d.del_note.as_deref().filter(|s| !s.trim().is_empty()))
        .map(strip_html_one_line)
        .unwrap_or_else(|| "—".into())
}

/// Date for tables and cards — the short form `2026-08-05`.
fn date_short(ts: Timestamp) -> String {
    ts.format("%Y-%m-%d")
}

/// Date with time — for the deal card.
fn date_full(ts: Timestamp) -> String {
    ts.format("%Y-%m-%d %H:%M")
}

fn to_offer_row(d: &Deal, verified_users: &[i64]) -> OfferRow {
    let creator = if d.listing_side == "request" {
        d.buyer_usr_id
    } else {
        d.seller_usr_id
    };
    let party_verified = creator
        .map(|id| verified_users.contains(&id))
        .unwrap_or(false);
    OfferRow {
        del_hash: d.del_hash.clone(),
        deal_no: d.public_no(),
        listing_side: if d.listing_side.is_empty() {
            "offer".into()
        } else {
            d.listing_side.clone()
        },
        offer_type: d.asset_type.clone(),
        rir: d.rir.clone(),
        addresses: crate::models::deal::address_count(&d.prefix).map(|n| n as i64),
        description: description(d),
        price_raw: d.del_amount.raw(),
        listed_at: date_short(d.del_dat),
        listed_for: listed_for(d.del_dat),
        expires_at: d.deadline_ts.map(date_short).unwrap_or_else(|| "—".into()),
        party_verified,
        del_status: d.del_status.clone(),
    }
}

pub struct DealsListPage;

#[async_trait]
impl Page for DealsListPage {
    fn path(&self) -> &'static str {
        "/deals/"
    }
    fn template(&self) -> &'static str {
        "deals/list.html.tera"
    }
    async fn load(&self, ctx: &mut RequestContext) -> WsResult<()> {
        let filter = BoardFilter::from_query(ctx);
        let scope = BoardScope::parse(Some(filter.scope.as_str()));
        let actor = ctx.user.as_ref().map(|u| u.usr_id);

        let (rows, total) = board_rows(&filter, scope, actor)
            .await
            .map_err(WsError::PageLoad)?;

        let fund = insurance_fund_usdc(&ctx.global.constants).await;

        ctx.insert("shown", &rows.len());
        ctx.insert("total", &total);
        ctx.insert("deals", &rows);
        ctx.insert("filter", &filter);
        ctx.insert("scope", &scope.as_str());
        ctx.insert("show_status", &scope.shows_status());
        ctx.insert("logged_in", &actor.is_some());
        ctx.insert("insurance_fund", &fund);
        Ok(())
    }
}

pub struct DealNewPage;

#[async_trait]
impl Page for DealNewPage {
    fn path(&self) -> &'static str {
        "/deals/new/"
    }
    fn template(&self) -> &'static str {
        "deals/new.html.tera"
    }
    async fn load(&self, ctx: &mut RequestContext) -> WsResult<()> {
        ctx.insert("user", &ctx.user.is_some());
        ctx.insert("offer_types", &OFFER_TYPES);
        // default listing lifetime — +31 days
        let default_deadline =
            Timestamp(Timestamp::now().raw() + crate::models::deal::DEFAULT_LISTING_DAYS * 86_400);
        ctx.insert("default_deadline", &date_short(default_deadline));
        // default side from query ?side=request
        let side = ctx.query.get("side").map(|s| s.as_str()).unwrap_or("offer");
        let side = if side == "request" {
            "request"
        } else {
            "offer"
        };
        ctx.insert("listing_side", &side);
        Ok(())
    }
}

/// Cache for the insurance fund balance: the chain is asked at most once a
/// minute, otherwise every render of the board would cost an RPC call.
static FUND_CACHE: std::sync::OnceLock<tokio::sync::RwLock<Option<(std::time::Instant, String)>>> =
    std::sync::OnceLock::new();

const FUND_CACHE_TTL: std::time::Duration = std::time::Duration::from_secs(60);

/// Insurance fund balance, for display on the board.
///
/// Read from the chain at the fund's address — a checkable figure: anyone can
/// open the explorer and see the same amount.
///
/// # Returns
/// * `Some(string)` — e.g. `"12.50"` · `None` — the chain is unconfigured or
///   unreachable, in which case the block is simply not shown
async fn insurance_fund_usdc(
    constants: &std::collections::HashMap<String, serde_json::Value>,
) -> Option<String> {
    let cache = FUND_CACHE.get_or_init(|| tokio::sync::RwLock::new(None));
    if let Some((at, value)) = cache.read().await.as_ref()
        && at.elapsed() < FUND_CACHE_TTL
    {
        return Some(value.clone());
    }

    let config = crate::chain::types::ChainConfig::from_constants(constants)?;
    let reader = crate::chain::core::ChainReader::new(&config)?;
    let raw = match reader.insurance_balance().await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "could not read the insurance fund balance");
            return None;
        }
    };

    let units: u128 = raw.to::<u128>();
    let value = usdc_display(units);
    *cache.write().await = Some((std::time::Instant::now(), value.clone()));
    Some(value)
}

/// USDC base units as a person would write the amount.
///
/// Thousands are separated, and a round sum stays round: `1` rather than
/// `1.00`. Trailing zeros in money read as false precision — nobody writes the
/// price of a block as "286 720.00".
///
/// # Parameters
/// * `units` — amount in USDC base units (6 decimals)
///
/// # Returns
/// * `String` — e.g. `286 720`, `1 024.5`, `0.02`
fn usdc_display(units: u128) -> String {
    let whole = units / 1_000_000;
    let frac = units % 1_000_000;

    let digits = whole.to_string();
    let mut grouped = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            grouped.push(' ');
        }
        grouped.push(c);
    }

    if frac == 0 {
        return grouped;
    }
    // two decimals is how money reads; drop a trailing zero, keep a leading one
    let cents = frac / 10_000;
    if cents == 0 {
        return grouped;
    }
    if cents.is_multiple_of(10) {
        format!("{grouped}.{}", cents / 10)
    } else {
        format!("{grouped}.{cents:02}")
    }
}

/// Everything the buyer's wallet needs in order to fund the lock.
#[derive(Debug, Serialize)]
struct ChainParams {
    chain_id: u64,
    /// hex chain id for `wallet_switchEthereumChain`
    chain_id_hex: String,
    rpc_url: String,
    usdc: String,
    lock: String,
    deal_id: String,
    condition_hash: String,
    seller: String,
    /// Amount in USDC base units, as a string — JS loses precision on big numbers.
    amount_units: String,
    deadline: u64,
}

/// Clickable Monad explorer URLs for the deal card's On-chain / Parties block.
///
/// Each field is `Some` only when the stored value is a real hex word that the
/// explorer can open. Mock stand-ins stay as plain text in the template.
#[derive(Debug, Default, Serialize)]
struct ExplorerLinks {
    /// EscrowLock contract (verified source on the explorer).
    contract: Option<String>,
    /// Label shown next to the contract link (short address).
    contract_label: Option<String>,
    /// Fund / lock transaction, when we actually stored a tx hash.
    lock_tx: Option<String>,
    /// Release transaction — the proof money left the lock.
    release_tx: Option<String>,
    seller: Option<String>,
    buyer: Option<String>,
    /// Cleanverse A-Token address, when the lot was issued as a verified asset.
    asset: Option<String>,
}

/// Builds explorer links for one deal.
///
/// Chain id comes from the live `chain` constant when present; otherwise we
/// fall back to Monad testnet (10143), which is where every public demo runs.
fn explorer_links(deal: &Deal, ctx: &RequestContext) -> ExplorerLinks {
    let chain_id = crate::chain::types::ChainConfig::from_constants(&ctx.global.constants)
        .map(|c| c.chain_id)
        .filter(|id| *id > 0)
        .unwrap_or(10143);

    let lock_addr = crate::chain::types::ChainConfig::from_constants(&ctx.global.constants)
        .map(|c| c.lock)
        .filter(|s| !s.trim().is_empty());

    // `lock_tx` is sometimes the on-chain deal id (bytes32), not a transaction
    // hash — that value must not become a /tx/ link or the explorer 404s mid-demo.
    let deal_fingerprint = crate::chain::deal_id_hex(&deal.del_hash);
    let lock_tx_url = deal.lock_tx.as_deref().and_then(|raw| {
        let t = raw.trim();
        if t.eq_ignore_ascii_case(&deal_fingerprint) {
            return None;
        }
        crate::chain::explorer_tx_url(chain_id, t)
    });

    ExplorerLinks {
        contract_label: lock_addr.clone(),
        contract: lock_addr
            .as_deref()
            .and_then(|a| crate::chain::explorer_address_url(chain_id, a)),
        lock_tx: lock_tx_url,
        release_tx: deal
            .release_tx
            .as_deref()
            .and_then(|t| crate::chain::explorer_tx_url(chain_id, t)),
        seller: deal
            .seller_wallet
            .as_deref()
            .and_then(|a| crate::chain::explorer_address_url(chain_id, a)),
        buyer: deal
            .buyer_wallet
            .as_deref()
            .and_then(|a| crate::chain::explorer_address_url(chain_id, a)),
        asset: deal
            .asset_token
            .as_deref()
            .and_then(|a| crate::chain::explorer_address_url(chain_id, a)),
    }
}

/// Builds the payment parameters. `None` when the chain is unconfigured, or
/// the deal has nothing to pay for (no seller, no amount, no deadline).
///
/// Settings come from the `chain` constant, which lands in the global context
/// when ws starts and is edited through `/admin/constants/`.
fn chain_params(deal: &Deal, ctx: &RequestContext) -> Option<ChainParams> {
    let config = crate::chain::types::ChainConfig::from_constants(&ctx.global.constants)?;
    if !config.mode().is_live() || config.chain_id == 0 {
        return None;
    }
    let seller = deal.seller_wallet.clone().filter(|s| !s.is_empty())?;
    let amount = crate::chain::core::usdc_units(deal.del_amount).ok()?;
    if amount.is_zero() {
        return None;
    }
    let deadline = deal.deadline_ts.map(|d| d.raw()).filter(|d| *d > 0)? as u64;
    let chain_id = config.chain_id;

    Some(ChainParams {
        chain_id,
        chain_id_hex: format!("0x{chain_id:x}"),
        rpc_url: config.rpc.clone(),
        usdc: config.usdc.clone(),
        lock: config.lock.clone(),
        deal_id: crate::chain::core::deal_id(&deal.del_hash).to_string(),
        condition_hash: crate::chain::core::condition_hash(
            &deal.prefix,
            &deal.resource_kind,
            deal.from_org.as_deref().unwrap_or(""),
            deal.to_org.as_deref().unwrap_or(""),
        )
        .to_string(),
        seller,
        amount_units: amount.to_string(),
        deadline,
    })
}

pub struct DealShowPage;

#[async_trait]
impl Page for DealShowPage {
    fn path(&self) -> &'static str {
        "/deals/{hash}/"
    }
    fn template(&self) -> &'static str {
        "deals/show.html.tera"
    }
    async fn load(&self, ctx: &mut RequestContext) -> WsResult<()> {
        let hash = ctx
            .path_params
            .get("hash")
            .filter(|s| !s.is_empty())
            .cloned()
            .ok_or_else(|| WsError::PageLoad("invalid deal hash".into()))?;
        let deal = app_context()
            .db
            .get_deal_by_hash(hash)
            .await
            .map_err(|e| WsError::PageLoad(format!("get_deal_by_hash: {e}")))?
            .ok_or_else(|| WsError::NotFound("deal not found".into()))?;

        let verified_users = app_context()
            .db
            .list_verified_sellers()
            .await
            .unwrap_or_default();
        let creator = if deal.listing_side == "request" {
            deal.buyer_usr_id
        } else {
            deal.seller_usr_id
        };
        let party_verified = creator
            .map(|id| verified_users.contains(&id))
            .unwrap_or(false);

        // The actor's roles — the template shows only the buttons that belong
        // to them. These rules must match `ws_handlers::deals::authorize_action`.
        let actor = ctx.user.as_ref().map(|u| u.usr_id);
        let is_seller = actor.is_some() && actor == deal.seller_usr_id;
        let is_buyer = actor.is_some() && actor == deal.buyer_usr_id;
        let is_creator = if deal.listing_side == "request" {
            is_buyer
        } else {
            is_seller
        };

        ctx.insert("deal_no", &deal.public_no());
        ctx.insert(
            "addresses",
            &crate::models::deal::address_count(&deal.prefix).map(|n| n as i64),
        );
        // The network is the goods: before funding, only its size goes out.
        let may_see_prefix = deal.may_see_prefix(actor);
        ctx.insert("may_see_prefix", &may_see_prefix);
        ctx.insert(
            "shown_prefix",
            &if may_see_prefix {
                deal.prefix.clone()
            } else {
                crate::models::deal::masked_prefix(&deal.prefix)
            },
        );
        ctx.insert(
            "oracle_auto",
            &(deal.rir == crate::models::deal::RIR_WITH_ORACLE),
        );
        // the price reaches the template as raw FixedN — a filter formats it, not an f64
        ctx.insert("price_raw", &deal.del_amount.raw());

        // Parameters for the buyer's wallet: approve and fund happen in the
        // browser. Amounts go as strings — beyond 2^53 JS loses precision.
        let chain = chain_params(&deal, ctx);
        ctx.insert("chain_ready", &chain.is_some());
        if let Some(c) = chain {
            ctx.insert("chain", &c);
        }
        // Clickable explorer URLs for the On-chain card. Plain hex is useless
        // in a demo: judges (and we) need one click into Monadscan.
        ctx.insert("explorer", &explorer_links(&deal, ctx));
        ctx.insert("created_at", &date_full(deal.del_dat));
        ctx.insert(
            "expires_at",
            &deal
                .deadline_ts
                .map(date_full)
                .unwrap_or_else(|| "—".into()),
        );
        ctx.insert("is_expired", &deal.is_expired());
        ctx.insert("reserve_expired", &deal.reserve_expired());
        ctx.insert(
            "reserved_until",
            &deal.reserved_until.map(date_full).unwrap_or_default(),
        );
        ctx.insert("declines", &deal.declines);
        ctx.insert("is_seller", &is_seller);
        ctx.insert("is_buyer", &is_buyer);
        ctx.insert("is_creator", &is_creator);
        ctx.insert("is_party", &(is_seller || is_buyer));
        ctx.insert("usr_id", &actor);
        ctx.insert("seller_verified", &party_verified);
        ctx.insert("party_verified", &party_verified);
        ctx.insert("listed_for", &listed_for(deal.del_dat));
        ctx.insert("deal", &deal);
        Ok(())
    }
}
