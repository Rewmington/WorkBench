use std::path::Path;
use std::time::Duration;

use rusqlite::Connection;

use crate::error::Result;

/// 单文件库 + WAL。`busy_timeout` 给同一台机器上可能的第二个进程（备份恢复窗口）留余地。
pub fn open(db_path: &Path) -> Result<Connection> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(db_path)?;
    configure(&conn)?;
    Ok(conn)
}

pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    configure(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    conn.busy_timeout(Duration::from_millis(5000))?;
    conn.execute("PRAGMA foreign_keys = ON", [])?;
    Ok(())
}

pub fn journal_mode(conn: &Connection) -> String {
    conn.query_row("PRAGMA journal_mode", [], |r| r.get::<_, String>(0))
        .unwrap_or_else(|_| "unknown".into())
}
