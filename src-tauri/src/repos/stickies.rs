//! 便签：一个槽位一条活记录，写空内容不删行。
//!
//! 为什么留着空行：清空正文是「我此刻没写字」，不是「这个槽位不存在」。
//! 删掉会让下次打字生成新 id、rev 从头计，同步端看到的是一次删除加一次新建，
//! 而它其实只是一次编辑。

use rusqlite::{params, Connection, Row};
use uuid::Uuid;

use crate::error::Result;
use crate::model::Sticky;
use crate::sync::lww::{touch, Stamp};
use crate::time::Now;

const COLS: &str = "id, rev, created_at, created_by, updated_at, updated_by, deleted_at, slot, content";

fn map_row(r: &Row) -> rusqlite::Result<Sticky> {
    Ok(Sticky {
        stamp: Stamp {
            id: r.get("id")?,
            rev: r.get("rev")?,
            created_at: r.get("created_at")?,
            created_by: r.get("created_by")?,
            updated_at: r.get("updated_at")?,
            updated_by: r.get("updated_by")?,
            deleted_at: r.get("deleted_at")?,
        },
        slot: r.get("slot")?,
        content: r.get("content")?,
    })
}

/// 按槽位排好序返回。首屏只画两个槽，但一次取全比按槽位查两遍便宜且不会漏。
pub fn list(conn: &Connection) -> Result<Vec<Sticky>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM stickies WHERE deleted_at IS NULL ORDER BY slot ASC"
    ))?;
    let rows = stmt.query_map([], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

fn find_slot(conn: &Connection, slot: i64) -> Result<Option<Sticky>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM stickies WHERE slot = ?1 AND deleted_at IS NULL"
    ))?;
    let found = stmt
        .query_map(params![slot], map_row)?
        .next()
        .transpose()?;
    Ok(found)
}

/// 写入某个槽位。前端 600 ms 防抖，所以这个函数会被频繁调用 —— 它必须是幂等的 upsert。
pub fn save(conn: &Connection, now: &dyn Now, device: &str, slot: i64, content: &str) -> Result<Sticky> {
    let ms = now.ms();
    if let Some(mut existing) = find_slot(conn, slot)? {
        // 正文没变就不推 rev：防抖多发一次不该在「最后写入」上留下痕迹
        if existing.content == content {
            return Ok(existing);
        }
        existing.content = content.to_owned();
        touch(&mut existing.stamp, ms, device);
        conn.execute(
            "UPDATE stickies SET rev=?2, updated_at=?3, updated_by=?4, content=?5 WHERE id=?1",
            params![existing.stamp.id, existing.stamp.rev, ms, device, existing.content],
        )?;
        return Ok(existing);
    }

    let stamp = Stamp::new(Uuid::now_v7().simple().to_string(), ms, device);
    conn.execute(
        "INSERT INTO stickies (id, rev, created_at, created_by, updated_at, updated_by, deleted_at, slot, content)
         VALUES (?1,?2,?3,?4,?5,?6,NULL,?7,?8)",
        params![
            stamp.id,
            stamp.rev,
            stamp.created_at,
            stamp.created_by,
            stamp.updated_at,
            stamp.updated_by,
            slot,
            content
        ],
    )?;
    Ok(Sticky { stamp, slot, content: content.to_owned() })
}

/// 清空一个槽位（连行一起软删）。与「正文写空」是两件事，所以单独一个命令。
pub fn clear(conn: &Connection, now: &dyn Now, device: &str, slot: i64) -> Result<bool> {
    let Some(existing) = find_slot(conn, slot)? else { return Ok(false) };
    conn.execute(
        "UPDATE stickies SET rev=?2, updated_at=?3, updated_by=?4, deleted_at=?5 WHERE id=?1",
        params![existing.stamp.id, existing.stamp.rev + 1, now.ms(), device, now.ms()],
    )?;
    Ok(true)
}
