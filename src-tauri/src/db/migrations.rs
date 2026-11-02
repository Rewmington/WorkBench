use rusqlite::Connection;

use crate::error::{Error, Result};

struct Migration {
    name: &'static str,
    sql: &'static str,
}

/// 迁移按顺序追加，`PRAGMA user_version` 就是已应用的条数。
/// 只加不改：已经在别人机器上跑过的编号不能改内容。
const MIGRATIONS: &[Migration] = &[
    Migration {
        name: "0001_init",
        sql: include_str!("../../migrations/0001_init.sql"),
    },
    Migration {
        name: "0002_done_state",
        sql: include_str!("../../migrations/0002_done_state.sql"),
    },
    Migration {
        name: "0003_countdown_pause",
        sql: include_str!("../../migrations/0003_countdown_pause.sql"),
    },
    Migration {
        name: "0004_remind_fired",
        sql: include_str!("../../migrations/0004_remind_fired.sql"),
    },
    Migration {
        name: "0005_timetable",
        sql: include_str!("../../migrations/0005_timetable.sql"),
    },
    Migration {
        name: "0006_sync_dirty",
        sql: include_str!("../../migrations/0006_sync_dirty.sql"),
    },
    Migration {
        name: "0007_created_hour_index",
        sql: include_str!("../../migrations/0007_created_hour_index.sql"),
    },
];

pub fn migrate(conn: &mut Connection) -> Result<u32> {
    let current: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    // 库比程序新 —— 装了旧版本，或者从旧备份把整个 app.db 换回去时都会撞上。
    // 让它继续跑不会当场报错：`skip(from)` 直接跳过整个循环，程序照起，只是这一版
    // 看不见的那几列照常参与同步与统计，表现是「数据莫名缺字段」，属于最难查的那类。
    // 所以拒绝启动，让启动失败那个弹窗（`logger::startup_failure`）把话说完。
    if current as usize > MIGRATIONS.len() {
        return Err(Error::msg(format!(
            "本机数据库比这个版本的新：库里记的是第 {current} 版，这个程序只认到第 {} 版。\
             请装回不低于它的版本再打开；用旧版本继续写会把新字段的数据写坏。",
            MIGRATIONS.len()
        )));
    }
    apply(conn, MIGRATIONS, current as usize)
}

/// 循环单独抽出来，是为了能喂进一条**注定失败**的迁移：下面那句"半个迁移不能伪装成已应用"
/// 光靠注释说不算 —— 第八条迁移一加，回归就只压在那句话上，而它出错的表现是
/// 「库看起来升上去了，其实缺列」，要到第一次用到那张表的命令才炸。
fn apply(conn: &mut Connection, migrations: &[Migration], from: usize) -> Result<u32> {
    for (offset, m) in migrations.iter().enumerate().skip(from) {
        // DDL 在 SQLite 里是事务性的：建表与版本号 bump 同事务，
        // 半个迁移不能伪装成已应用。
        let tx = conn.transaction()?;
        tx.execute_batch(m.sql).map_err(|source| Error::Migration {
            name: m.name,
            source,
        })?;
        tx.pragma_update(None, "user_version", (offset + 1) as u32)?;
        tx.commit()?;
        log::info!("已应用迁移 {}", m.name);
    }

    Ok(migrations.len() as u32)
}

