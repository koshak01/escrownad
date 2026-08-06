//! Domain models: demo (the reference entity) and deals (proof escrow).

pub mod deal;
pub mod demo;

pub use deal::{Deal, DealListFilter};
pub use demo::{Demo, DemoListFilter};
