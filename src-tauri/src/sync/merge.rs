//! 把对端那份快照**并进**本地表：行级 LWW、整行替换、墓碑跟着行走。
//!
//! 这里只有规则，没有网络 —— 传输是 M11b 的 `sync::relay`，
//! 所以 §3.3 那四条规则能在内存库里逐条钉死，不必连坚果云。
//!
//! 为什么不用 `INSERT OR REPLACE`：REPLACE 是「先删后插」，删的那一刻
//! `parent_id REFERENCES todos(id)` 会当场失败（子项还指着这一行），而本机的外键约束
//! 是开着的（`db::setup::configure`）。所以走「有则 UPDATE 全列、无则 INSERT」，
//! 效果同样是整行替换，但不经过删除这一跳。
//!
//! 「整行替换」落在实现上是对端发来的列逐个盖掉。两端导出的都是整张表的列
//! （见 `snapshot::wire_columns`），所以正常情况就是全列覆盖；字段级合并一次都不做 ——
//! 那需要逐字段比较写入者，而规则只有一句「哪一行的版本更新」。

use std::collections::{BTreeSet, HashMap};

use rusqlite::types::{Null, ToSql};
use rusqlite::{Connection, OptionalExtension};
use serde_json::{Map, Value};

use crate::error::{Error, Result};

use super::lww::is_newer;
use super::snapshot::{wire_columns, Snapshot};

/// PC 只接受对端传来的 `todos`。课表是单向的（§3.1：手机对课表只读，
/// 整学期替换只发生在 PC），所以手机那份里若出现 `classes`，整张表**不并**，
/// 而不是"反正内容一样"。钉子钉在这里，不靠调用方记得过滤。
pub const PC_ACCEPTS: &[&str] = &["todos"];

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    /// 对端的版本更新，写进本地的行数（含墓碑行）
    pub applied: usize,
    /// 本地版本更新或打平，保留原样的行数
    pub kept: usize,
    /// 快照里有、但这台机器不接受的表名
    pub refused: Vec<String>,
}

/// 一行里用来裁决的三列。缺任何一列都说明这不是一行完整的同步行。
fn stamp_of(table: &str, row: &Map<String, Value>) -> Result<(String, i64, String)> {
    let id = row
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| Error::msg(format!("{table} 里有一行没有 id")))
        .map(str::to_owned)?;
    let updated_at = row
        .get("updated_at")
        .and_then(Value::as_i64)
        .ok_or_else(|| Error::msg(format!("{table} 的行 {id} 没有 updated_at，无法裁决新旧")))?;
    let updated_by = row
        .get("updated_by")
        .and_then(Value::as_str)
        .ok_or_else(|| Error::msg(format!("{table} 的行 {id} 没有 updated_by，平票时无法裁决")))?
        .to_owned();
    Ok((id, updated_at, updated_by))
}

/// SQLite 的列类型只决定 affinity；光看 JSON 形状不够。对端把整数列写成 `"123"`
/// 会被本地列悄悄转换成数字，读回来时又成了整数；这种"看起来成功、语义已经变了"的
/// 事故必须按声明类型拒绝，不能靠 SQLite 的隐式转换兜。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Affinity {
    Integer,
    Real,
    Text,
    Blob,
    Numeric,
}

fn affinity_of(declared: &str) -> Affinity {
    let declared = declared.to_ascii_uppercase();
    if declared.contains("INT") {
        Affinity::Integer
    } else if declared.contains("CHAR") || declared.contains("CLOB") || declared.contains("TEXT") {
        Affinity::Text
    } else if declared.contains("BLOB") || declared.is_empty() {
        Affinity::Blob
    } else if declared.contains("REAL") || declared.contains("FLOA") || declared.contains("DOUB") {
        Affinity::Real
    } else {
        Affinity::Numeric
    }
}

fn kind_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "空值",
        Value::Bool(_) => "布尔值",
        Value::Number(_) => "数字",
        Value::String(_) => "字符串",
        Value::Array(_) => "数组",
        Value::Object(_) => "对象",
    }
}

