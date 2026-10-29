use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::error::Result;
use crate::sync::lww::Stamp;
use crate::time::Now;

/// 标签。名字在活动行之间唯一（见 0001_init.sql 的 `name_live` 生成列），
/// 所以「先查同名、没有再建」是必须的，否则每打一次标签就长出一条新行。
pub fn ensure_tag(conn: &Connection, now: &dyn Now, device: &str, name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(crate::error::Error::msg("标签名不能为空"));
    }

    if let Some((id, _)) = conn
        .query_row(
            "SELECT id, rev FROM tags WHERE name = ?1 AND deleted_at IS NULL",
            params![name],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )
        .optional()?
    {
        return Ok(id);
    }

    let stamp = Stamp::new(Uuid::now_v7().simple().to_string(), now.ms(), device);
    conn.execute(
        "INSERT INTO tags (id, rev, created_at, created_by, updated_at, updated_by, name)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            stamp.id,
            stamp.rev,
            stamp.created_at,
            stamp.created_by,
            stamp.updated_at,
            stamp.updated_by,
            name
        ],
    )?;
    Ok(stamp.id)
}

/// 挂标签的两类东西。表名不能参数化，所以用枚举映射成固定字面量 ——
/// 取值只来自这个枚举，永远不是用户输入，拼接是安全的。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    Todo,
    Note,
}

impl Owner {
    fn link_table(self) -> &'static str {
        match self {
            Owner::Todo => "todo_tag_links",
            Owner::Note => "note_tag_links",
        }
    }

    fn owner_column(self) -> &'static str {
        match self {
            Owner::Todo => "todo_id",
            Owner::Note => "note_id",
        }
    }
}

/// 整组替换：前端每次提交当前应该挂的全部标签，这里算差集。
/// 比「逐个 add/remove」少一轮往返，也不会出现两个操作交错留下的脏链接。
///
/// 收 `&Connection`（不是 `&mut`）：rusqlite 的 `Transaction` 只 Deref 不 DerefMut，
/// 所以事务由调用方持有 —— 命令层 `guard.transaction()` 之后把 `&tx` 传进来。
pub fn set_tags(
    conn: &Connection,
    now: &dyn Now,
    device: &str,
    owner: Owner,
    owner_id: &str,
    names: &[String],
) -> Result<()> {
    let (link, col) = (owner.link_table(), owner.owner_column());
    let wanted: Vec<String> = {
        let mut ids = Vec::new();
        for name in names {
            let name = name.trim();
            if name.is_empty() {
                continue;
            }
            ids.push(ensure_tag(conn, now, device, name)?);
        }
        ids
    };

    let current: Vec<String> = {
        let mut stmt = conn.prepare(&format!(
            "SELECT tag_id FROM {link} WHERE {col} = ?1 AND deleted_at IS NULL"
        ))?;
        let rows = stmt.query_map(params![owner_id], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };

    for tag_id in current.iter().filter(|t| !wanted.contains(t)) {
        conn.execute(
            &format!(
                "UPDATE {link} SET deleted_at = ?3, rev = rev + 1, updated_at = ?3, updated_by = ?4
                 WHERE {col} = ?1 AND tag_id = ?2 AND deleted_at IS NULL"
            ),
            params![owner_id, tag_id, now.ms(), device],
        )?;
    }

    for tag_id in wanted.iter().filter(|t| !current.contains(t)) {
        conn.execute(
            &format!(
                "INSERT INTO {link} ({col}, tag_id, rev, created_at, created_by, updated_at, updated_by)
                 VALUES (?1, ?2, 1, ?3, ?4, ?3, ?4)
                 ON CONFLICT({col}, tag_id) DO UPDATE SET
                   deleted_at = NULL, rev = {link}.rev + 1,
                   updated_at = excluded.updated_at, updated_by = excluded.updated_by"
            ),
            params![owner_id, tag_id, now.ms(), device],
        )?;
    }

    Ok(())
}

/// 一次取多条记录的标签，避免前端 N+1。
pub fn tags_for(conn: &Connection, owner: Owner, ids: &[String]) -> Result<Vec<(String, String)>> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let (link, col) = (owner.link_table(), owner.owner_column());
    let placeholders = ids
        .iter()
        .enumerate()
        .map(|(i, _)| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT l.{col}, t.name FROM {link} l
         JOIN tags t ON t.id = l.tag_id
         WHERE l.deleted_at IS NULL AND t.deleted_at IS NULL AND l.{col} IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let args: Vec<&dyn rusqlite::ToSql> = ids.iter().map(|v| v as &dyn rusqlite::ToSql).collect();
    let rows = stmt.query_map(args.as_slice(), |r| Ok((r.get(0)?, r.get(1)?)))?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn set_todo_tags(
    conn: &Connection,
    now: &dyn Now,
    device: &str,
    todo_id: &str,
    names: &[String],
) -> Result<()> {
    set_tags(conn, now, device, Owner::Todo, todo_id, names)
}

pub fn tags_for_todos(conn: &Connection, todo_ids: &[String]) -> Result<Vec<(String, String)>> {
    tags_for(conn, Owner::Todo, todo_ids)
}
