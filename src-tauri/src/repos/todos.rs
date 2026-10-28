use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::model::todo::{priority, zones, Todo, TodoDraft, TodoPatch};
use crate::repos::tags;
use crate::sync::lww::{touch, Stamp};
use crate::sync::sortkey;
use crate::time::Now;

const COLS: &str = "id, rev, created_at, created_by, updated_at, updated_by, deleted_at, \
     title, note, done_at, done_day, due_at, remind_at, priority, pinned, parent_id, \
     sort_key, zone, created_day, created_hour";

fn map_row(r: &Row) -> rusqlite::Result<Todo> {
    Ok(Todo {
        stamp: Stamp {
            id: r.get("id")?,
            rev: r.get("rev")?,
            created_at: r.get("created_at")?,
            created_by: r.get("created_by")?,
            updated_at: r.get("updated_at")?,
            updated_by: r.get("updated_by")?,
            deleted_at: r.get("deleted_at")?,
        },
        title: r.get("title")?,
        note: r.get("note")?,
        done_at: r.get("done_at")?,
        done_day: r.get("done_day")?,
        due_at: r.get("due_at")?,
        remind_at: r.get("remind_at")?,
        priority: r.get("priority")?,
        pinned: r.get::<_, i64>("pinned")? != 0,
        parent_id: r.get("parent_id")?,
        sort_key: r.get("sort_key")?,
        zone: r.get("zone")?,
        created_day: r.get("created_day")?,
        created_hour: r.get("created_hour")?,
        tags: Vec::new(),
    })
}

fn with_tags(conn: &Connection, rows: &mut [Todo]) -> Result<()> {
    let ids: Vec<String> = rows.iter().map(|t| t.stamp.id.clone()).collect();
    let pairs = tags::tags_for_todos(conn, &ids)?;
    for row in rows.iter_mut() {
        row.tags = pairs
            .iter()
            .filter(|(todo_id, _)| todo_id == &row.stamp.id)
            .map(|(_, name)| name.clone())
            .collect();
    }
    Ok(())
}

fn query_list(conn: &Connection, sql: &str, zone: &str) -> Result<Vec<Todo>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(params![zone], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    with_tags(conn, &mut out)?;
    Ok(out)
}

/// 所有分区的活动待办。日历视图与工作台首屏要的是「跨区的一天」，
/// 按区循环调用只会把同一个查询做四遍。
pub fn list_all(conn: &Connection) -> Result<Vec<Todo>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM todos WHERE deleted_at IS NULL ORDER BY pinned DESC, sort_key ASC"
    ))?;
    let rows = stmt.query_map([], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    with_tags(conn, &mut out)?;
    Ok(out)
}

/// 某区内未删除的全部待办（含已完成，「已完成」视图要用）。
pub fn list(conn: &Connection, zone: &str) -> Result<Vec<Todo>> {
    query_list(
        conn,
        &format!(
            "SELECT {COLS} FROM todos WHERE zone = ?1 AND deleted_at IS NULL
             ORDER BY pinned DESC, sort_key ASC"
        ),
        zone,
    )
}

/// 队列：未完成的。
pub fn list_undone(conn: &Connection, zone: &str) -> Result<Vec<Todo>> {
    query_list(
        conn,
        &format!(
            "SELECT {COLS} FROM todos WHERE zone = ?1 AND deleted_at IS NULL AND done_at IS NULL
             ORDER BY pinned DESC, sort_key ASC"
        ),
        zone,
    )
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<Todo>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM todos WHERE id = ?1 AND deleted_at IS NULL"
    ))?;
    let found = stmt
        .query_map(params![id], map_row)?
        .next()
        .transpose()
        .ok()
        .flatten();
    drop(stmt);
    let mut out = match found {
        Some(t) => vec![t],
        None => return Ok(None),
    };
    with_tags(conn, &mut out)?;
    Ok(Some(out.pop().expect("just checked")))
}

pub fn children(conn: &Connection, parent_id: &str) -> Result<Vec<Todo>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM todos WHERE parent_id = ?1 AND deleted_at IS NULL ORDER BY sort_key ASC"
    ))?;
    let rows = stmt.query_map(params![parent_id], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    with_tags(conn, &mut out)?;
    Ok(out)
}

fn next_sort_key(conn: &Connection, zone: &str) -> Result<f64> {
    let max: Option<f64> = conn
        .query_row(
            "SELECT MAX(sort_key) FROM todos WHERE zone = ?1 AND deleted_at IS NULL",
            params![zone],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(sortkey::between(max, None).unwrap_or(sortkey::GAP))
}

fn insert(conn: &Connection, stamp: &Stamp, todo: &Todo) -> Result<()> {
    conn.execute(
        "INSERT INTO todos (id, rev, created_at, created_by, updated_at, updated_by, deleted_at,
                            title, note, done_at, done_day, due_at, remind_at, priority, pinned,
                            parent_id, sort_key, zone, created_day, created_hour)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20)",
        params![
            stamp.id,
            stamp.rev,
            stamp.created_at,
            stamp.created_by,
            stamp.updated_at,
            stamp.updated_by,
            stamp.deleted_at,
            todo.title,
            todo.note,
            todo.done_at,
            todo.done_day,
            todo.due_at,
            todo.remind_at,
            todo.priority,
            todo.pinned as i64,
            todo.parent_id,
            todo.sort_key,
            todo.zone,
            todo.created_day,
            todo.created_hour
        ],
    )?;
    Ok(())
}

