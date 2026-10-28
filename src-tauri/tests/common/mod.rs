//! M4 两个集成测试共用的夹具。`#![allow(dead_code)]`：两个 test binary 用到的子集不同。
#![allow(dead_code)]

use std::io::Read;
use std::path::PathBuf;

use rusqlite::Connection;

use anvil_lib::db::migrations::migrate;
use anvil_lib::linking::{Status, Snapshot};
use anvil_lib::model::TodoDraft;
use anvil_lib::repos::{linked, settings, todos};
use anvil_lib::time::FixedNow;

pub const SECRET: &str = "stub@example.com:stub-app-password";
/// 2026-10-28T15:00+08:00 —— 与前端 reltime 的夹具同一时刻，本地日界是 20261028。
pub const MS: i64 = 1_793_170_800_000;

pub fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "anvil-{tag}-{}-{}",
        std::process::id(),
        uuid::Uuid::now_v7().simple()
    ));
    std::fs::create_dir_all(&dir).expect("建临时目录");
    dir
}

pub fn cleanup(dir: &PathBuf) {
    let _ = std::fs::remove_dir_all(dir);
}

/// 建库 + 迁移 + 塞进「一台用得有点脏的机器」的内容：待办、外部缓存、本机绝对路径、坚果云邮箱。
pub fn lived_in(conn: &mut Connection, n_todos: usize) {
    migrate(conn).expect("迁移");
    let now = FixedNow(MS);
    for i in 0..n_todos {
        todos::create(
            conn,
            &now,
            "pc-a",
            TodoDraft {
                title: format!("写备份这条链路的测试 {i}"),
                ..Default::default()
            },
        )
        .expect("create");
    }
    linked::upsert(
        conn,
        &Snapshot {
            source: "medicine".to_owned(),
            kind: "inventory".to_owned(),
            fetched_at: MS,
            sig: "deadbeef".to_owned(),
            status: Status::Ok,
            detail: String::new(),
            payload_json: "{\"remaining\":6}".to_owned(),
        },
    )
    .expect("linked upsert");

    for (key, value) in [
        (
            settings::KEY_BLOG_REPO_DIR,
            "D:\\ADevelop\\Repositories\\Mine\\Rewmington-Blog-main",
        ),
        (
            settings::KEY_MEDICINE_DB,
            "C:\\Users\\hp\\AppData\\Roaming\\dev.mine\\medicine_tracker\\medicine_tracker.db",
        ),
        (settings::KEY_WDAV_EMAIL, "me@wmddd.online"),
        (settings::KEY_THEME_MODE, "auto"),
    ] {
        settings::set_json(conn, key, &serde_json::json!(value), MS, "pc-a").expect("set");
    }
    // 留一条流水，验证它不会被传上去
    conn.execute(
        "INSERT INTO backup_log (at, ok, bytes, remote_path, error, kind) VALUES (?1,1,10,'x',' ','manual')",
        rusqlite::params![MS],
    )
    .expect("backup_log");
}

pub fn gunzip(gz: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(gz)
        .read_to_end(&mut out)
        .expect("解压");
    out
}

pub fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).unwrap_or(0)
}
