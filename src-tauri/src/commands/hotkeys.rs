//! 全局快捷键与开机自启的命令面。
//!
//! 两个都走 Rust 侧：JS 侧那两个插件包我们没装依赖，而且"注册成没成功"这件事
//! 本来就只有内核层知道。

use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::error::{Error, Result};
use crate::hotkeys;
use crate::state::AppState;

use super::in_tx;

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyView {
    pub action: String,
    pub label: String,
    pub combo: String,
    /// 当前是否真的挂上了。被别的程序抢走时，界面要能说清楚而不是假装能用。
    pub registered: bool,
}

fn is_registered(app: &AppHandle, combo: &str) -> bool {
    match combo.trim().parse::<Shortcut>() {
        Ok(shortcut) => app.global_shortcut().is_registered(shortcut),
        Err(_) => false,
    }
}

fn known_action(action: &str) -> Result<&'static str> {
    if action == hotkeys::ACTION_SHOW {
        Ok(hotkeys::ACTION_SHOW)
    } else {
        Err(Error::msg(format!("不认识的快捷键动作：{action}")))
    }
}

fn view(app: &AppHandle, action: &'static str, combo: String) -> HotkeyView {
    HotkeyView {
        action: action.to_owned(),
        label: "唤起窗口".to_owned(),
        registered: is_registered(app, &combo),
        combo,
    }
}

#[tauri::command]
pub fn hotkeys_list(app: AppHandle, state: State<'_, AppState>) -> Result<Vec<HotkeyView>> {
    let combo = {
        let conn = state.lock();
        hotkeys::combo_for(&conn, hotkeys::ACTION_SHOW)
    };
    Ok(vec![view(&app, hotkeys::ACTION_SHOW, combo)])
}

/// 换绑。失败时 Rust 侧已经回滚了旧绑定，所以前端只需显示这句话，不必自己恢复状态。
#[tauri::command]
pub fn hotkeys_set(
    app: AppHandle,
    state: State<'_, AppState>,
    action: String,
    combo: String,
) -> Result<HotkeyView> {
    let action = known_action(&action)?;
    let combo = in_tx(&state, |tx| hotkeys::rebind(&app, tx, action, &combo))?;
    Ok(view(&app, action, combo))
}

#[tauri::command]
pub fn hotkeys_reset(app: AppHandle, state: State<'_, AppState>, action: String) -> Result<HotkeyView> {
    let action = known_action(&action)?;
    let combo = in_tx(&state, |tx| {
        hotkeys::rebind(&app, tx, action, hotkeys::DEFAULT_SHOW)
    })?;
    Ok(view(&app, action, combo))
}

#[tauri::command]
pub fn autostart_get(app: AppHandle) -> Result<bool> {
    app.autolaunch()
        .is_enabled()
        .map_err(|e| Error::msg(e.to_string()))
}

#[tauri::command]
pub fn autostart_set(app: AppHandle, enabled: bool) -> Result<bool> {
    let mgr = app.autolaunch();
    if enabled {
        mgr.enable().map_err(|e| Error::msg(e.to_string()))?;
    } else {
        mgr.disable().map_err(|e| Error::msg(e.to_string()))?;
    }
    Ok(enabled)
}
