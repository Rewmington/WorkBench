use crate::error::{Error, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

/// `settings(key, value_json)`：主题、布局、快捷键、服务开关。
/// 坚果云应用密码不进这张表 —— 它走 Windows 凭据管理器（PLAN §6.2）。
///
/// 这张表按 PLAN §5.1 的约定属于**豁免同步列**的那一类：每台机器自己的 UX，
/// 同步过来的主题在别人机器上是错的。
/// 读一条设置。**「没有这一行」和「读不出来」是两件事**：前者 `Ok(None)`，后者 `Err`。
///
/// 原来这里两层 `.ok()` 把 SQL 故障与 JSON 解析失败都折成 `None`，而 `get_or_default`
/// 见 `None` 就把默认值写回库 —— 于是一次瞬时故障会把用户设置静默重置掉。
/// 这一层是那条链的起点，所以必须在这里就把两种情况分开。
pub fn get_json(conn: &Connection, key: &str) -> Result<Option<Value>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value_json FROM settings WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .optional()?;
    match raw {
        None => Ok(None),
        Some(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| Error::msg(format!("设置 {key} 存的内容读不出：{e}"))),
    }
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
        // 这一条是「列表页」，一行读不出不该把整页变空白；但也不能装作读得出来，
        // 所以留 Null 并明确记一笔 —— 单项的严格判断在 `get_json`。
        let value = serde_json::from_str(&raw).unwrap_or_else(|e| {
            log::error!("设置 {key} 存的内容读不出：{e}");
            Value::Null
        });
        Ok((key, value))
    })?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    Ok(out)
}

/// 读不到就用默认值，并且把默认值**写回去** —— 设置页要能看见当前生效值，
/// 而不是一片空白让人猜系统到底按什么跑。
///
/// 只有「确实没有这一行」才会走到写回：读故障与解析失败由 `get_json` 抛出来，
/// 不会伪装成缺失去覆盖用户填过的东西。
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
/// 手机同步的状态（上次同步时间 / 最近一次失败 / 远端设备 listing）。
/// 唯一读写处是 `sync::relay::record` 与 `sync::relay::stored_state` ——
/// 界面与开机调度都从这一份念，不落第二份账（PLAN-MOBILE §6 M11b）。
pub const KEY_SYNC_STATE: &str = "sync.state";

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

    /// 「读不出来」绝不能被当成「没设过」—— 后者会触发写回默认值，
    /// 一次瞬时故障就能把用户填过的设置整批重置掉，而且界面看不出来。
    #[test]
    fn a_read_failure_is_not_mistaken_for_absence() {
        let mut conn = crate::db::open_in_memory().unwrap();
        crate::db::migrations::migrate(&mut conn).unwrap();
        set_json(&conn, KEY_THEME_MODE, &Value::String("dark".into()), 1, "pc-a").unwrap();

        // 表没了 —— 真机上对应库被占用/损坏那一类故障
        conn.execute_batch("DROP TABLE settings").unwrap();
        assert!(get_json(&conn, KEY_THEME_MODE).is_err(), "读失败要抛出来，不能给 None");
        assert!(
            get_or_default(&conn, KEY_THEME_MODE, &Value::String("light".into()), 2, "pc-a").is_err(),
            "读失败时更不该把默认值写回去"
        );
    }

    #[test]
    fn a_corrupt_row_is_reported_and_never_replaced_by_the_default() {
        let mut conn = crate::db::open_in_memory().unwrap();
        crate::db::migrations::migrate(&mut conn).unwrap();
        conn.execute(
            "INSERT INTO settings (key, value_json, updated_at, updated_by) VALUES (?1, ?2, 1, 'pc-a')",
            params![KEY_THEME_ACCENT, "{这不是 JSON"],
        )
        .unwrap();

        assert!(get_json(&conn, KEY_THEME_ACCENT).is_err());
        assert!(
            get_or_default(&conn, KEY_THEME_ACCENT, &Value::String("blue".into()), 2, "pc-a").is_err()
        );
        let raw: String = conn
            .query_row(
                "SELECT value_json FROM settings WHERE key = ?1",
                params![KEY_THEME_ACCENT],
                |r| r.get(0),
            )
            .unwrap();
        assert!(raw.contains("不是 JSON"), "坏内容该留在库里等着人看：{raw}");

        // 但列表页不能因为这一行整页空白：`all` 给它 Null，代价是必须留一条日志
        let listed = all(&conn).unwrap();
        assert_eq!(
            listed
                .iter()
                .find(|(k, _)| k == KEY_THEME_ACCENT)
                .expect("列表里该有这条")
                .1,
            Value::Null
        );
    }

    /// 真的没设过，才允许落默认值 —— 这条是上面两条想保住的正向行为。
    #[test]
    fn only_a_missing_key_falls_back_and_persists_the_default() {
        let mut conn = crate::db::open_in_memory().unwrap();
        crate::db::migrations::migrate(&mut conn).unwrap();
        assert_eq!(get_json(&conn, KEY_THEME_PRESET).unwrap(), None);
        let got = get_or_default(&conn, KEY_THEME_PRESET, &Value::String("苔".into()), 1, "pc-a").unwrap();
        assert_eq!(got, Value::String("苔".into()));
        assert_eq!(all(&conn).unwrap().len(), 1, "默认值确实写回了一次");
    }
}
