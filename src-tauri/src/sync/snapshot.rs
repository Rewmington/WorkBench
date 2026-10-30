//! 一次同步在两台机器之间传的那份「全量表快照」。
//!
//! 明文形状（gzip 之前）：
//! ```json
//! {"ver":1,"device":"pc-…","sealed_at":1730000000000,
//!  "tables":{"todos":[{列名:值,…},…] ,"classes":[…]}}
//! ```
//! 每一行是**列名 → 值**的对象，不是按位置对齐的数组：两端版本不可能永远一致，
//! 多一列少一列时数组会错一位静默把数据写歪，对象则看得见「对面没有这一列」。
//! 键用 DB 里的蛇形列名原样，与手机 SQLite 的建表列名一一对上，中间不再翻译一次。
//!
//! 快照里没有 `dirty`：它的意思是「这台机器还没把它传出去」，属于本机，
//! 传给对面只会让对面以为那条在自己这边也没传（PLAN-MOBILE §3.4）。
//!
//! 明文进 gzip，gzip 进 `backup::crypto` 的信封，信封 PUT 成 `device-<id>.enc`（§3.2）。

use std::io::{Read, Write};

use rusqlite::{types::ValueRef, Connection};
use serde_json::{Map, Value};

use crate::error::{Error, Result};

pub const VER: u8 = 1;

/// 只在本地有意义、绝不上线的列。
pub const LOCAL_ONLY: &[&str] = &["dirty"];

/// PC 发布哪几张表：`todos` 对等，`classes` 单向 PC→手机（§3.1）。
pub const PC_TABLES: &[&str] = &["todos", "classes"];

/// 快照里允许跟表的行一起走的那几条 settings：键名就是 settings 里的原键。
///
/// 为什么需要它：手机要用 `timetable.week1Monday` 算「今天第几周」、用
/// `timetable.term` 过滤旧学期，而 `settings` 那段注释说的是「每台机器自己的 UX」，
/// 不同步。这两条不是 UX 偏好，是**解释 classes 表的基准**——少了它们，
/// 手机拿到课表也算不出这周上什么（PLAN-MOBILE §3.2 补记）。
///
/// 白名单而不是整张 settings 表：主题、快捷键、路径同步过去是错的。
pub const META_KEYS: &[&str] = &["timetable.term", "timetable.week1Monday"];

/// 一台设备发到云上的那份快照。
///
/// `tables` 用 `BTreeMap`：列目录顺序稳定，同一份库两次导出的明文字节才一致，
/// 「同内容两次 seal 仍然不同」这件事才只由 salt/nonce 解释（见 backup::crypto 的同款测试）。
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub device: String,
    pub sealed_at: i64,
    pub tables: std::collections::BTreeMap<String, Vec<Map<String, Value>>>,
    /// 解释 `classes` 的基准（见 [`META_KEYS`]），值都是字符串。
    pub meta: std::collections::BTreeMap<String, String>,
}

impl Snapshot {
    pub fn rows(&self, table: &str) -> &[Map<String, Value>] {
        self.tables.get(table).map(Vec::as_slice).unwrap_or(&[])
    }

    /// gzip 后的明文，等着进信封。
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let json = serde_json::to_vec(&serde_json::json!({
            "ver": VER,
            "device": self.device,
            "sealed_at": self.sealed_at,
            "tables": self.tables,
            "meta": self.meta,
        }))
        .map_err(|e| Error::msg(format!("快照编码失败：{e}")))?;
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(9));
        gz.write_all(&json)?;
        Ok(gz.finish()?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Snapshot> {
        let mut json = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .read_to_end(&mut json)
            .map_err(|_| Error::msg("快照解不开 gzip：传输出来的不是 Anvil 的同步文件"))?;
        let root: Value = serde_json::from_slice(&json)
            .map_err(|_| Error::msg("快照不是合法 JSON：文件被改过或版本不认识"))?;
        let ver = root
            .get("ver")
            .and_then(Value::as_i64)
            .ok_or_else(|| Error::msg("快照没有版本号"))?;
        if ver != i64::from(VER) {
            return Err(Error::msg(format!(
                "快照版本 {ver} 这台机器读不懂（当前写的是 v{VER}）"
            )));
        }
        let mut tables = std::collections::BTreeMap::new();
        if let Some(obj) = root.get("tables").and_then(Value::as_object) {
            for (name, rows) in obj {
                let rows = rows.as_array().ok_or_else(|| {
                    Error::msg(format!("快照里的 {name} 不是一行行的数组"))
                })?;
                let mut out = Vec::with_capacity(rows.len());
                for row in rows {
                    out.push(row.as_object().ok_or_else(|| {
                        Error::msg(format!("{name} 里有一行不是列名→值的对象"))
                    })?.clone());
                }
                tables.insert(name.clone(), out);
            }
        }
        // meta 只认白名单里的键，且只认字符串：旧版快照没有这个对象，
        // 当作空表处理，而不是把「没有」升级成「文件坏了」。
        let mut meta = std::collections::BTreeMap::new();
        if let Some(obj) = root.get("meta").and_then(Value::as_object) {
            for (key, value) in obj {
                if !META_KEYS.contains(&key.as_str()) {
                    continue;
                }
                if let Some(s) = value.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                    meta.insert(key.clone(), s.to_owned());
                }
            }
        }
        Ok(Snapshot {
            device: root
                .get("device")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            sealed_at: root.get("sealed_at").and_then(Value::as_i64).unwrap_or(0),
            tables,
            meta,
        })
    }
}

