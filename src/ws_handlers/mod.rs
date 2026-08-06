//! WS handlers. The skeleton ships a single `echo` handler demonstrating
//! WS-pushed HTML — no page reloads.
//!
//! Every handler:
//!   1. Takes typed Params (`#[derive(Deserialize)]`).
//!   2. Does its work through `crate::app_context()` (db / notifier / redis).
//!   3. Renders partial HTML through `app_context().renderer`.
//!   4. Returns an `ActionResp` carrying `Action::ReplaceHtml` — the client
//!      injects `html` into `selector` without a reload.
//!
//! Real projects add their own handlers — `cart_add`, `order_create`,
//! `category_save` and so on. Registration happens in `bin/ws.rs` through
//! `router.route(...)` or the wsgate dispatch table.

pub mod deals;
pub mod demos;
pub mod echo;

pub use deals::{
    DealActionParams, DealFundedParams, DealSaveParams, DealSearchParams, deals_action,
    deals_funded, deals_save, deals_search,
};
pub use demos::{DemoDeleteParams, DemoSaveParams, demos_delete, demos_save};
pub use echo::echo;
