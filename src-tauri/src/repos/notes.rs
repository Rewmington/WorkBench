//! 笔记的读写。三态要分清楚，混了就出不来「回收站可恢复」这条验收：
//!
//! - 活动：`deleted_at IS NULL AND trashed_at IS NULL`
//! - 回收站：`deleted_at IS NULL AND trashed_at IS NOT NULL` —— 还能还原
//! - 彻底删除：打 `deleted_at` 墓碑（v2 同步靠它传播删除）

use rusqlite::{params, Connection, Row};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::model::note::{Note, NoteDraft, NotePatch};
use crate::repos::tags;
use crate::sync::lww::{touch, Stamp};
use crate::time::Now;

const COLS: &str = "id, rev, created_at, created_by, updated_at, updated_by, deleted_at, \
     title, body_md, folder_id, pinned, trashed_at, created_day";

fn map_row(r: &Row) -> rusqlite::Result<Note> {
    Ok(Note {
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
        body_md: r.get("body_md")?,
        folder_id: r.get("folder_id")?,
        pinned: r.get::<_, i64>("pinned")? != 0,
        trashed_at: r.get("trashed_at")?,
        created_day: r.get("created_day")?,
        tags: Vec::new(),
    })
}

fn with_tags(conn: &Connection, rows: &mut [Note]) -> Result<()> {
    let ids: Vec<String> = rows.iter().map(|n| n.stamp.id.clone()).collect();
    let pairs = tags::tags_for(conn, tags::Owner::Note, &ids)?;
    for row in rows.iter_mut() {
        row.tags = pairs
            .iter()
            .filter(|(owner, _)| owner == &row.stamp.id)
            .map(|(_, name)| name.clone())
            .collect();
    }
    Ok(())
}

