//! Reference domain model of a project — `demos`.
//!
//! This is NOT a business entity but a teaching exhibit: it shows how to stand
//! up admin CRUD for your own table on this platform. Every field demonstrates
//! one capability of the standard list and form:
//!
//!   - `dmo_hash`      — logical uniqueness: `#[db(unique, hash)]`, sha256 of a business key
//!   - `dmo_code`      — unique code: text filter (ILIKE) plus a sortable column
//!   - `dmo_title`     — heading: a second sortable column and an ordinary form field
//!   - `dmo_note`      — free text: a "wide" field in the form
//!   - `dmo_amount`    — money as `FixedN<8>` (or the `Price` alias) → SQL `bigint`, NOT numeric/f64
//!   - `dmo_event_ts`  — domain time as `Timestamp` → SQL `timestamp` in UTC, NOT timestamptz
//!   - `dmo_is_enable` — flag: an inline toggle in the row plus a tribool filter
//!
//! Timestamps (`dmo_dat` / `dmo_updated`) are set by the database itself
//! (`DEFAULT now()`) and do not appear in the struct — as with the platform's
//! own `Constant` and `Telegram`.
//!
//! Build your own entity from this template.
//!
//! ## Type conventions — read this BEFORE writing your own table
//!
//! In places the platform goes AGAINST the industry default, precisely where
//! "the way the Postgres tutorial teaches it" leads into a pit. These are the
//! mistakes new projects make most often:
//!
//! | Quantity              | Platform type (Rust)             | SQL type          | Do NOT use |
//! |---|---|---|---|
//! | Primary key           | `i64` + `#[db(skip_insert)]`     | `bigint …IDENTITY` | a composite PK |
//! | Money / price / volume| `FixedN<8>` or `Price`/`Volume`  | `bigint`          | `numeric`, `f64`, your own alias |
//! | Percentage            | `FixedN<4>` or `Percent`         | `bigint`          | `numeric`, `f64`, your own alias |
//! | Time (seconds)        | `Timestamp` (`forge_core`)       | `timestamp` UTC   | `timestamptz` |
//! | Time (milliseconds)   | `TimestampMs`                    | `timestamp` UTC   | `timestamptz`, `bigint` |
//! | Created / updated     | — (absent from the model)        | `timestamp DEFAULT now()` | keeping it in Rust |
//! | Logical uniqueness    | `String` `#[db(unique, hash)]`   | `varchar` (sha256) | a composite PK |
//!
//! Why: money is integral (bigint), exact arithmetic without floats; time is
//! UTC throughout as `timestamp`, with no timezone conversions. Copy this model
//! and the types are already right.
//!
//! ## Removing or renaming the demo entity — every wiring point
//!
//! Everything greps on `demo` / `Demo` / `dmo_`. The points are:
//!   - `src/models/demo.rs` (this file) and `src/models/mod.rs`
//!   - `src/lib.rs` — 4 `DbCommand` variants, 2 `DbResponse` variants,
//!     4 `client!` methods, 3 `Box::new(...)` entries in `pages()`
//!   - `src/bin/database.rs` — 4 `match` arms plus `register_list_model("demos", …)`
//!   - `src/bin/ws.rs` — 2 macro declarations, 2 delegates, the params `use`
//!   - `seeds/demos.sql` and the table itself: `DROP TABLE demos;`
//!
//! Delete all of it at once: a partial removal will not build, because
//! `client!`, the `match` and `pages()` would reference deleted types. Renaming
//! into your own entity (`orders` / `ord_`) touches the same points.

use forge_core::Timestamp;
use forge_core::hash::sha256_hex;
use forge_db::sqlx::{FromRow, PgPool};
use forge_db::{DbModel, ListFilter};
use forge_fixed_n::FixedN;
use serde::{Deserialize, Serialize};

/// The demo entity. `#[derive(DbModel)]` generates `query()` / `insert` /
/// `update` / `delete` / `find_by_id` / `delete_by_id` from `#[db(table, pk)]`.
#[derive(Debug, Clone, Default, FromRow, DbModel, Serialize, Deserialize)]
#[db(table = "demos", pk = "dmo_id")]
pub struct Demo {
    /// Primary key — serial, never part of an INSERT.
    #[db(skip_insert)]
    pub dmo_id: i64,