fn shape_matches(affinity: Affinity, v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Bool(_) => matches!(affinity, Affinity::Integer | Affinity::Real | Affinity::Numeric),
        Value::Number(n) => match affinity {
            Affinity::Integer => n
                .as_i64()
                .is_some()
                || n.as_f64().is_some_and(|f| f.is_finite() && f.fract() == 0.0),
            Affinity::Real | Affinity::Numeric => n.as_f64().is_some_and(f64::is_finite),
            Affinity::Text | Affinity::Blob => false,
        },
        Value::String(_) => matches!(affinity, Affinity::Text | Affinity::Blob),
        Value::Array(_) | Value::Object(_) => false,
    }
}

/// JSON 值 → SQL 值。数组/对象一律拒绝：这几张表里没有 JSON 列，真出现了就是
/// schema 与传输对不上，存成文本会让下次读回来的是带引号的字符串，那种错更难查。
fn to_sql(table: &str, id: &str, col: &str, declared: &str, v: &Value) -> Result<Box<dyn ToSql>> {
    let affinity = affinity_of(declared);
    if !shape_matches(affinity, v) {
        return Err(Error::msg(format!(
            "{table} 的行 {id} 列 {col} 本地声明是 {declared}，对端却发来{}；拒绝让 SQLite 猜一个转换",
            kind_name(v)
        )));
    }

    Ok(match v {
        Value::Null => Box::new(Null),
        Value::Bool(b) => Box::new(i64::from(*b)),
        Value::Number(n) => {
            // 整数列与浮点列（sort_key）都从这条路上过：先试 i64，试不出就是 f64
            if let Some(i) = n.as_i64() {
                Box::new(i)
            } else {
                Box::new(n.as_f64().ok_or_else(|| {
                    Error::msg(format!("{table} 的行 {id} 列 {col} 是个不是数的数字"))
                })?)
            }
        }
        Value::String(s) => Box::new(s.clone()),
        other => {
            return Err(Error::msg(format!(
                "{table} 的行 {id} 列 {col} 是 {}，同步表里没有这种列，拒绝写入",
                if other.is_array() { "数组" } else { "对象" }
            )))
        }
    })
}

fn declared_types(conn: &Connection, table: &str) -> Result<HashMap<String, String>> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?.unwrap_or_default(),
        ))
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let (name, declared) = row?;
        out.insert(name, declared);
    }
    Ok(out)
}

