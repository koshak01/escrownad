//! Публичные + admin pages.

pub mod about;
pub mod admin;
pub mod cabinet;
pub mod deals;
pub mod index;
pub mod oracle;

pub use about::AboutPage;
pub use cabinet::CabinetPage;
pub use deals::{DealNewPage, DealShowPage, DealsListPage};
pub use index::IndexPage;
pub use oracle::OraclePage;
