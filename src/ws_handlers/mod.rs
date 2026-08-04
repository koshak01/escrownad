//! WS-обработчики. В skeleton — один `echo`-handler для демо WS-push HTML
//! (`forge/docs/CONVENTIONS.md §7.5` — никаких page-reload).
//!
//! Каждый handler:
//!   1. Принимает типизированный Params (`#[derive(Deserialize)]`).
//!   2. Делает работу через `crate::app_context()` (db / notifier / redis).
//!   3. Рендерит partial HTML через `app_context().renderer`.
//!   4. Возвращает `ActionResp` с `Action::ReplaceHtml` —
//!      клиент в JS инжектит `html` в `selector` без перезагрузки.
//!
//! Реальные проекты добавляют свои handler'ы: `cart_add`, `order_create`,
//! `category_save` и т.д. Регистрация — в `bin/ws.rs` через `router.route(...)`
//! или dispatch-таблицу wsgate.

pub mod demos;
pub mod echo;

pub use demos::{DemoDeleteParams, DemoSaveParams, demos_delete, demos_save};
pub use echo::echo;
