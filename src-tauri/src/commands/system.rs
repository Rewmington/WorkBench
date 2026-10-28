use tauri::State;

use crate::db;
use crate::error::Error;
use crate::state::{AppState, PathsView};

/// §2 原则 4「数据位置可感知」的落地：设置页原样显示绝对路径，
/// 而不是显示「已加密」三个字。启动时也打一遍日志，方便核对没落到 Roaming。
#[tauri::command]
pub fn paths(state: State<'_, AppState>) -> PathsView {
    let journal = db::journal_mode(&state.lock());
    state.paths.view(&journal)
}

#[tauri::command]
pub fn device_id(state: State<'_, AppState>) -> String {
    state.device_id.to_string()
}

/// 前端拦掉默认 contextmenu 后只传「点的是哪一类东西」，菜单本身是 Win32 原生件。
#[tauri::command]
pub fn context_menu(window: tauri::Window, target: String) -> Result<(), Error> {
    crate::ui::context::popup(window, crate::ui::context::MenuKind::parse(&target))
}
