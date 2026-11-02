use tauri::State;

use crate::error::Error;
use crate::model::{Todo, TodoDraft, TodoPatch};
use crate::repos::{tags, todos};
use crate::state::AppState;
use crate::time::SystemNow;

use super::in_tx;

#[tauri::command]
pub fn todos_list(state: State<'_, AppState>, zone: String) -> Result<Vec<Todo>, Error> {
    let conn = state.lock();
    todos::list(&conn, &zone)
}

#[tauri::command]
pub fn todos_all(state: State<'_, AppState>) -> Result<Vec<Todo>, Error> {
    let conn = state.lock();
    todos::list_all(&conn)
}

#[tauri::command]
pub fn todos_get(state: State<'_, AppState>, id: String) -> Result<Option<Todo>, Error> {
    let conn = state.lock();
    todos::get(&conn, &id)
}

#[tauri::command]
pub fn todos_children(state: State<'_, AppState>, id: String) -> Result<Vec<Todo>, Error> {
    let conn = state.lock();
    todos::children(&conn, &id)
}

#[tauri::command]
pub fn todos_create(state: State<'_, AppState>, draft: TodoDraft) -> Result<Todo, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    // 建条目 + 挂标签要一起成或一起不成
    in_tx(&state, |tx| todos::create(tx, &now, &device, draft))
}

#[tauri::command]
pub fn todos_update(
    state: State<'_, AppState>,
    id: String,
    patch: TodoPatch,
) -> Result<Todo, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    // 改期会顺手动 `reminded_at` 与父项状态，两条语句必须一起成或一起不成
    in_tx(&state, |tx| todos::update(tx, &now, &device, &id, patch))
}

#[tauri::command]
pub fn todos_set_done(state: State<'_, AppState>, id: String, done: bool) -> Result<Todo, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    in_tx(&state, |tx| todos::set_done(tx, &now, &device, &id, done))
}

/// 删除返回**受影响 id 列表**（父删子随），撤销条按这份列表整体还原。
#[tauri::command]
pub fn todos_delete(state: State<'_, AppState>, id: String) -> Result<Vec<String>, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    // 级联是逐条 UPDATE：不在事务里，中途失败就留下「子项已删、父项还活着」那种
    // 谁也解释不了的库（撤销条拿到的 id 列表也会少几项）
    in_tx(&state, |tx| todos::soft_delete(tx, &now, &device, &id))
}

#[tauri::command]
pub fn todos_restore(state: State<'_, AppState>, ids: Vec<String>) -> Result<(), Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    // 与删除对称：还原一半比一条都没还原更难解释
    in_tx(&state, |tx| todos::restore(tx, &now, &device, &ids))
}

/// 把 `id` 放到 `prev_id` 与 `next_id` 之间（None 表示落在两端）。
#[tauri::command]
pub fn todos_reorder(
    state: State<'_, AppState>,
    id: String,
    prev_id: Option<String>,
    next_id: Option<String>,
) -> Result<(), Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    // 拖完之后 `rebalance` 可能重写整个区的 sort_key —— 一次不成不如不做
    in_tx(&state, |tx| {
        // 返回的那个新 sort_key 这里用不上（界面重取列表），但要留在事务里
        todos::reorder(
            tx,
            &now,
            &device,
            &id,
            prev_id.as_deref(),
            next_id.as_deref(),
        )?;
        Ok(())
    })
}

#[tauri::command]
pub fn todos_set_tags(
    state: State<'_, AppState>,
    id: String,
    tags_input: Vec<String>,
) -> Result<(), Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    in_tx(&state, |tx| {
        tags::set_todo_tags(tx, &now, &device, &id, &tags_input)
    })
}