/// 表里实际存在、且允许上线的列。表名来自本模块的常量，列名来自库自己，
/// 所以拼进 SQL 的不可能是外部输入。
pub fn wire_columns(conn: &Connection, table: &str) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let cols = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if cols.is_empty() {
        return Err(Error::msg(format!("库里没有 {table} 这张表")));
    }
    Ok(cols
        .into_iter()
        .filter(|c| !LOCAL_ONLY.contains(&c.as_str()))
        .collect())
}

fn value_of(v: ValueRef<'_>) -> Result<Value> {
    Ok(match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::from(i),
        ValueRef::Real(f) => Value::from(f),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(_) => {
            return Err(Error::msg(
                "同步表里出现了二进制列：这条传输只走文本，别把它悄悄丢掉",
            ))
        }
    })
}

/// 一张表的全部行，**含墓碑**：传播删除靠的就是那些行（§3.2）。
fn dump_table(conn: &Connection, table: &str) -> Result<Vec<Map<String, Value>>> {
    let cols = wire_columns(conn, table)?;
    let list = cols
        .iter()
        .map(|c| format!("\"{c}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let mut stmt = conn.prepare(&format!("SELECT {list} FROM {table}"))?;
    let mut rows = Vec::new();
    let mut query = stmt.query([])?;
    while let Some(row) = query.next()? {
        let mut obj = Map::with_capacity(cols.len());
        for (i, name) in cols.iter().enumerate() {
            obj.insert(name.clone(), value_of(row.get_ref(i)?)?);
        }
        rows.push(obj);
    }
    Ok(rows)
}

/// 本机要发出去的那份快照。`tables` 由调用方给（PC 用 [`PC_TABLES`]）。
pub fn dump(conn: &Connection, tables: &[&str], device: &str, sealed_at: i64) -> Result<Snapshot> {
    let mut out = std::collections::BTreeMap::new();
    for table in tables {
        out.insert((*table).to_owned(), dump_table(conn, table)?);
    }
    Ok(Snapshot {
        device: device.to_owned(),
        sealed_at,
        tables: out,
        meta: meta_of(conn)?,
    })
}

/// [`META_KEYS`] 里那几条当前的字符串值；没设过或不是字符串就不发。
fn meta_of(conn: &Connection) -> Result<std::collections::BTreeMap<String, String>> {
    let mut meta = std::collections::BTreeMap::new();
    for key in META_KEYS {
        let value = crate::repos::settings::get_json(conn, key)?;
        if let Some(s) = value
            .as_ref()
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            meta.insert((*key).to_owned(), s.to_owned());
        }
    }
    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn conn() -> Connection {
        let mut c = db::open_in_memory().unwrap();
        db::migrations::migrate(&mut c).unwrap();
        c
    }

    #[test]
    fn round_trip_keeps_every_row_and_type() {
        let c = conn();
        let snap = dump(&c, PC_TABLES, "pc-a", 1_700_000_000_000).unwrap();
        let bytes = snap.to_bytes().unwrap();
        let back = Snapshot::from_bytes(&bytes).unwrap();
        assert_eq!(back.device, "pc-a");
        assert_eq!(back.sealed_at, 1_700_000_000_000);
        assert_eq!(back.tables.keys().len(), 2);
        assert!(back.rows("todos").is_empty(), "空库导出的就是空表");
    }

    #[test]
    fn tombstones_rid_in_the_dump_because_that_is_how_deletes_spread() {
        use crate::model::TodoDraft;
        use crate::repos::todos;
        use crate::time::FixedNow;

        let c = conn();
        let now = FixedNow(1_700_000_000_000);
        let live = todos::create(&c, &now, "pc-a", TodoDraft {
            title: "留着".into(),
            ..Default::default()
        })
        .unwrap();
        let gone = todos::create(&c, &now, "pc-a", TodoDraft {
            title: "删掉".into(),
            ..Default::default()
        })
        .unwrap();
        todos::soft_delete(&c, &now, "pc-a", &gone.stamp.id).unwrap();

        let snap = dump(&c, PC_TABLES, "pc-a", 1_700_000_000_000).unwrap();
        let rows = snap.rows("todos");
        assert_eq!(rows.len(), 2, "列表里只剩一条，快照里必须还是两条（另一条是墓碑）");
        let dead = rows
            .iter()
            .find(|r| r.get("id").and_then(Value::as_str) == Some(gone.stamp.id.as_str()))
            .expect("墓碑行要在");
        assert_ne!(
            dead.get("deleted_at").and_then(Value::as_i64),
            None,
            "墓碑靠 deleted_at 这一列传播"
        );
        assert!(rows
            .iter()
            .any(|r| r.get("id").and_then(Value::as_str) == Some(live.stamp.id.as_str())));
        for r in rows {
            assert!(!r.contains_key("dirty"), "本机状态不外传：{r:?}");
        }
    }

    #[test]
    fn bytes_survive_the_trip_into_an_empty_database() {
        // 手机第一次装上就是这形状：PC 那份文件 → 一台什么都没有的库。
        use crate::model::TodoDraft;
        use crate::repos::todos;
        use crate::time::FixedNow;

        let src = conn();
        let now = FixedNow(1_700_000_000_000);
        todos::create(&src, &now, "pc-a", TodoDraft {
            title: "从 PC 带来的".into(),
            note: "带备注".into(),
            ..Default::default()
        })
        .unwrap();
        let bytes = dump(&src, PC_TABLES, "pc-a", 1_700_000_000_000)
            .unwrap()
            .to_bytes()
            .unwrap();

        let dst = conn();
        let snap = Snapshot::from_bytes(&bytes).unwrap();
        let report = crate::sync::merge::apply(&dst, &snap, crate::sync::merge::PC_ACCEPTS).unwrap();
        assert_eq!(
            (report.applied, report.refused.as_slice()),
            (1, &["classes".to_owned()][..]),
            "todos 并进来了；classes 是 PC 自己的表，从对端那份里也不接"
        );
        let got = todos::list(&dst, crate::model::zones::INBOX).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].note, "带备注");
        assert_eq!(crate::sync::merge::pending(&dst).unwrap(), 0, "带来的不算本机欠的");
    }

    #[test]
    fn dirty_never_goes_on_the_wire() {
        let c = conn();
        let cols = wire_columns(&c, "todos").unwrap();
        assert!(!cols.contains(&"dirty".to_owned()), "本机状态不外传");
        assert!(cols.contains(&"updated_at".to_owned()));
    }

    /// 手机没有课的基准就算不出「这周上什么」。这两条必须随快照走，
    /// 而 settings 里别的键（主题、快捷键）一条都不许混进来。
    #[test]
    fn the_class_baseline_travels_with_the_rows_and_nothing_else_does() {
        use crate::repos::settings;
        use crate::repos::timetable;
        use crate::time::{FixedNow, Now};

        let c = conn();
        let now = FixedNow(1_700_000_000_000);
        timetable::set_week1(&c, &now, "pc-a", "2026-09-07").unwrap();
        settings::set_json(
            &c,
            timetable::KEY_TERM,
            &serde_json::json!("2026-2027-1"),
            now.ms(),
            "pc-a",
        )
        .unwrap();
        // 一条不该上线的 UX 偏好，用来证明白名单真的在过滤
        settings::set_json(
            &c,
            settings::KEY_THEME_MODE,
            &serde_json::json!("dark"),
            now.ms(),
            "pc-a",
        )
        .unwrap();

        let bytes = dump(&c, PC_TABLES, "pc-a", now.ms()).unwrap().to_bytes().unwrap();
        let back = Snapshot::from_bytes(&bytes).unwrap();
        assert_eq!(
            back.meta.get(timetable::KEY_WEEK1).map(String::as_str),
            Some("2026-09-07")
        );
        assert_eq!(
            back.meta.get(timetable::KEY_TERM).map(String::as_str),
            Some("2026-2027-1")
        );
        assert!(
            !back.meta.contains_key(settings::KEY_THEME_MODE),
            "主题这类本机 UX 不许跟着课表基准混上云：{:?}",
            back.meta
        );
    }

    /// 旧版快照没有 meta 对象；读它不能变成「文件坏了」。
    #[test]
    fn a_snapshot_without_meta_is_still_readable() {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(1));
        gz.write_all(br#"{"ver":1,"device":"old","sealed_at":1,"tables":{}}"#).unwrap();
        let snap = Snapshot::from_bytes(&gz.finish().unwrap()).unwrap();
        assert!(snap.meta.is_empty());
        assert_eq!(snap.device, "old");
    }

    /// Dart 侧 `Snapshot.toBytes()` 的产出，原样冻在这里。
    ///
    /// 重新产出：
    /// ```
    /// cd mobile && flutter test tool/emit_xlang_snapshot.dart
    /// ```
    ///
    /// 这条钉的是**手机→PC**这个方向：手机发的就是这份，PC 必须一字不差地读懂。
    /// 布局或键名漂了（比如 Dart 把蛇形列名改了、meta 换了键），这里立刻红，
    /// 而不是等到真机上「同步了但一条都没进来」。
    const XLANG_DART_SNAPSHOT: &str = "H4sIAAAAAAAACl1RwU7CQBD9l3fekm0xIPsPfoExpKWTsGG722y3SCVNOJhw8ezBxIOJMVyIBw/yQYj4F6YFbHFOM3kz7828mWNKFsJniGkqRwSBdGw0eTMFhoxCRfEwdBB+nx/DD7oMLowUZRBzOBObDOJ6DhlDYKY8HwyWphABw8hS6E4UvcExOOcNFBXnonkaNyPnqifo/0hMik4jOleKwUmnqmP2n6v9+3q/Wnw/3W83i93zCxi0cRX29brcfax+lg9fm7ftZr17XFZURlOLpy7jsPir8zZqKZG6LZtaaax0RX17KrWmGIIzpKEl7YaVQ4fGzFg3nFAB4fPgosMZ7oyulpI6MjM07tTaAQ96Pu+2TBub3EIMGEYmSevja53DQi0nypuSISEX1q+SVRYp6jiyCQQqXi/gQb/+WQPfEk38K6Nr8UMXv/S6PsryF27Y9j4xAgAA";

    #[test]
    fn a_dart_sealed_snapshot_is_readable_and_mergeable_here() {
        use base64::{engine::general_purpose::STANDARD as B64, Engine};

        let bytes = B64.decode(XLANG_DART_SNAPSHOT).unwrap();
        let snap = Snapshot::from_bytes(&bytes).unwrap();
        assert_eq!(snap.device, "phone-xl");
        assert_eq!(snap.sealed_at, 1_700_000_000_123);
        assert_eq!(
            snap.meta.get("timetable.week1Monday").map(String::as_str),
            Some("2026-08-31")
        );

        let c = conn();
        let report = crate::sync::merge::apply(&c, &snap, crate::sync::merge::PC_ACCEPTS).unwrap();
        assert_eq!(report.applied, 1);
        let title: String = c
            .query_row("SELECT title FROM todos WHERE id = 'xl-1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(title, "跨语言的一条");
        assert_eq!(crate::sync::merge::pending(&c).unwrap(), 0, "带来的不算本机欠的");
    }

    #[test]
    fn a_foreign_or_truncated_blob_is_refused_with_a_sentence() {
        for bad in [&[][..], b"not gzip at all".as_ref()] {
            let err = Snapshot::from_bytes(bad).unwrap_err();
            assert!(!err.to_string().is_empty(), "解不开也得说人话");
        }
        // 版本对不上时点名版本号，而不是报「JSON 坏了」
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(1));
        gz.write_all(br#"{"ver":9,"tables":{}}"#).unwrap();
        let err = Snapshot::from_bytes(&gz.finish().unwrap()).unwrap_err();
        assert!(err.to_string().contains("版本 9"), "{err}");
    }
}