fn collect(conn: &Connection, sql: &str, p: &[&dyn rusqlite::ToSql]) -> Result<Vec<Note>> {
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map(p, map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    drop(stmt);
    with_tags(conn, &mut out)?;
    Ok(out)
}

/// `folder = Some(..)` 只看该文件夹直属的笔记（不含子文件夹的 —— 树在左侧已经表达了层级）。
///
/// 条件在 Rust 里拼，不玩 `folder_id IS ?1 OR ?1 IS NULL` 那套：传 None 时后半句恒真，
/// 「无过滤」和「只要没有文件夹的」会被混成一回事。
pub fn list_active(conn: &Connection, folder: Option<&str>) -> Result<Vec<Note>> {
    match folder {
        Some(id) => collect(
            conn,
            &format!(
                "SELECT {COLS} FROM notes
                 WHERE deleted_at IS NULL AND trashed_at IS NULL AND folder_id = ?1
                 ORDER BY pinned DESC, updated_at DESC"
            ),
            &[&id],
        ),
        None => collect(
            conn,
            &format!(
                "SELECT {COLS} FROM notes
                 WHERE deleted_at IS NULL AND trashed_at IS NULL
                 ORDER BY pinned DESC, updated_at DESC"
            ),
            &[],
        ),
    }
}

pub fn list_trashed(conn: &Connection) -> Result<Vec<Note>> {
    collect(
        conn,
        &format!(
            "SELECT {COLS} FROM notes
             WHERE deleted_at IS NULL AND trashed_at IS NOT NULL
             ORDER BY trashed_at DESC"
        ),
        &[],
    )
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<Note>> {
    let mut out = collect(
        conn,
        &format!("SELECT {COLS} FROM notes WHERE id = ?1 AND deleted_at IS NULL"),
        &[&id],
    )?;
    Ok(out.pop())
}

fn require(conn: &Connection, id: &str) -> Result<Note> {
    get(conn, id)?.ok_or_else(|| Error::msg("这条笔记不存在或已被彻底删除"))
}

fn write_row(conn: &Connection, n: &Note) -> Result<()> {
    conn.execute(
        "UPDATE notes SET rev=?2, updated_at=?3, updated_by=?4, title=?5, body_md=?6,
            folder_id=?7, pinned=?8, trashed_at=?9, deleted_at=?10
         WHERE id=?1",
        params![
            n.stamp.id,
            n.stamp.rev,
            n.stamp.updated_at,
            n.stamp.updated_by,
            n.title,
            n.body_md,
            n.folder_id,
            n.pinned as i64,
            n.trashed_at,
            n.stamp.deleted_at
        ],
    )?;
    Ok(())
}

pub fn create(conn: &Connection, now: &dyn Now, device: &str, draft: NoteDraft) -> Result<Note> {
    let title = draft.title.trim().to_owned();
    if title.is_empty() {
        return Err(Error::msg("笔记标题不能为空"));
    }
    let ms = now.ms();
    let stamp = Stamp::new(Uuid::now_v7().simple().to_string(), ms, device);
    conn.execute(
        "INSERT INTO notes (id, rev, created_at, created_by, updated_at, updated_by, deleted_at,
                            title, body_md, folder_id, pinned, trashed_at, created_day)
         VALUES (?1,?2,?3,?4,?5,?6,NULL,?7,?8,?9,0,NULL,?10)",
        params![
            stamp.id,
            stamp.rev,
            stamp.created_at,
            stamp.created_by,
            stamp.updated_at,
            stamp.updated_by,
            title,
            draft.body_md,
            draft.folder_id,
            i64::from(now.day_of(ms))
        ],
    )?;

    let names = normalize(&draft.tags);
    if !names.is_empty() {
        tags::set_tags(conn, now, device, tags::Owner::Note, &stamp.id, &names)?;
    }
    Ok(Note {
        stamp,
        title,
        body_md: draft.body_md,
        folder_id: draft.folder_id,
        pinned: false,
        trashed_at: None,
        created_day: Some(i64::from(now.day_of(ms))),
        tags: names,
    })
}

fn normalize(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for name in raw {
        let name = name.trim();
        if !name.is_empty() && !out.iter().any(|e| e == name) {
            out.push(name.to_owned());
        }
    }
    out
}

pub fn set_tags(conn: &Connection, now: &dyn Now, device: &str, id: &str, names: &[String]) -> Result<()> {
    require(conn, id)?;
    tags::set_tags(conn, now, device, tags::Owner::Note, id, &normalize(names))
}

pub fn update(conn: &Connection, now: &dyn Now, device: &str, id: &str, patch: NotePatch) -> Result<Note> {
    let mut note = require(conn, id)?;
    if let Some(title) = patch.title {
        let title = title.trim().to_owned();
        if title.is_empty() {
            return Err(Error::msg("笔记标题不能为空"));
        }
        note.title = title;
    }
    if let Some(body) = patch.body_md {
        note.body_md = body;
    }
    if let Some(folder_id) = patch.folder_id {
        note.folder_id = folder_id;
    }
    if let Some(pinned) = patch.pinned {
        note.pinned = pinned;
    }
    touch(&mut note.stamp, now.ms(), device);
    write_row(conn, &note)?;
    Ok(note)
}

/// 进回收站：只写 `trashed_at`，正文与标签都留着，所以能原样还原。
pub fn trash(conn: &Connection, now: &dyn Now, device: &str, id: &str) -> Result<Note> {
    let mut note = require(conn, id)?;
    note.trashed_at = Some(now.ms());
    touch(&mut note.stamp, now.ms(), device);
    write_row(conn, &note)?;
    Ok(note)
}

pub fn restore(conn: &Connection, now: &dyn Now, device: &str, id: &str) -> Result<Note> {
    let mut note = require(conn, id)?;
    if note.trashed_at.is_none() {
        return Ok(note);
    }
    note.trashed_at = None;
    touch(&mut note.stamp, now.ms(), device);
    write_row(conn, &note)?;
    Ok(note)
}

/// 彻底删除 = 打墓碑。**不**用 `DELETE FROM`：v2 手机端要能收到这条删除，
/// 物理删掉就没有东西可传播了（PLAN §5.1）。
pub fn purge(conn: &Connection, now: &dyn Now, device: &str, id: &str) -> Result<()> {
    let mut note = require(conn, id)?;
    note.stamp.deleted_at = Some(now.ms());
    touch(&mut note.stamp, now.ms(), device);
    write_row(conn, &note)?;
    Ok(())
}

/// 删文件夹时调用：把里面的笔记挪到「无文件夹」那一层，**不连带删笔记**。
/// 用户删的是组织结构，不是内容 —— 这两件事在数据上必须可分。
pub fn orphan_folder_contents(conn: &Connection, now: &dyn Now, device: &str, folder_id: &str) -> Result<usize> {
    let mut stmt = conn.prepare("SELECT id FROM notes WHERE folder_id = ?1 AND deleted_at IS NULL")?;
    let ids: Vec<String> = stmt
        .query_map(params![folder_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    drop(stmt);

    for id in &ids {
        if let Some(mut note) = get(conn, id)? {
            note.folder_id = None;
            touch(&mut note.stamp, now.ms(), device);
            write_row(conn, &note)?;
        }
    }
    Ok(ids.len())
}