    /// A hash column is how **logical uniqueness** is expressed —
    /// `#[db(unique, hash)]`. When the logical key is a combination of columns
    /// rather than one, do NOT reach for a composite primary key. The PK stays
    /// `dmo_id bigserial`, and uniqueness comes from a hash of the business
    /// key. `#[db(unique, hash)]` yields `find_by_hash` / `delete_by_hash`. The
    /// value is produced by `save()` below (`sha256_hex(dmo_code)`); the SQL
    /// type is `varchar`.
    #[db(unique, hash)]
    pub dmo_hash: String,

    /// Unique code — `ILIKE` filter, sorting, and a `<code>` cell.
    #[db(unique)]
    pub dmo_code: Option<String>,

    /// Heading — the second sortable column.
    pub dmo_title: Option<String>,

    /// Free-form note — demonstrates a "wide" form field.
    pub dmo_note: Option<String>,

    /// Money. **SQL type `bigint`** (raw i64 = value × 10^8). NOT `numeric`
    /// and NOT `f64`: integer arithmetic, no floating-point drift. The scale
    /// lives only in Rust and has no presence in the database. `DbModel` reads
    /// and writes it automatically.
    ///
    /// A domain alias reads better for a money field — `Price`, `Volume`,
    /// `Percent`. `DbModel` recognises those on equal terms with a literal
    /// `FixedN<N>`. `Price` = `FixedN<8>`,
    /// `Volume` = `FixedN<8>`, `Percent` = `FixedN<4>`.
    ///
    /// But only the PLATFORM's aliases. A project's own alias
    /// (`type Rub = FixedN<2>`) is invisible to the macro: it matches the
    /// token as written, and a proc-macro cannot resolve aliases. For your own
    /// scale, write `FixedN<2>` literally or ask for a platform alias.
    pub dmo_amount: FixedN<8>,

    /// Domain event time — the `Timestamp` type (`forge_core`, UNIX seconds
    /// UTC). **SQL type `timestamp` without time zone, in UTC.** NOT
    /// `timestamptz`: the platform contract is "everything in the database is
    /// UTC as `timestamp`", with no Postgres timezone conversions. Millisecond
    /// tick data uses `TimestampMs`. The housekeeping `dmo_dat` / `dmo_updated`
    /// are not repeated here — the database sets them (`DEFAULT now()`).
    pub dmo_event_ts: Timestamp,

    /// Active flag — an inline toggle in the list row plus a tribool filter.
    pub dmo_is_enable: bool,
}

/// Typed list filter for `/admin/demos/` (structure-driven UI, ADR-0010).
/// `#[derive(ListFilter)]` generates `apply()`, which layers onto
/// `Demo::query()`, and `to_fields()`, the metadata for `filter_accordion`. It
/// travels over WS as canonical msgpack and is stored in the platform's
/// `filters.flt_params` — one source of truth.
#[derive(Debug, Clone, Default, Serialize, Deserialize, ListFilter)]
#[list_filter(model = Demo)]
pub struct DemoListFilter {
    /// Code — `ILIKE %v%`.
    #[filter(text, col = "dmo_code", label = "Code")]
    pub code: Option<String>,
    /// Active — All / Yes / No.
    #[filter(tribool, col = "dmo_is_enable", label = "Active")]
    pub is_enable: Option<bool>,
}

impl Demo {
    /// save: `dmo_id == 0` inserts and writes the new id back into self; otherwise updates.
    pub async fn save(&mut self, pool: &PgPool) -> forge_db::sqlx::Result<i64> {
        // The hash derives from the business key (dmo_code) — the reference use
        // of `#[db(unique, hash)]`. Generated HERE rather than in the form, so
        // it always stays consistent with the key and unique.
        self.dmo_hash = sha256_hex(self.dmo_code.as_deref().unwrap_or_default().as_bytes());
        if self.dmo_id > 0 {
            self.update(pool).await?;
            Ok(self.dmo_id)
        } else {
            let new_id = self.insert(pool).await?;
            self.dmo_id = new_id;
            Ok(new_id)
        }
    }
}
