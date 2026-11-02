//! 菜单项 id 与分发。托盘菜单和右键菜单共用这一套 id，出口也共用一个 ——
//! 「一切操作可从命令面板直达」（§2 原则 1）要求同一个动作只有一处实现。

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_opener::OpenerExt;

use crate::state::AppState;
use crate::ui::windows;

pub const ID_TOGGLE: &str = "toggle";
pub const ID_SETTINGS: &str = "settings";
pub const ID_QUIT: &str = "quit";
pub const ID_OPEN_DIR: &str = "open-data-dir";
pub const ID_RELOAD: &str = "reload";
pub const ID_COPY: &str = "copy-selection";

/// 前端把这些 id 当 action 用（命令面板里是同一批 id）。
pub const MENU_EVENT: &str = "menu://click";
/// 一行提示，不是对话框（§2 原则 5：失败与状态不弹窗打断）。
pub const HINT_EVENT: &str = "app://hint";

pub fn handle(app: &AppHandle, id: &str) {
    match id {
        ID_TOGGLE => windows::toggle(app),
        ID_SETTINGS => {
            windows::show_and_focus(app);
            let _ = app.emit(MENU_EVENT, ID_SETTINGS.to_owned());
        }
        ID_OPEN_DIR => {
            let path = app.state::<AppState>().paths.root.display().to_string();
            let _ = app.opener().open_path(path, None::<&str>);
        }
        ID_RELOAD => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.eval("location.reload()");
            }
        }
        ID_QUIT => {
            // 这里以前有一句 `w.eval("window.__anvilFlush && …")`，说的是"退之前把前端攒着的
            // 防抖写入冲刷掉"。那句承诺是**假的**：前端从来没定义过 `__anvilFlush`（全仓只有这一
            // 处提到它），而 eval 是 fire-and-forget，紧跟着的 exit 会抢在它那几个 IPC 之前杀进程。
            // 要做对得让「退出」走 menu://click 进前端、前端 await 完各视图的 flush 再调一条 quit
            // 命令，Rust 这边放兜底超时 —— 那是一整条新的跨语言协议，等作者点头（见 docs/AUDIT-TRIAGE.md
            // 批次 8 的"仍未做"）。当下的实际暴露：正在打字那一格里 ≤600ms 的内容。
            app.exit(0);
        }
        other => {
            let _ = app.emit(MENU_EVENT, other.to_owned());
        }
    }
}
