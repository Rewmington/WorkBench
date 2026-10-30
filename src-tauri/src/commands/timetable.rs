//! 课表的命令面。
//!
//! 三条读 + 一条写。**导入是这页唯一的破坏性动作**，所以它先弹原生对话框、
//! 读完文件才拿数据库锁（对话框开着的时候不能把连接锁住 —— 备份 tick 与通知
//! ticker 都在等这把锁）。

use tauri::{AppHandle, State};

use crate::error::Error;
use crate::repos::timetable::{self, ClassRow, Report, StateView};
use crate::state::AppState;
use crate::time::SystemNow;

/// 首屏要的那一组数：学期、开学日、当前周、总周数、几门几段。
/// 全是本机 SQLite，不发网络（§6.1 那条"首屏不等外部源"）。
#[tauri::command]
pub fn timetable_state(state: State<'_, AppState>) -> Result<StateView, Error> {
    let conn = state.lock();
    timetable::state(&conn, &SystemNow)
}

/// 设第 1 周的周一（`yyyy-mm-dd`）。空串 = 清掉，回到"没设"并要求重填。
#[tauri::command]
pub fn timetable_set_week1(
    state: State<'_, AppState>,
    date: String,
) -> Result<StateView, Error> {
    let now = SystemNow;
    let conn = state.lock();
    timetable::set_week1(&conn, &now, &state.device_id, &date)
}

/// 某一天的课。`week=None` 用当前周；开学日没设时返回空列表而不是猜一周。
#[tauri::command]
pub fn timetable_week(
    state: State<'_, AppState>,
    week: Option<i32>,
) -> Result<Vec<ClassRow>, Error> {
    let conn = state.lock();
    Ok(timetable::week_rows(&conn, &SystemNow, week)?.1)
}

/// 选文件 → 读 → 导入。取消返回 `null`，不是错误。
#[tauri::command]
pub fn timetable_import(app: AppHandle, state: State<'_, AppState>) -> Result<Option<Report>, Error> {
    use tauri_plugin_dialog::DialogExt;

    let picked = app
        .dialog()
        .file()
        .set_title("选择教务导出的课表 CSV")
        .blocking_pick_file();
    let Some(path) = picked.as_ref().and_then(|p| p.as_path()) else {
        // picked 为 None 是取消；有值但取不出路径是网络位置，下面单独报
        if picked.is_some() {
            return Err(Error::msg("选中的路径读不出来（可能是网络位置或临时项）"));
        }
        return Ok(None);
    };
    let bytes = std::fs::read(path).map_err(|e| Error::msg(format!("读不了这份文件：{e}")))?;
    // 中文 Windows 上 Excel 的「CSV(逗号分隔)」写的是 GBK。不为一个每学期一次的动作
    // 引解码依赖，但也不能默默导成乱码 —— 给一句能照着做的提示。
    let text = String::from_utf8(bytes).map_err(|_| {
        Error::msg(
            "这份文件不是 UTF-8 编码。请在 Excel 里打开它，另存为「CSV UTF-8(逗号分隔) (*.csv)」再导一次。",
        )
    })?;

    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    Ok(Some(timetable::import(&conn, &now, &device, &text)?))
}
