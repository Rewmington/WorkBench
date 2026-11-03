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

/// 通用设置入口只允许**纯本机 UX 键**。
///
/// `webdav.email`、`linked.medicine.davDir`、`linked.blog.repoDir` 这些不是「一个字符串键」
/// 那么简单：前者改的是备份/同步信封的 KDF 口令，中者是会带着应用密码出网的地址。
/// 专用命令在落库前各自有判据；通用命令能绕过那些判据，就等于那些钉子只是摆设。
/// 所以这里宁可要求新界面设置也走专门命令，也不开一个「什么都能写」的洞。
const UI_SETTINGS_KEYS: &[&str] = &[
    settings::KEY_THEME_MODE,
    settings::KEY_THEME_PRESET,
    settings::KEY_THEME_ACCENT,
    settings::KEY_MOTION_REDUCED,
    settings::KEY_WIDGET_ORDER,
    settings::KEY_BACKUP_ENABLED,
];

#[tauri::command]
pub fn settings_set(state: State<'_, AppState>, key: String, value: Value) -> Result<(), Error> {
    if !UI_SETTINGS_KEYS.contains(&key.as_str()) {
        return Err(Error::msg(format!(
            "设置项 {key} 要走它自己的命令，不能从通用设置口写"
        )));
    }
    let now = SystemNow;
    let ms = now.ms();
    let conn = state.lock();
    settings::set_json(&conn, &key, &value, ms, &state.device_id)?;
    Ok(())
}
