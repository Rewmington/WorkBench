//! 首屏两个只读命令。都是同步 `fn`（碰 DB 的命令一律同步，见 `db/mod.rs` 的约定），
//! 且都不进任何 `.await`，所以不会跨等待持有连接锁。

use tauri::State;

use crate::error::Error;
use crate::repos::dashboard;
use crate::state::AppState;
use crate::suggest;
use crate::time::SystemNow;

/// 环最多铺几张卡。超过这个数圆环会挤成一叠，不如退化成列表（§8.8.1 的降级方向）。
const FOCUS_LIMIT: i64 = 8;

#[tauri::command]
pub fn dash_summary(state: State<'_, AppState>) -> Result<dashboard::Summary, Error> {
    let now = SystemNow;
    let conn = state.lock();
    dashboard::summary(&conn, &now, FOCUS_LIMIT)
}

#[tauri::command]
pub fn dash_suggestions(state: State<'_, AppState>) -> Result<Vec<suggest::Suggestion>, Error> {
    let now = SystemNow;
    let conn = state.lock();
    suggest::top(&conn, &now)
}
