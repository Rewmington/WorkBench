use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::Serialize;
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::sync::lww::{touch, Stamp};
use crate::time::Now;

/// 速达条目。`secret` 只在显式「查看备注」时解密返回 —— 列表接口一律不带，
/// 免得密文解开后顺着 IPC 日志或前端状态被留在不该在的地方。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    #[serde(flatten)]
    pub stamp: Stamp,
    pub kind: String,
    pub name: String,
    pub target: String,
    pub args: String,
    pub icon_path: Option<String>,
    pub zone_id: Option<String>,
    pub use_count: i64,
    pub last_launched_at: Option<i64>,
    pub note: String,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResourceDraft {
    pub kind: String,
    pub name: String,
    pub target: String,
    #[serde(default)]
    pub args: String,
    #[serde(default)]
    pub icon_path: Option<String>,
    #[serde(default)]
    pub zone_id: Option<String>,
    #[serde(default)]
    pub note: String,
    /// 明文进来、加密落盘；None 表示这次不设备注
    #[serde(default)]
    pub secret: Option<String>,
}

const COLS: &str = "id, rev, created_at, created_by, updated_at, updated_by, deleted_at, kind, \
     name, target, args, icon_path, zone_id, use_count, last_launched_at, note";

fn map_row(r: &Row) -> rusqlite::Result<Resource> {
    Ok(Resource {
        stamp: Stamp {
            id: r.get("id")?,
            rev: r.get("rev")?,
            created_at: r.get("created_at")?,
            created_by: r.get("created_by")?,
            updated_at: r.get("updated_at")?,
            updated_by: r.get("updated_by")?,
            deleted_at: r.get("deleted_at")?,
        },
        kind: r.get("kind")?,
        name: r.get("name")?,
        target: r.get("target")?,
        args: r.get("args")?,
        icon_path: r.get("icon_path")?,
        zone_id: r.get("zone_id")?,
        use_count: r.get("use_count")?,
        last_launched_at: r.get("last_launched_at")?,
        note: r.get("note")?,
    })
}

/// 分区内按「用得多的在前」，同次数按加入时间。PLAN §4 的「分区与常用」就是这个顺序。
pub fn list(conn: &Connection) -> Result<Vec<Resource>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM resources WHERE deleted_at IS NULL ORDER BY use_count DESC, created_at ASC"
    ))?;
    let rows = stmt.query_map([], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn get(conn: &Connection, id: &str) -> Result<Option<Resource>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM resources WHERE id = ?1 AND deleted_at IS NULL"
    ))?;
    let found = stmt
        .query_map(params![id], map_row)?
        .next()
        .and_then(|r| r.ok());
    Ok(found)
}

pub fn create(
    conn: &Connection,
    now: &dyn Now,
    device: &str,
    draft: ResourceDraft,
) -> Result<Resource> {
    if !matches!(draft.kind.as_str(), "app" | "web" | "file") {
        return Err(Error::msg("速达条目类型只能是 app / web / file"));
    }
    if draft.name.trim().is_empty() || draft.target.trim().is_empty() {
        return Err(Error::msg("名称与目标都不能为空"));
    }
    // web 类型允许 URL；app / file 必须已经在磁盘上，否则磁贴一点就报错
    if draft.kind != "web" && !std::path::Path::new(&draft.target).exists() {
        return Err(Error::msg("目标文件不存在，可能被移动或删除了"));
    }

    let ms = now.ms();
    let stamp = Stamp::new(Uuid::now_v7().simple().to_string(), ms, device);
    let resource = Resource {
        stamp: stamp.clone(),
        kind: draft.kind,
        name: draft.name.trim().to_owned(),
        target: draft.target.trim().to_owned(),
        args: draft.args,
        icon_path: draft.icon_path,
        zone_id: draft.zone_id,
        use_count: 0,
        last_launched_at: None,
        note: draft.note,
    };

    conn.execute(
        "INSERT INTO resources (id, rev, created_at, created_by, updated_at, updated_by, kind, name,
                                target, args, icon_path, zone_id, use_count, last_launched_at, note)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,0,NULL,?13)",
        params![
            resource.stamp.id,
            resource.stamp.rev,
            resource.stamp.created_at,
            resource.stamp.created_by,
            resource.stamp.updated_at,
            resource.stamp.updated_by,
            resource.kind,
            resource.name,
            resource.target,
            resource.args,
            resource.icon_path,
            resource.zone_id,
            resource.note
        ],
    )?;

    if let Some(secret) = draft.secret.filter(|s| !s.trim().is_empty()) {
        set_secret(conn, &resource.stamp.id, &secret, ms, device)?;
    }
    Ok(resource)
}

/// 记一次启动：次数 +1、时间刷新。这是「常用」排序的唯一数据来源。
pub fn record_launch(conn: &Connection, now: &dyn Now, device: &str, id: &str) -> Result<()> {
    let ms = now.ms();
    let changed = conn.execute(
        "UPDATE resources SET use_count = use_count + 1, last_launched_at = ?2,
                rev = rev + 1, updated_at = ?2, updated_by = ?3
         WHERE id = ?1 AND deleted_at IS NULL",
        params![id, ms, device],
    )?;
    if changed == 0 {
        return Err(Error::msg("这个速达条目不存在或已被删除"));
    }
    Ok(())
}

