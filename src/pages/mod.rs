//! Публичные + admin pages.

pub mod admin;
pub mod deals;
pub mod index;
pub mod oracle;

pub use deals::{DealShowPage, DealsListPage};
pub use index::IndexPage;
pub use oracle::OraclePage;
