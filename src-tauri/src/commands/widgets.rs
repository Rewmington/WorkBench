//! 「进行」视图四个部件里的三个数据面：便签、倒计时、系统资源。
//! （速达钉选走既有的 `resources_*`，不在这里重复一套读接口。）

use tauri::State;

use crate::error::Error;
use crate::model::{Countdown, CountdownDraft, Sticky, SysLoad};
use crate::repos::{countdowns, stickies};
use crate::state::AppState;
use crate::time::SystemNow;

// ---- 便签 ----

#[tauri::command]
pub fn stickies_list(state: State<'_, AppState>) -> Result<Vec<Sticky>, Error> {
    let conn = state.lock();
    stickies::list(&conn)
}

#[tauri::command]
pub fn sticky_save(state: State<'_, AppState>, slot: i64, content: String) -> Result<Sticky, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    stickies::save(&conn, &now, &device, slot, &content)
}

#[tauri::command]
pub fn sticky_clear(state: State<'_, AppState>, slot: i64) -> Result<bool, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    stickies::clear(&conn, &now, &device, slot)
}

// ---- 倒计时 ----

#[tauri::command]
pub fn countdowns_list(state: State<'_, AppState>) -> Result<Vec<Countdown>, Error> {
    let now = SystemNow;
    let conn = state.lock();
    countdowns::list(&conn, &now)
}

#[tauri::command]
pub fn countdown_add(state: State<'_, AppState>, draft: CountdownDraft) -> Result<Countdown, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    countdowns::create(&conn, &now, &device, draft)
}

/// 状态变化都是「读一行 → 改几个字段 → 写回一行」，单条 UPDATE 本身就是原子的，
/// 所以这里一律不套 `in_tx`。
#[tauri::command]
pub fn countdown_start(state: State<'_, AppState>, id: String) -> Result<Countdown, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    countdowns::start(&conn, &now, &device, &id)
}

#[tauri::command]
pub fn countdown_pause(state: State<'_, AppState>, id: String) -> Result<Countdown, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    countdowns::pause(&conn, &now, &device, &id)
}

#[tauri::command]
pub fn countdown_resume(state: State<'_, AppState>, id: String) -> Result<Countdown, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    countdowns::resume(&conn, &now, &device, &id)
}

#[tauri::command]
pub fn countdown_reset(state: State<'_, AppState>, id: String) -> Result<Countdown, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    countdowns::reset(&conn, &now, &device, &id)
}

#[tauri::command]
pub fn countdown_delete(state: State<'_, AppState>, id: String) -> Result<(), Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    countdowns::remove(&conn, &now, &device, &id)
}

// ---- 系统资源 ----

/// CPU 与内存。不落库、不进备份 —— 它是这台机器此刻的状态，不是用户的数据。
#[tauri::command]
pub fn sys_load() -> SysLoad {
    #[cfg(windows)]
    {
        crate::win32::sysinfo::sample_load()
    }
    #[cfg(not(windows))]
    {
        SysLoad { cpu_percent: None, mem_total: 0, mem_avail: 0 }
    }
}