pub fn create(conn: &Connection, now: &dyn Now, device: &str, draft: TodoDraft) -> Result<Todo> {
    let title = draft.title.trim();
    if title.is_empty() {
        return Err(Error::msg("待办标题不能为空"));
    }
    let zone = draft.zone.unwrap_or_else(|| zones::INBOX.to_owned());
    let ms = now.ms();

    let sort_key = next_sort_key(conn, &zone)?;
    let stamp = Stamp::new(Uuid::now_v7().simple().to_string(), ms, device);
    let mut todo = Todo {
        stamp: stamp.clone(),
        title: title.to_owned(),
        note: draft.note,
        done_at: None,
        done_day: None,
        due_at: draft.due_at,
        remind_at: draft.remind_at,
        priority: draft.priority.unwrap_or(priority::NORMAL),
        pinned: false,
        parent_id: draft.parent_id,
        sort_key,
        zone,
        created_day: Some(i64::from(now.day_of(ms))),
        created_hour: Some(i64::from(now.hour_of(ms))),
        tags: Vec::new(),
    };
    insert(conn, &stamp, &todo)?;

    let names = normalize_tags(&draft.tags);
    if !names.is_empty() {
        tags::set_todo_tags(conn, now, device, &todo.stamp.id, &names)?;
    }
    todo.tags = names;
    Ok(todo)
}

/// 标签名去空白、去重（顺序保持用户输入的顺序）。
fn normalize_tags(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in raw {
        let name = name.trim();
        if name.is_empty() || out.iter().any(|existing| existing == name) {
            continue;
        }
        out.push(name.to_owned());
    }
    out
}

fn update_row(conn: &Connection, todo: &Todo) -> Result<()> {
    conn.execute(
        "UPDATE todos SET rev=?2, updated_at=?3, updated_by=?4, title=?5, note=?6, done_at=?7,
            done_day=?8, due_at=?9, remind_at=?10, priority=?11, pinned=?12, parent_id=?13,
            sort_key=?14, zone=?15, deleted_at=?16
         WHERE id=?1",
        params![
            todo.stamp.id,
            todo.stamp.rev,
            todo.stamp.updated_at,
            todo.stamp.updated_by,
            todo.title,
            todo.note,
            todo.done_at,
            todo.done_day,
            todo.due_at,
            todo.remind_at,
            todo.priority,
            todo.pinned as i64,
            todo.parent_id,
            todo.sort_key,
            todo.zone,
            todo.stamp.deleted_at
        ],
    )?;
    Ok(())
}

pub fn update(
    conn: &Connection,
    now: &dyn Now,
    device: &str,
    id: &str,
    patch: TodoPatch,
) -> Result<Todo> {
    let mut todo = get(conn, id)?.ok_or_else(|| Error::msg("这条待办不存在或已被删除"))?;

    if let Some(title) = patch.title {
        let title = title.trim().to_owned();
        if title.is_empty() {
            return Err(Error::msg("待办标题不能为空"));
        }
        todo.title = title;
    }
    if let Some(note) = patch.note {
        todo.note = note.unwrap_or_default();
    }
    if let Some(zone) = patch.zone {
        todo.zone = zone;
    }
    if let Some(priority) = patch.priority {
        todo.priority = priority;
    }
    if let Some(due_at) = patch.due_at {
        todo.due_at = due_at;
    }
    if let Some(remind_at) = patch.remind_at {
        todo.remind_at = remind_at;
    }
    if let Some(pinned) = patch.pinned {
        todo.pinned = pinned;
    }
    if let Some(parent_id) = patch.parent_id {
        todo.parent_id = parent_id;
    }

    touch(&mut todo.stamp, now.ms(), device);
    update_row(conn, &todo)?;
    Ok(todo)
}

/// 勾选 / 取消勾选。
///
/// 取消勾选**不清空** `done_at` / `done_day`，只递增 rev 并保留最后完成时间 ——
/// 清空会让历史统计少一格（PLAN §5.1 点名的那条语义）。
pub fn set_done(
    conn: &Connection,
    now: &dyn Now,
    device: &str,
    id: &str,
    done: bool,
) -> Result<Todo> {
    let mut todo = get(conn, id)?.ok_or_else(|| Error::msg("这条待办不存在或已被删除"))?;
    let ms = now.ms();

    if done {
        todo.done_at = Some(ms);
        todo.done_day = Some(i64::from(now.day_of(ms)));
    }

    touch(&mut todo.stamp, ms, device);
    update_row(conn, &todo)?;
    Ok(todo)
}

