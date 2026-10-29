//! 笔记与文件夹命令。全部同步 `fn`：碰 DB 不跨 `.await`（`db/mod.rs` 的锁纪律）。

use tauri::State;

use crate::error::Error;
use crate::model::{Note, NoteDraft, NoteFolder, NotePatch};
use crate::repos::{note_folders, notes};
use crate::state::AppState;
use crate::time::SystemNow;

use super::in_tx;

#[tauri::command]
pub fn notes_list(state: State<'_, AppState>, folder_id: Option<String>) -> Result<Vec<Note>, Error> {
    let conn = state.lock();
    notes::list_active(&conn, folder_id.as_deref())
}

#[tauri::command]
pub fn notes_trashed(state: State<'_, AppState>) -> Result<Vec<Note>, Error> {
    let conn = state.lock();
    notes::list_trashed(&conn)
}

#[tauri::command]
pub fn notes_get(state: State<'_, AppState>, id: String) -> Result<Option<Note>, Error> {
    let conn = state.lock();
    notes::get(&conn, &id)
}

#[tauri::command]
pub fn notes_create(state: State<'_, AppState>, draft: NoteDraft) -> Result<Note, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    in_tx(&state, |tx| notes::create(tx, &now, &device, draft))
}

#[tauri::command]
pub fn notes_update(state: State<'_, AppState>, id: String, patch: NotePatch) -> Result<Note, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    notes::update(&conn, &now, &device, &id, patch)
}

#[tauri::command]
pub fn notes_set_tags(
    state: State<'_, AppState>,
    id: String,
    tags_input: Vec<String>,
) -> Result<(), Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    in_tx(&state, |tx| notes::set_tags(tx, &now, &device, &id, &tags_input))
}

#[tauri::command]
pub fn notes_trash(state: State<'_, AppState>, id: String) -> Result<Note, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    notes::trash(&conn, &now, &device, &id)
}

#[tauri::command]
pub fn notes_restore(state: State<'_, AppState>, id: String) -> Result<Note, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    notes::restore(&conn, &now, &device, &id)
}

/// 彻底删除。前端要先走原生 confirm —— 这一步之后只能靠坚果云的快照找回。
#[tauri::command]
pub fn notes_purge(state: State<'_, AppState>, id: String) -> Result<(), Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    notes::purge(&conn, &now, &device, &id)
}

#[tauri::command]
pub fn folders_list(state: State<'_, AppState>) -> Result<Vec<NoteFolder>, Error> {
    let conn = state.lock();
    note_folders::list(&conn)
}

#[tauri::command]
pub fn folder_add(
    state: State<'_, AppState>,
    name: String,
    parent_id: Option<String>,
) -> Result<NoteFolder, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    note_folders::create(&conn, &now, &device, &name, parent_id)
}

#[tauri::command]
pub fn folder_rename(state: State<'_, AppState>, id: String, name: String) -> Result<NoteFolder, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    note_folders::rename(&conn, &now, &device, &id, &name)
}

#[tauri::command]
pub fn folder_move(
    state: State<'_, AppState>,
    id: String,
    parent_id: Option<String>,
) -> Result<NoteFolder, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    note_folders::move_to(&conn, &now, &device, &id, parent_id)
}

/// 返回被挪走的笔记数（0 也要报，界面据此说「空目录已删除」还是「N 篇已挪到上一层」）。
#[tauri::command]
pub fn folder_delete(state: State<'_, AppState>, id: String) -> Result<usize, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    note_folders::delete(&conn, &now, &device, &id)
}
