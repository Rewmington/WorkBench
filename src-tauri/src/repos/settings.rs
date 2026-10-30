use crate::error::Result;
use rusqlite::{params, Connection};
use serde_json::Value;

/// `settings(key, value_json)`：主题、布局、快捷键、服务开关。
/// 坚果云应用密码不进这张表 —— 它走 Windows 凭据管理器（PLAN §6.2）。
///
/// 这张表按 PLAN §5.1 的约定属于**豁免同步列**的那一类：每台机器自己的 UX，
/// 同步过来的主题在别人机器上是错的。
pub fn get_json(conn: &Connection, key: &str) -> Result<Option<Value>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value_json FROM settings WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .ok();
    Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
}

pub fn set_json(
    conn: &Connection,
    key: &str,
    value: &Value,
    now_ms: i64,
    device: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO settings (key, value_json, updated_at, updated_by)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(key) DO UPDATE SET value_json = excluded.value_json,
                                        updated_at = excluded.updated_at,
                                        updated_by = excluded.updated_by",
        params![key, value.to_string(), now_ms, device],
    )?;
    Ok(())
}

pub fn all(conn: &Connection) -> Result<Vec<(String, Value)>> {
    let mut stmt = conn.prepare("SELECT key, value_json FROM settings ORDER BY key")?;
    let rows = stmt.query_map([], |r| {
        let key: String = r.get(0)?;
        let raw: String = r.get(1)?;
        Ok((key, serde_json::from_str(&raw).unwrap_or(Value::Null)))
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 读不到就用默认值，并且把默认值**写回去** —— 设置页要能看见当前生效值，
/// 而不是一片空白让人猜系统到底按什么跑。
pub fn get_or_default(
    conn: &Connection,
    key: &str,
    default: &Value,
    now_ms: i64,
    device: &str,
) -> Result<Value> {
    match get_json(conn, key)? {
        Some(v) => Ok(v),
        None => {
            set_json(conn, key, default, now_ms, device)?;
            Ok(default.clone())
        }
    }
}

pub const KEY_THEME_MODE: &str = "theme.mode";
pub const KEY_THEME_PRESET: &str = "theme.preset";
pub const KEY_THEME_ACCENT: &str = "theme.accent";
pub const KEY_MOTION_REDUCED: &str = "ui.reducedMotion";
pub const KEY_WIDGET_ORDER: &str = "ui.widgetOrder";
pub const KEY_BLOG_REPO_DIR: &str = "linked.blog.repoDir";
pub const KEY_MEDICINE_DB: &str = "linked.medicine.dbPath";
/// 药箱在坚果云上的同步目录。它是**机器无关**的配置，所以和 `backup.enabled` 一样
/// 跟着备份走 —— 换机还原后目录还在、应用密码不在（凭据管理器不出门），
/// 代码按「没配凭据 = 不启用」处理，不会出现半条配置。
pub const KEY_MEDICINE_DAV_DIR: &str = "linked.medicine.davDir";
/// 药箱那边用的坚果云注册邮箱。应用密码走凭据管理器，邮箱不是秘密所以留在 settings，
/// 才能解释那条凭据属于谁（与 `webdav.email` 同一个道理，两条互不干扰）。
pub const KEY_MEDICINE_EMAIL: &str = "linked.medicine.email";
pub const KEY_WDAV_EMAIL: &str = "webdav.email";
pub const KEY_BACKUP_ENABLED: &str = "backup.enabled";
pub const KEY_HOTKEYS: &str = "hotkeys";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_default_write_back() {
        let mut conn = crate::db::open_in_memory().unwrap();
        crate::db::migrations::migrate(&mut conn).unwrap();
        let v = get_or_default(&conn, KEY_MOTION_REDUCED, &Value::Bool(false), 1, "pc-a").unwrap();
        assert_eq!(v, Value::Bool(false));
        // 默认值确实落了库
        assert_eq!(all(&conn).unwrap().len(), 1);
        set_json(&conn, KEY_MOTION_REDUCED, &Value::Bool(true), 2, "pc-b").unwrap();
        assert_eq!(
            get_json(&conn, KEY_MOTION_REDUCED).unwrap(),
            Some(Value::Bool(true))
        );
    }
}
