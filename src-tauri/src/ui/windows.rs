//! 窗口生命周期与显示状态。

use tauri::{AppHandle, CloseRequestApi, Emitter, Manager, Window};

use crate::state::AppState;

pub const MAIN: &str = "main";

pub fn show_and_focus(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.hide();
    }
}

pub fn toggle(app: &AppHandle) {
    match app.get_webview_window(MAIN) {
        Some(w) if w.is_visible().unwrap_or(false) => hide(app),
        Some(_) => show_and_focus(app),
        None => {}
    }
}

/// 关窗口 = 收进托盘。第一次这么做时给一行提示，之后不再打扰（不弹窗，§2 原则 5）。
pub fn on_close_requested(window: &Window, api: &CloseRequestApi) {
    api.prevent_close();
    window.hide().ok();

    let app = window.app_handle();
    let shown_before = {
        let state = app.state::<AppState>();
        state
            .tray_hint_shown
            .swap(true, std::sync::atomic::Ordering::Relaxed)
    };
    if !shown_before {
        let _ = app.emit(
            crate::ui::menus::HINT_EVENT,
            "已收进托盘，仍在后台运行。托盘菜单里选「退出」才会真的关掉。",
        );
    }
}