fn apply_table(conn: &Connection, table: &str, rows: &[Map<String, Value>]) -> Result<(usize, usize)> {
    let local_cols = wire_columns(conn, table)?;
    let types = declared_types(conn, table)?;
    let local_set: BTreeSet<&str> = local_cols.iter().map(String::as_str).collect();

    // 一次读光现有版本戳，别每行查一次：全量合并本来就要跑几百上千行
    let mut current: HashMap<String, (i64, String)> = HashMap::new();
    {
        let mut stmt = conn.prepare(&format!("SELECT id, updated_at, updated_by FROM {table}"))?;
        let found = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in found {
            let (id, at, by) = row?;
            current.insert(id, (at, by));
        }
    }

    let mut applied = 0usize;
    let mut kept = 0usize;

    for row in rows {
        let (id, updated_at, updated_by) = stamp_of(table, row)?;

        if let Some((here_at, here_by)) = current.get(&id) {
            if !is_newer((updated_at, &updated_by), (*here_at, here_by)) {
                kept += 1;
                continue;
            }
            // 对端赢：把它发来的每一列都盖上去；dirty 置 0 —— 它已经在云上那份里了
            let sets: Vec<&String> = row
                .keys()
                .filter(|k| local_set.contains(k.as_str()) && k.as_str() != "id")
                .collect();
            if sets.is_empty() {
                return Err(Error::msg(format!("{table} 的行 {id} 除了 id 什么都没带")));
            }
            let sql = format!(
                "UPDATE {table} SET {}, dirty = 0 WHERE id = ?{}",
                sets.iter()
                    .map(|k| format!("\"{k}\" = ?"))
                    .collect::<Vec<_>>()
                    .join(", "),
                sets.len() + 1
            );
            let mut args: Vec<Box<dyn ToSql>> = Vec::with_capacity(sets.len() + 1);
            for k in &sets {
                args.push(to_sql(
                    table,
                    &id,
                    k,
                    types.get(k.as_str()).map(String::as_str).unwrap_or(""),
                    row.get(*k).expect("刚从 keys 里取的"),
                )?);
            }
            args.push(Box::new(id.clone()));
            let refs: Vec<&dyn ToSql> = args.iter().map(|a| a.as_ref()).collect();
            conn.execute(&sql, refs.as_slice())?;
            applied += 1;
        } else {
            // 本地没有这一行：对端发来的列原样插进去，没发的列由建表时的 DEFAULT 兜。
            // 若某个 NOT NULL 且无默认值的列没发，SQLite 直接报错 —— 宁可失败也别编一个值。
            let cols: Vec<&String> = row
                .keys()
                .filter(|k| local_set.contains(k.as_str()))
                .collect();
            if cols.is_empty() {
                return Err(Error::msg(format!("{table} 的行 {id} 一列可用都没有")));
            }
            let sql = format!(
                "INSERT INTO {table} ({}, dirty) VALUES ({}, 0)",
                cols.iter()
                    .map(|k| format!("\"{k}\""))
                    .collect::<Vec<_>>()
                    .join(", "),
                cols.iter().map(|_| "?").collect::<Vec<_>>().join(", ")
            );
            let mut args: Vec<Box<dyn ToSql>> = Vec::with_capacity(cols.len());
            for k in &cols {
                args.push(to_sql(
                    table,
                    &id,
                    k,
                    types.get(k.as_str()).map(String::as_str).unwrap_or(""),
                    row.get(*k).expect("刚从 keys 里取的"),
                )?);
            }
            let refs: Vec<&dyn ToSql> = args.iter().map(|a| a.as_ref()).collect();
            match conn.execute(&sql, refs.as_slice()) {
                Ok(_) => applied += 1,
                Err(e) => {
                    return Err(Error::msg(format!(
                        "{table} 插不进对端的行 {id}：{e}（多半是这一版没有它要求的某一列）"
                    )))
                }
            }
        }
    }

    Ok((applied, kept))
}

/// 并进本地。整批一个事务：一半生效一半失败的库，比整批没发生更难解释。
pub fn apply(conn: &Connection, snap: &Snapshot, accepts: &[&str]) -> Result<Report> {
    let mut report = Report::default();
    let tx = conn.unchecked_transaction()?;
    for (table, rows) in &snap.tables {
        if !accepts.contains(&table.as_str()) {
            report.refused.push(table.clone());
            continue;
        }
        let exists: Option<i64> = tx
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [table],
                |r| r.get(0),
            )
            .optional()?;
        if exists.is_none() {
            return Err(Error::msg(format!(
                "对端发来了 {table}，这台机器的库里没有这张表（版本差得太远）"
            )));
        }
        let (a, k) = apply_table(&tx, table, rows)?;
        report.applied += a;
        report.kept += k;
    }
    tx.commit()?;
    Ok(report)
}

/// PUT 成功之后清脏。**比的是行自己的 `updated_at`，不是本机 now**（§3.3 第 2 条）：
/// 序列化之后、响应回来之前新写的那一条必须留在 outbox 里，否则它会被下一次合并悄悄盖掉。
///
/// `sealed_at` 必须是那份快照自己的时间戳（`Snapshot::sealed_at`），调用方不许传 now。
pub fn clear_dirty(conn: &Connection, sealed_at: i64) -> Result<usize> {
    Ok(conn.execute(
        "UPDATE todos SET dirty = 0 WHERE dirty = 1 AND updated_at <= ?1",
        [sealed_at],
    )?)
}