/// 软删。父项被删时子项跟着删，返回**受影响的 id 列表**给撤销条用 ——
/// 只还原父项会让子项悄悄留在墓碑里。
pub fn soft_delete(
    conn: &Connection,
    now: &dyn Now,
    device: &str,
    id: &str,
) -> Result<Vec<String>> {
    let ms = now.ms();
    let mut affected = Vec::new();
    let mut queue = vec![id.to_owned()];

    while let Some(current) = queue.pop() {
        let mut stmt = conn.prepare(
            "SELECT id FROM todos WHERE (id = ?1 OR parent_id = ?1) AND deleted_at IS NULL",
        )?;
        let live: Vec<String> = stmt
            .query_map(params![current], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(stmt);

        for hit in live {
            conn.execute(
                "UPDATE todos SET deleted_at = ?2, rev = rev + 1, updated_at = ?2, updated_by = ?3 WHERE id = ?1",
                params![hit, ms, device],
            )?;
            affected.push(hit.clone());
            queue.push(hit);
        }
    }

    if affected.is_empty() {
        return Err(Error::msg("这条待办不存在或已被删除"));
    }
    Ok(affected)
}

/// 撤销删除：把 `soft_delete` 报出来的那批 id 一起清掉墓碑，每个都 bump rev。
pub fn restore(conn: &Connection, now: &dyn Now, device: &str, ids: &[String]) -> Result<()> {
    let ms = now.ms();
    for id in ids {
        conn.execute(
            "UPDATE todos SET deleted_at = NULL, rev = rev + 1, updated_at = ?2, updated_by = ?3 WHERE id = ?1",
            params![id, ms, device],
        )?;
    }
    Ok(())
}

/// 拖拽排序：把 `id` 放到 `prev_id` 与 `next_id` 之间，取两者中点。
///
/// 参数是**邻居的 id 而不是键**：缝隙被浮点榨干时这里会整区重排，键全变了，
/// 前端传来的旧键值就成了过期数据 —— 重排后必须按 id 重新取一次。
pub fn reorder(
    conn: &Connection,
    now: &dyn Now,
    device: &str,
    id: &str,
    prev_id: Option<&str>,
    next_id: Option<&str>,
) -> Result<f64> {
    let zone: String = conn
        .query_row("SELECT zone FROM todos WHERE id = ?1", params![id], |r| {
            r.get(0)
        })
        .optional()?
        .ok_or_else(|| Error::msg("这条待办不存在"))?;

    let key_of = |neighbor: Option<&str>| -> Result<Option<f64>> {
        match neighbor {
            Some(n) => Ok(conn
                .query_row(
                    "SELECT sort_key FROM todos WHERE id = ?1",
                    params![n],
                    |r| r.get(0),
                )
                .optional()?
                .ok_or_else(|| Error::msg("排序邻居不存在或已被删除"))?),
            None => Ok(None),
        }
    };

    let mut a = key_of(prev_id)?;
    let mut b = key_of(next_id)?;
    let key = match sortkey::between(a, b) {
        Some(k) => k,
        None => {
            rebalance(conn, now, device, &zone)?;
            a = key_of(prev_id)?;
            b = key_of(next_id)?;
            sortkey::between(a, b).unwrap_or(sortkey::GAP)
        }
    };

    conn.execute(
        "UPDATE todos SET sort_key = ?2, rev = rev + 1, updated_at = ?3, updated_by = ?4 WHERE id = ?1",
        params![id, key, now.ms(), device],
    )?;
    Ok(key)
}

/// 整区重排：等距重发 sort_key，顺序保持。
pub fn rebalance(conn: &Connection, now: &dyn Now, device: &str, zone: &str) -> Result<()> {
    let ids: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT id FROM todos WHERE zone = ?1 AND deleted_at IS NULL ORDER BY pinned DESC, sort_key ASC",
        )?;
        let rows = stmt.query_map(params![zone], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let ms = now.ms();
    for (i, id) in ids.iter().enumerate() {
        conn.execute(
            "UPDATE todos SET sort_key = ?2, rev = rev + 1, updated_at = ?3, updated_by = ?4 WHERE id = ?1",
            params![id, (i as i64 + 1) as f64 * sortkey::GAP, ms, device],
        )?;
    }
    Ok(())
}

/// 命令面板的搜索。v1 用 LIKE 不上 FTS5：千余条量级，毫秒就回来。
pub fn search(conn: &Connection, query: &str, limit: i64) -> Result<Vec<Todo>> {
    let pattern = format!("%{}%", query.trim());
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM todos
         WHERE deleted_at IS NULL AND (title LIKE ?1 OR note LIKE ?1)
         ORDER BY pinned DESC, sort_key ASC LIMIT ?2"
    ))?;
    let rows = stmt.query_map(params![pattern, limit], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    with_tags(conn, &mut out)?;
    Ok(out)
}
