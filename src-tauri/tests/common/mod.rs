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
            // 故意写成一条**云端来源**的药箱进度：备份必须连它一起剥掉
            detail: "云端 · phone 3 分钟前发布".to_owned(),
            payload_json: "{\"remaining\":6}".to_owned(),
        },
    )
    .expect("linked upsert");

    // 课表：手工导入的本机数据，与上面那条药箱缓存相反 —— 它该跟着备份出门（PLAN §6.7）
    conn.execute(
        "INSERT INTO classes (id, rev, created_at, created_by, updated_at, updated_by,
                              deleted_at, term, name, teacher, room, day,
                              start_min, end_min, period, weeks)
         VALUES ('cls-fixture',1,?1,'pc-a',?1,'pc-a',NULL,'2026-2027-1','数据结构','张老师','一教101',3,480,580,'第一大节 [01-02]节','1-16')",
        [MS],
    )
    .expect("class insert");

    for (key, value) in [
        (
            settings::KEY_BLOG_REPO_DIR,
            "D:\\fixture\\blog-repo-not-a-real-path",
        ),
        (
            settings::KEY_MEDICINE_DB,
            "C:\\fixture\\medicine_tracker-not-a-real-path.db",
        ),
        (settings::KEY_WDAV_EMAIL, "webdav-fixture@example.com"),
        // 药箱那边用的邮箱：同样是邮箱，一样不能出门。
        // 两条故意给不同的值 —— 相等就测不出「剥的是键，不是某一键」
        (settings::KEY_MEDICINE_EMAIL, "medicine-fixture@example.com"),
        // 目录地址是机器无关的，故意留着 —— 还原后仍然有效
        (
            settings::KEY_MEDICINE_DAV_DIR,
            "https://dav.jianguoyun.com/dav/medicine-sync/",
        ),
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
