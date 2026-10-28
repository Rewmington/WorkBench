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

/// 用系统默认程序打开 URL / 路径。走 Rust 侧 ShellExecute 而不是前端的 window.open：
/// 后者在 webview 里会开出一个新的应用窗口，那是最露馅的「像网页」行为之一。
#[tauri::command]
pub fn open_url(target: String) -> Result<(), Error> {
    crate::win32::launch::open_shell(&target)
}

/// 命令面板要能直达每一项能力（§2 原则 1），所以原生菜单里的动作也得有命令入口。
#[tauri::command]
pub fn open_data_dir(app: tauri::AppHandle) {
    crate::ui::menus::handle(&app, crate::ui::menus::ID_OPEN_DIR);
}
