use rusqlite::{params, Connection};
use serde::Serialize;
use tauri::State;

use crate::error::Error;
use crate::repos::todos;
use crate::state::AppState;

/// 命令面板的一条数据命中。动作在前端注册表里，不混进这里 ——
/// 「一切能力都有 action id」那条规则要求动作与数据分开列。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Hit {
    pub kind: &'static str,
    pub id: String,
    pub label: String,
    pub sub: Option<String>,
}

fn search_resources(conn: &Connection, pattern: &str, limit: i64) -> Result<Vec<Hit>, Error> {
    let mut stmt = conn.prepare(
        "SELECT id, name, kind FROM resources
         WHERE deleted_at IS NULL AND (name LIKE ?1 OR target LIKE ?1)
         ORDER BY use_count DESC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![pattern, limit], |r| {
        Ok(Hit {
            kind: "resource",
            id: r.get(0)?,
            label: r.get(1)?,
            sub: Some(r.get(2)?),
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

#[tauri::command]
pub fn palette_search(state: State<'_, AppState>, query: String) -> Result<Vec<Hit>, Error> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let pattern = format!("%{trimmed}%");
    let conn = state.lock();

    let mut hits = Vec::new();
    for todo in todos::search(&conn, trimmed, 8)? {
        hits.push(Hit {
            kind: "todo",
            id: todo.stamp.id,
            label: todo.title,
            sub: if todo.note.trim().is_empty() {
                None
            } else {
                Some(todo.note.chars().take(40).collect())
            },
        });
    }
    hits.extend(search_resources(&conn, &pattern, 8)?);
    // 笔记的命中在 M6 接进来：那时才有 notes 表的可读内容
    Ok(hits)
}