pub fn version(conn: &Connection) -> Result<u32> {
    Ok(conn.query_row("PRAGMA user_version", [], |r| r.get(0))?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_is_idempotent_and_counts_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        // 用 len() 而不是写死的数字：往后加一条迁移不该来改这条测试。
        // 真正在验的是第二行 —— 重复执行会重跑 ALTER TABLE，一重跑就 panic。
        let total = MIGRATIONS.len() as u32;
        migrate(&mut conn).unwrap();
        assert_eq!(version(&conn).unwrap(), total);
        migrate(&mut conn).unwrap();
        assert_eq!(version(&conn).unwrap(), total);

        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        // 13 张表 + sqlite_sequence（backup_log 用了 AUTOINCREMENT）
        assert!(n >= 13, "期望至少 13 张表，实际 {n}");
    }

    /// 上面那条测试证明"能跑完"，这条证明"跑不完的时候不留半个"。
    #[test]
    fn a_failing_migration_does_not_disguise_itself_as_applied() {
        let mut conn = Connection::open_in_memory().unwrap();
        let migrations = [
            Migration {
                name: "0001_ok",
                sql: "CREATE TABLE todos(id INTEGER);",
            },
            Migration {
                name: "0002_bad",
                // 前一句建表、后一句引用不存在的表：若事务没兜住，half_table 就留下了，
                // 而版本号要么停在 1（表却在）要么走到 2（语句没成）。
                sql: "CREATE TABLE half_table(x); INSERT INTO no_such_table VALUES (1);",
            },
        ];

        let err = apply(&mut conn, &migrations, 0).unwrap_err();
        assert!(
            err.to_string().contains("0002_bad"),
            "启动日志必须点出是哪一条迁移坏了，光一句数据库错误没法查：{err}"
        );
        assert_eq!(version(&conn).unwrap(), 1, "第二条没成，user_version 不能写成 2");

        let half: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'half_table'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(half, 0, "同事务里的 DDL 必须跟着一起回滚");

        // 坏的那条修好后仍能从断点续上，而不是从头重跑（重跑第一条会撞上已存在的 todos）。
        // 第一条只是占位：`from = 1` 把它跳过，不会执行它的 SQL。
        let repaired = [
            Migration {
                name: "0001_ok",
                sql: "这一条已经应用过，占位用，不该被执行",
            },
            Migration {
                name: "0002_bad",
                sql: "CREATE TABLE half_table(x);",
            },
        ];
        apply(&mut conn, &repaired, 1).expect("修好后续跑");
        assert_eq!(version(&conn).unwrap(), 2);
    }

    /// 库比程序新：不能"默默什么都不做"然后带着看不懂的库起跑。
    #[test]
    fn a_database_newer_than_the_app_refuses_to_open() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        let ahead = MIGRATIONS.len() as u32 + 3;
        conn.pragma_update(None, "user_version", ahead).unwrap();

        let err = migrate(&mut conn).unwrap_err().to_string();
        assert!(err.contains("比这个版本的新"), "要说人话，不能只一句数据库错误：{err}");
        assert!(err.contains(&ahead.to_string()), "版本号要念出来，不然没法核对：{err}");
        assert_eq!(
            version(&conn).unwrap(),
            ahead,
            "拒绝启动不许顺手把版本号改回去（那是抹掉证据）"
        );
    }

    /// 升级路径真正要保的是**数据**与**形状**：
    /// 只验"DDL 能跑完"的话，一条老库升上来发现行没了、或者升出来的库和新装的库不一样，
    /// 都抓不到。这里插两条 0001 时代的行，升到最新，再和"一次建好"的库逐张对象对比。
    #[test]
    fn upgrading_an_old_database_keeps_rows_and_lands_on_the_same_shape() {
        let mut old = Connection::open_in_memory().unwrap();
        apply(&mut old, &MIGRATIONS[..1], 0).unwrap(); // 停在 0001：一个 v0.1.x 时代的库
        // 0001 只有 done_at，没有 completed / dirty —— 正是这两列后来补的
        old.execute(
            "INSERT INTO todos (id, rev, created_at, created_by, updated_at, updated_by, title, done_at)
             VALUES ('t-done', 1, 7, 'pc', 7, 'pc', '做完的一条', 7)",
            [],
        )
        .unwrap();
        old.execute(
            "INSERT INTO todos (id, rev, created_at, created_by, updated_at, updated_by, title)
             VALUES ('t-open', 1, 7, 'pc', 7, 'pc', '没做完的一条')",
            [],
        )
        .unwrap();

        migrate(&mut old).unwrap();

        let rows: i64 = old
            .query_row("SELECT count(*) FROM todos", [], |r| r.get(0))
            .unwrap();
        assert_eq!(rows, 2, "升级不许丢行");

        // 0002 的语义回填：有 done_at 的算已完成，没有的算未完成
        let completed: i64 = old
            .query_row("SELECT completed FROM todos WHERE id = 't-done'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(completed, 1, "0002 的回填跑没跑");
        let open_completed: i64 = old
            .query_row("SELECT completed FROM todos WHERE id = 't-open'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(open_completed, 0);

        // 0006：存量行一律 dirty=1（它们从没上过云，初始化成 0 就是谎报已上传）
        let dirty: i64 = old
            .query_row("SELECT count(*) FROM todos WHERE dirty = 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(dirty, 2, "升级上来的存量行要算「还没发出去」");

        let shape = |c: &Connection| -> Vec<String> {
            let mut stmt = c
                .prepare("SELECT name || ' :: ' || COALESCE(sql, '') FROM sqlite_master ORDER BY type, name")
                .unwrap();
            stmt.query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap()
        };
        let mut fresh = Connection::open_in_memory().unwrap();
        migrate(&mut fresh).unwrap();
        assert_eq!(
            shape(&old),
            shape(&fresh),
            "升上来的库和新装的库必须是同一个形状，否则两条路径的 schema 会越走越远"
        );
    }
}
