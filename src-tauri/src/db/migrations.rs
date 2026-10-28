use rusqlite::Connection;

use crate::error::{Error, Result};

struct Migration {
    name: &'static str,
    sql: &'static str,
}

/// 迁移按顺序追加，`PRAGMA user_version` 就是已应用的条数。
/// 只加不改：已经在别人机器上跑过的编号不能改内容。
const MIGRATIONS: &[Migration] = &[Migration {
    name: "0001_init",
    sql: include_str!("../../migrations/0001_init.sql"),
}];

pub fn migrate(conn: &mut Connection) -> Result<u32> {
    let current: u32 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    for (offset, m) in MIGRATIONS.iter().enumerate().skip(current as usize) {
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

    Ok(MIGRATIONS.len() as u32)
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
        let applied = migrate(&mut conn).unwrap();
        assert_eq!(applied, 1);
        // M0 验收：迁移可重复执行。
        migrate(&mut conn).unwrap();
        assert_eq!(version(&conn).unwrap(), 1);

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
}
