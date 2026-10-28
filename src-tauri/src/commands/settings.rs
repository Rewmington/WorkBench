use serde_json::{Map, Value};
use tauri::State;

use crate::error::Error;
use crate::repos::settings;
use crate::state::AppState;
use crate::time::{Now, SystemNow};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub values: Map<String, Value>,
    pub paths: crate::state::PathsView,
    pub device_id: String,
}

#[tauri::command]
pub fn settings_all(state: State<'_, AppState>) -> Result<SettingsView, Error> {
    let conn = state.lock();
    let mut values = Map::new();
    for (k, v) in settings::all(&conn)? {
        values.insert(k, v);
    }
    let journal = crate::db::journal_mode(&conn);
    Ok(SettingsView {
        values,
        paths: state.paths.view(&journal),
        device_id: state.device_id.to_string(),
    })
}

#[tauri::command]
pub fn settings_get(state: State<'_, AppState>, key: String) -> Result<Value, Error> {
    let conn = state.lock();
    Ok(settings::get_json(&conn, &key)?.unwrap_or(Value::Null))
}

#[tauri::command]
pub fn settings_set(state: State<'_, AppState>, key: String, value: Value) -> Result<(), Error> {
    let now = SystemNow;
    let ms = now.ms();
    let conn = state.lock();
    settings::set_json(&conn, &key, &value, ms, &state.device_id)?;
    Ok(())
}
