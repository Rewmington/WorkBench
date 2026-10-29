//! 统计视图的一个只读命令。五张图一次出齐 —— 分开五个命令会出现
//! 「热力图有今天、柱状图没有」这种各自取时间造成的对不上。

use tauri::State;

use crate::error::Error;
use crate::repos::stats;
use crate::state::AppState;
use crate::time::SystemNow;

#[tauri::command]
pub fn stats_report(state: State<'_, AppState>) -> Result<stats::Report, Error> {
    let now = SystemNow;
    let conn = state.lock();
    stats::report(&conn, &now)
}
