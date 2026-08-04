//! ЭТАЛОН ws-handler'ов save/delete доменной сущности `demos`.
//!
//! Сквозная структура (`forge/docs/architecture/03_handlers.md`, паттерн 2):
//! payload приходит как `DemoSaveParams` → `From<DemoSaveParams> for Demo` →
//! `db.save_demo(p.into())`. Никакой ручной пересборки полей в handler'е.
//!
//! Имена полей одинаковые во ВСЕХ слоях: `dmo_code` в SQL, в `Demo`, в
//! `DemoSaveParams`, в `<field name="dmo_code">`, в JS-payload. Один grep — вся
//! жизнь поля.
//!
//! Кэш-хук: ядерные `constants_save/delete` дёргают `AdminHooks::on_constants_changed`
//! (инвалидация кеша констант в database). Demo НЕ кешируется → hook не нужен.
//! Своей кешируемой сущности — дёрни `on_<entity>_changed` после save/delete.

use forge_ws::ActionResp;
use serde::Deserialize;

use crate::app_context;
use crate::models::Demo;

/// Payload формы сохранения. Отличается от `Demo` только `id` (hidden из URL) и
/// Option-обёртками (форма может не прислать поле) → отдельная struct + `From`.
#[derive(Debug, Deserialize)]
pub struct DemoSaveParams {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub dmo_code: Option<String>,
    #[serde(default)]
    pub dmo_title: Option<String>,
    #[serde(default)]
    pub dmo_note: Option<String>,
    #[serde(default)]
    pub dmo_is_enable: Option<bool>,
}

impl From<DemoSaveParams> for Demo {
    fn from(p: DemoSaveParams) -> Self {
        Self {
            dmo_id: p.id.unwrap_or(0),
            dmo_code: p.dmo_code,
            dmo_title: p.dmo_title,
            dmo_note: p.dmo_note,
            dmo_is_enable: p.dmo_is_enable.unwrap_or(true),
            // Типовые поля-образцы (dmo_hash/dmo_amount/dmo_event_ts) форма demo не
            // шлёт → Default (hash перезапишет `Demo::save`). В своей сущности,
            // где эти поля редактируемы, добавь их в Params + форму.
            ..Default::default()
        }
    }
}

/// Сохранить demo (insert/update решает `Demo::save` по `dmo_id`).
pub async fn demos_save(p: DemoSaveParams) -> Result<ActionResp, String> {
    app_context()
        .db
        .save_demo(p.into())
        .await
        .map_err(|e| e.to_string())?;
    Ok(ActionResp::redirect_with_success(
        "/admin/demos/",
        "Демо сохранено",
    ))
}

#[derive(Debug, Deserialize)]
pub struct DemoDeleteParams {
    pub id: i64,
}

/// Удалить demo по id.
pub async fn demos_delete(p: DemoDeleteParams) -> Result<ActionResp, String> {
    app_context()
        .db
        .delete_demo(p.id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ActionResp::redirect_with_success("/admin/demos/", "Демо удалено"))
}