/// 「N 条待上传」的唯一计数来源（§3.4）。别在界面或命令层里另写一条 SQL。
pub fn pending(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT count(*) FROM todos WHERE dirty = 1", [], |r| r.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use serde_json::json;

    fn conn() -> Connection {
        let mut c = db::open_in_memory().unwrap();
        db::migrations::migrate(&mut c).unwrap();
        c
    }

    /// 一行完整的 todos 同步行，形状与 `snapshot::dump` 发出去的一模一样。
    fn row(id: &str, at: i64, by: &str) -> Map<String, Value> {
        json!({"id": id, "rev": 1, "created_at": at, "created_by": by,
               "updated_at": at, "updated_by": by, "deleted_at": Value::Null,
               "title": "对端写的", "note": "", "done_at": Value::Null, "done_day": Value::Null,
               "due_at": Value::Null, "remind_at": Value::Null, "priority": 2, "pinned": 0,
               "parent_id": Value::Null, "sort_key": 1024.0, "zone": "inbox",
               "created_day": 20261030, "created_hour": 9, "completed": 0,
               "reminded_at": Value::Null})
        .as_object()
        .unwrap()
        .clone()
    }

    fn snap_of(tables: &[(&str, Vec<Map<String, Value>>)]) -> Snapshot {
        let mut m = std::collections::BTreeMap::new();
        for (name, rows) in tables {
            m.insert((*name).to_owned(), rows.clone());
        }
        Snapshot {
            device: "phone-a".into(),
            sealed_at: 500,
            tables: m,
            meta: std::collections::BTreeMap::new(),
        }
    }

    /// 本地已有的一行，由 `by` 在 `at` 写过、还没传出去（dirty=1）。
    fn seed(conn: &Connection, id: &str, at: i64, by: &str, title: &str) {
        conn.execute(
            "INSERT INTO todos (id, rev, created_at, created_by, updated_at, updated_by, title,
                                note, sort_key, zone, completed, dirty)
             VALUES (?1,1,?2,?3,?2,?3,?4,'本机原来的备注',1024,'inbox',0,1)",
            rusqlite::params![id, at, by, title],
        )
        .unwrap();
    }

    fn read(conn: &Connection, id: &str) -> (String, String, Option<i64>, i64) {
        conn.query_row(
            "SELECT title, note, deleted_at, dirty FROM todos WHERE id = ?1",
            rusqlite::params![id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap()
    }

    fn live_rows(conn: &Connection) -> i64 {
        conn.query_row("SELECT count(*) FROM todos", [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn newer_row_replaces_the_whole_row_not_field_by_field() {
        let c = conn();
        seed(&c, "t1", 100, "pc-a", "本机写的");

        let report = apply(&c, &snap_of(&[("todos", vec![row("t1", 200, "phone-a")])]), PC_ACCEPTS)
            .unwrap();
        assert_eq!(report.applied, 1);
        let (title, note, _, dirty) = read(&c, "t1");
        assert_eq!(title, "对端写的");
        // 对端那行的 note 是空串，本机原本是「本机原来的备注」：整行替换后必须换成对端那份。
        // 字段级合并才会把两条都留着，而 §3.3 第 3 条明确不做那种事。
        assert_eq!(note, "", "没带的值按对端那份走，不与本机旧值混");
        assert_eq!(dirty, 0, "对端传来的行已经在云上那份里了，不该再算待上传");
    }

    #[test]
    fn older_incoming_row_loses_and_keeps_local_pending() {
        let c = conn();
        seed(&c, "t1", 300, "pc-a", "本机刚改的");
        let report = apply(&c, &snap_of(&[("todos", vec![row("t1", 200, "phone-a")])]), PC_ACCEPTS)
            .unwrap();
        assert_eq!((report.applied, report.kept), (0, 1));
        let (title, _, _, dirty) = read(&c, "t1");
        assert_eq!((title.as_str(), dirty), ("本机刚改的", 1), "本机那条还没传出去");
    }

    #[test]
    fn same_clock_ties_are_resolved_by_device_id_the_same_way_everywhere() {
        // §7 第 3 条最后一句：两端同时改同一条，两台机器算出来的赢家必须是同一个。
        // 规则只有一条（比 id 字典序），所以这里两个方向各验一次而不是指望它巧合。
        let c = conn();
        seed(&c, "t1", 500, "bbb", "本机写");
        apply(&c, &snap_of(&[("todos", vec![row("t1", 500, "ddd")])]), PC_ACCEPTS).unwrap();
        assert_eq!(read(&c, "t1").0, "对端写的", "平票时 id 大的一方赢");

        let c2 = conn();
        seed(&c2, "t1", 500, "bbb", "本机写");
        let report = apply(&c2, &snap_of(&[("todos", vec![row("t1", 500, "aaa")])]), PC_ACCEPTS)
            .unwrap();
        assert_eq!((report.applied, report.kept), (0, 1), "id 小的一方在哪台机器上都赢不了");
        assert_eq!(read(&c2, "t1").0, "本机写");
    }

    #[test]
    fn tombstone_spreads_and_never_comes_back_on_a_second_merge() {
        let c = conn();
        seed(&c, "t1", 100, "pc-a", "要删的");
        let mut gone = row("t1", 200, "phone-a");
        gone.insert("deleted_at".to_owned(), json!(200));

        apply(&c, &snap_of(&[("todos", vec![gone.clone()])]), PC_ACCEPTS).unwrap();
        assert_eq!(read(&c, "t1").2, Some(200), "墓碑传播过来还是墓碑");

        // 第二趟：对端发来一份**更早**的、这一行还活着的快照（旧设备留下的旧文件）。
        // 让它复活等于承认「清掉墓碑就能靠旧快照找回数据」，§3.3 第 4 条不许这么干。
        apply(&c, &snap_of(&[("todos", vec![row("t1", 150, "phone-a")])]), PC_ACCEPTS).unwrap();
        assert_eq!(read(&c, "t1").2, Some(200), "旧快照不得复活已删的行");

        // 同一份墓碑再并一次是无事发生，而不是「又应用一次」
        let report = apply(&c, &snap_of(&[("todos", vec![gone])]), PC_ACCEPTS).unwrap();
        assert_eq!((report.applied, report.kept), (0, 1));
    }

    #[test]
    fn classes_from_a_peer_are_refused_wholesale() {
        let c = conn();
        seed(&c, "t1", 100, "pc-a", "唯一的一行");
        let mut cls = row("cls-1", 999, "phone-a");
        cls.insert("term".to_owned(), json!("2026-2027-1"));
        cls.insert("name".to_owned(), json!("手机编的课"));

        let before: i64 = c
            .query_row("SELECT count(*) FROM classes", [], |r| r.get(0))
            .unwrap();
        let report = apply(
            &c,
            &snap_of(&[("todos", vec![row("t1", 200, "phone-a")]), ("classes", vec![cls])]),
            PC_ACCEPTS,
        )
        .unwrap();
        assert_eq!(report.refused, vec!["classes".to_owned()]);
        let after: i64 = c
            .query_row("SELECT count(*) FROM classes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(before, after, "课表是单向的，手机那份里的 classes 一行都不许落地");
        assert_eq!(report.applied, 1, "todos 该照常并进来");
    }

    #[test]
    fn a_row_the_peer_invented_is_inserted_whole_and_not_pending() {
        let c = conn();
        let report =
            apply(&c, &snap_of(&[("todos", vec![row("new-1", 700, "phone-a")])]), PC_ACCEPTS)
                .unwrap();
        assert_eq!(report.applied, 1);
        let (title, _, _, dirty) = read(&c, "new-1");
        assert_eq!((title.as_str(), dirty), ("对端写的", 0));
        // 建表时有 DEFAULT 的列由 SQLite 兜住，不是我们在这里编值
        let zone: String = c
            .query_row("SELECT zone FROM todos WHERE id='new-1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(zone, "inbox");
    }

    #[test]
    fn a_child_row_survives_merging_its_parent_writing_that_way() {
        // 换掉 INSERT OR REPLACE 的那条理由要有个钉子：改父项时子项的外键不能炸。
        let c = conn();
        seed(&c, "p", 100, "pc-a", "装修");
        c.execute(
            "INSERT INTO todos (id, rev, created_at, created_by, updated_at, updated_by, title,
                                parent_id, sort_key, zone, completed, dirty)
             VALUES ('kid',1,100,'pc-a',100,'pc-a','买漆','p',2048,'inbox',0,0)",
            [],
        )
        .unwrap();
        let mut parent = row("p", 200, "phone-a");
        parent.insert("title".to_owned(), json!("装修（对端改的）"));
        apply(&c, &snap_of(&[("todos", vec![parent])]), PC_ACCEPTS).unwrap();
        assert_eq!(read(&c, "p").0, "装修（对端改的）");
        assert_eq!(live_rows(&c), 2, "子项还在，父项整行换掉");
    }

    #[test]
    fn missing_stamp_aborts_the_whole_batch_rather_than_half_applying() {
        let c = conn();
        let mut half = row("t1", 100, "phone-a");
        half.remove("updated_at");
        let good = row("t2", 100, "phone-a");
        let err = apply(
            &c,
            &snap_of(&[("todos", vec![half, good])]),
            PC_ACCEPTS,
        )
        .unwrap_err();
        assert!(err.to_string().contains("updated_at"), "{err}");
        assert_eq!(live_rows(&c), 0, "整批一个事务：失败就一行都不该落地");
    }

    #[test]
    fn a_json_shaped_value_is_refused_by_name() {
        let c = conn();
        let mut weird = row("t1", 100, "phone-a");
        weird.insert("note".to_owned(), json!(["a", "b"]));
        let err = apply(&c, &snap_of(&[("todos", vec![weird])]), PC_ACCEPTS).unwrap_err();
        assert!(err.to_string().contains("note"), "报错要点名是哪一列：{err}");
        assert_eq!(live_rows(&c), 0);
    }

    /// `"100"` 进 INTEGER 列会被 SQLite 悄悄转成 100；"1" 进 TEXT 列也会悄悄变形状。
    /// 这类转换不报错，等到下一次按类型比较或统计时才表现成"数据莫名不对"。
    #[test]
    fn a_scalar_of_the_wrong_declared_shape_is_refused_by_name() {
        let c = conn();
        let mut wrong = row("t1", 100, "phone-a");
        wrong.insert("priority".to_owned(), json!("2"));
        let err = apply(&c, &snap_of(&[("todos", vec![wrong])]), PC_ACCEPTS)
            .unwrap_err()
            .to_string();
        assert!(err.contains("priority") && err.contains("INTEGER"), "{err}");
        assert_eq!(live_rows(&c), 0, "整批一个事务：类型错也不能落一半");

        let mut wrong = row("t2", 100, "phone-a");
        wrong.insert("title".to_owned(), json!(123));
        let err = apply(&c, &snap_of(&[("todos", vec![wrong])]), PC_ACCEPTS)
            .unwrap_err()
            .to_string();
        assert!(err.contains("title") && err.contains("TEXT"), "{err}");

        let mut fractional = row("t3", 100, "phone-a");
        fractional.insert("priority".to_owned(), json!(1.5));
        let err = apply(&c, &snap_of(&[("todos", vec![fractional])]), PC_ACCEPTS)
            .unwrap_err()
            .to_string();
        assert!(err.contains("priority"), "{err}");

        let mut whole_float = row("t4", 100, "phone-a");
        whole_float.insert("priority".to_owned(), json!(2.0));
        apply(&c, &snap_of(&[("todos", vec![whole_float])]), PC_ACCEPTS)
            .expect("整数值的浮点可以落到 INTEGER，不该被误杀");
    }

    #[test]
    fn dirty_clearing_uses_the_snapshot_stamp_not_the_wall_clock() {
        // §3.3 第 2 条，也是药箱 db.dart:246 那条教训的落地：
        // 序列化之后、PUT 响应回来之前新写的那一条必须还脏着。
        let c = conn();
        seed(&c, "t1", 100, "pc-a", "快照里带着的");
        seed(&c, "t2", 150, "pc-a", "也带着");
        clear_dirty(&c, 200).unwrap();
        assert_eq!(pending(&c).unwrap(), 0, "sealed_at=200 那份快照含 t1/t2，两条都该清");

        // PUT 还在路上，用户又改了 t2、并新建了 t3
        c.execute("UPDATE todos SET updated_at = 260, dirty = 1 WHERE id = 't2'", [])
            .unwrap();
        c.execute(
            "INSERT INTO todos (id, rev, created_at, created_by, updated_at, updated_by,
                                title, sort_key, zone, completed, dirty)
             VALUES ('t3',1,300,'pc-a',300,'pc-a','路上新写的',2048,'inbox',0,1)",
            [],
        )
        .unwrap();

        // 响应回来了，清的是**那份** sealed_at=200 的脏
        clear_dirty(&c, 200).unwrap();
        assert_eq!(
            pending(&c).unwrap(),
            2,
            "路上改的 t2 与新写的 t3 都还得留在 outbox 里"
        );

        // 下一趟真把 300 这份传上去了，才轮到它们清掉
        clear_dirty(&c, 300).unwrap();
        assert_eq!(pending(&c).unwrap(), 0);
    }

    #[test]
    fn every_local_write_path_marks_the_row_pending() {
        // §3.4：漏置脏不会报错，只会让那一行永远上不了云 —— 所以每条写路径钉一次。
        // 每一步都是「先清脏（当作这份已经传上去了）→ 走一条写路径 → 数它脏了几行」，
        // 这样任何一条路径忘了置脏都会在这里变成 0。
        use crate::model::TodoDraft;
        use crate::model::zones;
        use crate::repos::todos;
        use crate::time::FixedNow;

        const MS: i64 = 1_700_000_000_000;
        let c = conn();
        let now = FixedNow(MS);
        let device = "pc-a";
        let settled = |want: i64, why: &str| {
            assert_eq!(pending(&c).unwrap(), want, "{why}：待上传的应该是 {want} 行");
            assert_eq!(clear_dirty(&c, MS).unwrap() as i64, want, "{why}：清脏数要等于计数");
            assert_eq!(pending(&c).unwrap(), 0, "{why}：清完之后不该还剩");
        };
        let draft = |title: &str| TodoDraft {
            title: title.to_owned(),
            ..Default::default()
        };

        let a = todos::create(&c, &now, device, draft("买猫粮")).unwrap();
        settled(1, "新建");
        todos::create(&c, &now, device, draft("写周报")).unwrap();
        settled(1, "新建第二行");

        todos::set_done(&c, &now, device, &a.stamp.id, true).unwrap();
        settled(1, "勾选完成");
        todos::update(
            &c,
            &now,
            device,
            &a.stamp.id,
            crate::model::TodoPatch {
                title: Some("买猫粮（大袋）".into()),
                ..Default::default()
            },
        )
        .unwrap();
        settled(1, "改字段");
        todos::set_done(&c, &now, device, &a.stamp.id, false).unwrap();
        settled(1, "取消勾选 —— 0002 之后它是一次真写入，不是一句「只推 rev」");

        let b = todos::list(&c, zones::INBOX)
            .unwrap()
            .into_iter()
            .find(|t| t.stamp.id != a.stamp.id)
            .unwrap();
        todos::reorder(&c, &now, device, &b.stamp.id, Some(&a.stamp.id), None).unwrap();
        settled(1, "拖拽排序");
        todos::rebalance(&c, &now, device, zones::INBOX).unwrap();
        settled(2, "整区重排：两行都改了键，两行都该脏");

        let gone = todos::soft_delete(&c, &now, device, &a.stamp.id).unwrap();
        settled(1, "删除更要传出去");
        todos::restore(&c, &now, device, &gone).unwrap();
        settled(1, "还原");

        // 提醒盖章也算：`reminded_at` 在快照里，不置脏就等于两台机器上这个标记会长期不一致
        let due = todos::create(
            &c,
            &FixedNow(MS - 10),
            device,
            TodoDraft {
                title: "记得弹我".into(),
                remind_at: Some(MS - 5),
                ..Default::default()
            },
        )
        .unwrap();
        clear_dirty(&c, MS).unwrap();
        let fired = todos::take_due_reminders(&c, &now, device, 10).unwrap();
        assert!(
            fired.iter().any(|r| r.id == due.stamp.id),
            "夹具要让提醒真的到点"
        );
        settled(1, "弹过通知也是写入");
    }

    #[test]
    fn unknown_columns_from_a_newer_peer_are_ignored_not_fatal() {
        let c = conn();
        let mut future = row("t1", 800, "phone-a");
        future.insert("recurrence".to_owned(), json!("每天"));
        apply(&c, &snap_of(&[("todos", vec![future])]), PC_ACCEPTS).unwrap();
        assert_eq!(read(&c, "t1").0, "对端写的", "对面多出来的列不挡这一行落地");
    }
}