pub fn rename(conn: &Connection, now: &dyn Now, device: &str, id: &str, name: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::msg("名称不能为空"));
    }
    let mut row = get(conn, id)?.ok_or_else(|| Error::msg("速达条目不存在"))?;
    row.name = name.to_owned();
    touch(&mut row.stamp, now.ms(), device);
    write_back(conn, &row)
}

pub fn move_to_zone(
    conn: &Connection,
    now: &dyn Now,
    device: &str,
    id: &str,
    zone_id: Option<String>,
) -> Result<()> {
    let mut row = get(conn, id)?.ok_or_else(|| Error::msg("速达条目不存在"))?;
    row.zone_id = zone_id;
    touch(&mut row.stamp, now.ms(), device);
    write_back(conn, &row)
}

pub fn soft_delete(conn: &Connection, now: &dyn Now, _device: &str, id: &str) -> Result<()> {
    let ms = now.ms();
    let changed = conn.execute(
        "UPDATE resources SET deleted_at = ?2, rev = rev + 1, updated_at = ?2, updated_by = ?3 WHERE id = ?1 AND deleted_at IS NULL",
        params![id, ms, _device],
    )?;
    if changed == 0 {
        return Err(Error::msg("速达条目不存在或已被删除"));
    }
    Ok(())
}

fn write_back(conn: &Connection, r: &Resource) -> Result<()> {
    conn.execute(
        "UPDATE resources SET rev=?2, updated_at=?3, updated_by=?4, name=?5, target=?6, args=?7,
            icon_path=?8, zone_id=?9, note=?10, deleted_at=?11 WHERE id=?1",
        params![
            r.stamp.id,
            r.stamp.rev,
            r.stamp.updated_at,
            r.stamp.updated_by,
            r.name,
            r.target,
            r.args,
            r.icon_path,
            r.zone_id,
            r.note,
            r.stamp.deleted_at
        ],
    )?;
    Ok(())
}

/// 加密备注。密文格式 `v1:<key_id 8 hex>:<base64(nonce|ct|tag)>` ——
/// 带 key_id 是为了跨机恢复后能明确说「这台机器解不开」，而不是显示乱码。
pub fn set_secret(
    conn: &Connection,
    id: &str,
    plain: &str,
    now_ms: i64,
    device: &str,
) -> Result<()> {
    let cipher = crate::secrets::encrypt(plain)?;
    conn.execute(
        "UPDATE resources SET secret_cipher = ?2, rev = rev + 1, updated_at = ?3, updated_by = ?4 WHERE id = ?1",
        params![id, cipher, now_ms, device],
    )?;
    Ok(())
}

pub fn reveal_secret(conn: &Connection, id: &str) -> Result<Option<String>> {
    let cipher: Option<String> = conn
        .query_row(
            "SELECT secret_cipher FROM resources WHERE id = ?1 AND deleted_at IS NULL",
            params![id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    match cipher {
        None => Ok(None),
        Some(text) => crate::secrets::decrypt(&text).map(Some),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Zone {
    pub id: String,
    pub name: String,
    pub sort: f64,
    pub item_count: i64,
}

pub fn list_zones(conn: &Connection) -> Result<Vec<Zone>> {
    let mut stmt = conn.prepare(
        "SELECT z.id, z.name, z.sort,
                (SELECT count(*) FROM resources r WHERE r.zone_id = z.id AND r.deleted_at IS NULL)
         FROM resource_zones z WHERE z.deleted_at IS NULL ORDER BY z.sort ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Zone {
            id: r.get(0)?,
            name: r.get(1)?,
            sort: r.get(2)?,
            item_count: r.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

pub fn add_zone(conn: &Connection, now: &dyn Now, device: &str, name: &str) -> Result<Zone> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::msg("分区名不能为空"));
    }
    let ms = now.ms();
    let id = Uuid::now_v7().simple().to_string();
    let sort: f64 = conn.query_row(
        "SELECT COALESCE(MAX(sort), 0) + 1024 FROM resource_zones WHERE deleted_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO resource_zones (id, rev, created_at, created_by, updated_at, updated_by, name, sort)
         VALUES (?1, 1, ?2, ?3, ?2, ?3, ?4, ?5)",
        params![id, ms, device, name, sort],
    )?;
    Ok(Zone {
        id,
        name: name.to_owned(),
        sort,
        item_count: 0,
    })
}

/// 删分区只删分区本身：里面的条目落回「未分区」（icon_path 之类的字段不动）。
/// 连带删掉用户攒了一年的快捷方式是不可逆的坏事。
pub fn delete_zone(conn: &Connection, now: &dyn Now, device: &str, id: &str) -> Result<()> {
    let ms = now.ms();
    conn.execute(
        "UPDATE resources SET zone_id = NULL, rev = rev + 1, updated_at = ?2, updated_by = ?3 WHERE zone_id = ?1 AND deleted_at IS NULL",
        params![id, ms, device],
    )?;
    conn.execute(
        "UPDATE resource_zones SET deleted_at = ?2, rev = rev + 1, updated_at = ?2, updated_by = ?3 WHERE id = ?1 AND deleted_at IS NULL",
        params![id, ms, device],
    )?;
    Ok(())
}
