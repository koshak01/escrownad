//! Reference save/delete ws handlers for the `demos` domain entity.
//!
//! The struct travels end to end: the payload arrives as `DemoSaveParams`,
//! then `From<DemoSaveParams> for Demo`, then `db.save_demo(p.into())`. No
//! hand-reassembling of fields inside the handler.
//!
//! Field names are identical in EVERY layer: `dmo_code` in SQL, in `Demo`, in
//! `DemoSaveParams`, in `<field name="dmo_code">`, in the JS payload. One grep
//! shows a field's whole life.
//!
//! Cache hooks: the platform's `constants_save/delete` call
//! `AdminHooks::on_constants_changed` to invalidate the constants cache in
//! database. Demo is not cached, so it needs no hook. For a cached entity of
//! your own, call `on_<entity>_changed` after save and delete.

use forge_ws::ActionResp;
use serde::Deserialize;

use crate::app_context;
use crate::models::Demo;

/// The save form's payload. It differs from `Demo` only by `id`, hidden in the
/// URL, and by Option wrappers, since a form may omit a field — hence a
/// separate struct plus a `From` impl.
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
            // The demo form does not send the sample fields (dmo_hash,
            // dmo_amount, dmo_event_ts), so they take Default — `Demo::save`
            // overwrites the hash anyway. In an entity where those are
            // editable, add them to Params and to the form.
            ..Default::default()
        }
    }
}

/// Save a demo — `Demo::save` decides insert or update by `dmo_id`.
pub async fn demos_save(p: DemoSaveParams) -> Result<ActionResp, String> {
    app_context()
        .db
        .save_demo(p.into())
        .await
        .map_err(|e| e.to_string())?;
    Ok(ActionResp::redirect_with_success(
        "/admin/demos/",
        "Demo saved",
    ))
}

#[derive(Debug, Deserialize)]
pub struct DemoDeleteParams {
    pub id: i64,
}

/// Delete a demo by id.
pub async fn demos_delete(p: DemoDeleteParams) -> Result<ActionResp, String> {
    app_context()
        .db
        .delete_demo(p.id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(ActionResp::redirect_with_success(
        "/admin/demos/",
        "Demo deleted",
    ))
}
