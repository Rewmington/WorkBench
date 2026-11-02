//! 笔记文件夹：一棵可嵌套的树，删父目录不删内容。

use rusqlite::{params, Connection, Row};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::model::note::NoteFolder;
use crate::repos::notes;
use crate::sync::lww::{touch, Stamp};
use crate::sync::sortkey;
use crate::time::Now;

const COLS: &str = "id, rev, created_at, created_by, updated_at, updated_by, deleted_at, name, parent_id, sort";

fn map_row(r: &Row) -> rusqlite::Result<NoteFolder> {
    Ok(NoteFolder {
        stamp: Stamp {
            id: r.get("id")?,
            rev: r.get("rev")?,
            created_at: r.get("created_at")?,
            created_by: r.get("created_by")?,
            updated_at: r.get("updated_at")?,
            updated_by: r.get("updated_by")?,
            deleted_at: r.get("deleted_at")?,
        },
        name: r.get("name")?,
        parent_id: r.get("parent_id")?,
        sort: r.get("sort")?,
        note_count: 0,
    })
}

pub fn list(conn: &Connection) -> Result<Vec<NoteFolder>> {
    let sql = format!(
        "SELECT {COLS} FROM note_folders WHERE deleted_at IS NULL ORDER BY sort ASC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    drop(stmt);

    // 计数一次算完，别让前端为每个文件夹发一条 COUNT
    let mut stmt = conn.prepare(
        "SELECT folder_id, count(*) FROM notes
         WHERE deleted_at IS NULL AND trashed_at IS NULL AND folder_id IS NOT NULL
         GROUP BY folder_id",
    )?;
    let counts: Vec<(String, i64)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    for folder in &mut out {
        folder.note_count = counts
            .iter()
            .find(|(id, _)| id == &folder.stamp.id)
            .map(|(_, n)| *n)
            .unwrap_or(0);
    }
    Ok(out)
}

fn get(conn: &Connection, id: &str) -> Result<Option<NoteFolder>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM note_folders WHERE id = ?1 AND deleted_at IS NULL"
    ))?;
    let found = stmt
        .query_map(params![id], map_row)?
        .next()
        .transpose()?;
    Ok(found)
}

fn require(conn: &Connection, id: &str) -> Result<NoteFolder> {
    get(conn, id)?.ok_or_else(|| Error::msg("这个文件夹不存在或已被删除"))
}

pub fn create(conn: &Connection, now: &dyn Now, device: &str, name: &str, parent_id: Option<String>) -> Result<NoteFolder> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(Error::msg("文件夹名不能为空"));
    }
    let ms = now.ms();
    let max: Option<f64> = conn
        .query_row(
            "SELECT max(sort) FROM note_folders WHERE deleted_at IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(None);
    let stamp = Stamp::new(Uuid::now_v7().simple().to_string(), ms, device);
    let sort = sortkey::between(max, None).unwrap_or(sortkey::GAP);
    conn.execute(
        "INSERT INTO note_folders (id, rev, created_at, created_by, updated_at, updated_by,
                                   deleted_at, name, parent_id, sort)
         VALUES (?1,?2,?3,?4,?5,?6,NULL,?7,?8,?9)",
        params![
            stamp.id,
            stamp.rev,
            stamp.created_at,
            stamp.created_by,
            stamp.updated_at,
            stamp.updated_by,
            name,
            parent_id,
            sort
        ],
    )?;
    Ok(NoteFolder {
        stamp,
        name,
        parent_id,
        sort,
        note_count: 0,
    })
}

pub fn rename(conn: &Connection, now: &dyn Now, device: &str, id: &str, name: &str) -> Result<NoteFolder> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(Error::msg("文件夹名不能为空"));
    }
    let mut folder = require(conn, id)?;
    folder.name = name;
    touch(&mut folder.stamp, now.ms(), device);
    conn.execute(
        "UPDATE note_folders SET rev=?2, updated_at=?3, updated_by=?4, name=?5 WHERE id=?1",
        params![folder.stamp.id, folder.stamp.rev, folder.stamp.updated_at, folder.stamp.updated_by, folder.name],
    )?;
    Ok(folder)
}

/// 挪进自己的后代会造出环，树一渲染就死循环 —— 先挡住。
pub fn move_to(conn: &Connection, now: &dyn Now, device: &str, id: &str, parent_id: Option<String>) -> Result<NoteFolder> {
    let mut folder = require(conn, id)?;
    if let Some(target) = &parent_id {
        if target == id || is_descendant(conn, target, id)? {
            return Err(Error::msg("不能把文件夹挪进它自己或它的子目录里"));
        }
    }
    folder.parent_id = parent_id;
    touch(&mut folder.stamp, now.ms(), device);
    conn.execute(
        "UPDATE note_folders SET rev=?2, updated_at=?3, updated_by=?4, parent_id=?5 WHERE id=?1",
        params![folder.stamp.id, folder.stamp.rev, folder.stamp.updated_at, folder.stamp.updated_by, folder.parent_id],
    )?;
    Ok(folder)
}

/// `candidate` 是不是 `ancestor` 的后代。递归深度按人手搭的文件夹树算，不会成问题。
fn is_descendant(conn: &Connection, candidate: &str, ancestor: &str) -> Result<bool> {
    let mut current = Some(candidate.to_owned());
    let mut hops = 0;
    while let Some(id) = current {
        if hops > 64 {
            return Err(Error::msg("文件夹层级超过 64 层，多半是数据里出现了环"));
        }
        let parent: Option<String> = conn
            .query_row(
                "SELECT parent_id FROM note_folders WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
                |r| r.get(0),
            )
            .unwrap_or(None);
        if parent.as_deref() == Some(ancestor) {
            return Ok(true);
        }
        current = parent;
        hops += 1;
    }
    Ok(false)
}

/// 删目录：先把里面的笔记挪到无文件夹那一层，再把子目录提到上一层，最后打墓碑。
/// 一步都不删内容 —— 用户删的是组织结构。
pub fn delete(conn: &Connection, now: &dyn Now, device: &str, id: &str) -> Result<usize> {
    let folder = require(conn, id)?;
    let moved = notes::orphan_folder_contents(conn, now, device, id)?;

    // 这一条挪动子目录的 UPDATE 不打 rev/touch，是**有意的**：
    // `note_folders` 不在 `sync::snapshot::PC_TABLES` 里，这张表从来不出门，
    // 所以少一个时间戳不会让任何设备看到旧结构。**若哪天把它放进 PC_TABLES，
    // 这一行必须先补上 touch**，否则别人的机器会永远看不见这次挪动。
    conn.execute(
        "UPDATE note_folders SET parent_id = ?2 WHERE parent_id = ?1 AND deleted_at IS NULL",
        params![id, folder.parent_id],
    )?;

    let mut f = folder;
    f.stamp.deleted_at = Some(now.ms());
    touch(&mut f.stamp, now.ms(), device);
    conn.execute(
        "UPDATE note_folders SET rev=?2, updated_at=?3, updated_by=?4, deleted_at=?5 WHERE id=?1",
        params![f.stamp.id, f.stamp.rev, f.stamp.updated_at, f.stamp.updated_by, f.stamp.deleted_at],
    )?;
    Ok(moved)
}
