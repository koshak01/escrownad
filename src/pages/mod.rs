//! Проектные страницы skeleton'а. Здесь — только главная.
//! Реальные проекты добавляют свои страницы поверх этого минимума:
//!   - публичные (`pages::CartPage`, `pages::ProductPage`, ...)
//!   - проектные admin (`pages::admin::categories::list::ListPage`)
//!
//! Ядерные admin-страницы (users, roles, menus, constants, templates,
//! telegrams) подключаются через `forge_admin::pages::all()` в `crate::pages()`.

pub mod admin;
pub mod index;

pub use index::IndexPage;
