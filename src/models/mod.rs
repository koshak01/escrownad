//! Доменные модели: demo (эталон) + deals (proof-escrow).

pub mod deal;
pub mod demo;

pub use deal::{Deal, DealListFilter};
pub use demo::{Demo, DemoListFilter};
