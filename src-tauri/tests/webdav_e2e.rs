//! M4 验收：对着 `tools/webdav-stub.mjs` 跑完整的 PUT → PROPFIND → GET → 还原链路。
//!
//! 这个测试默认忽略，因为它要一个外部进程。跑法只有一种（由 `tools/m4-check.mjs` 拉起）：
//!   node tools/m4-check.mjs
//! 起桩、传环境变量、收摊都在那儿。真实上传留给用户在设置页点「立即备份」。

mod common;
use common::{cleanup, count, gunzip, lived_in, temp_dir, SECRET};

use rusqlite::Connection;

use anvil_lib::backup::{self, crypto, webdav::Remote};
use anvil_lib::db;
use anvil_lib::repos::settings;
use anvil_lib::state::Paths;
use anvil_lib::time::FixedNow;
use serde_json::json;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("m4-check.mjs 没传 {name}"))
}

fn remote() -> Remote {
    Remote::new(
        &env("ANVIL_WEBDAV_BASE"),
        &env("ANVIL_WDAV_USER"),
        &env("ANVIL_WDAV_PASS"),
    )
    .expect("桩的目录必须满足 workbench-backup 边界")
}

#[test]
#[ignore = "需要 tools/webdav-stub.mjs 在跑；请用 node tools/m4-check.mjs"]
fn put_propfind_get_and_restore_against_the_stub() {
    let r = remote();
    r.ensure_dir().expect("MKCOL");

    // 桩里预先混了一个药箱的 device-pc1.json：客户端必须看不见它
    let listed = r.list().expect("PROPFIND");
    assert!(
        listed.iter().all(|e| !e.name.starts_with("device-")),
        "device-* 泄漏到了上层：{:?}",
        listed.iter().map(|e| &e.name).collect::<Vec<_>>()
    );

    // 一次真备份：文件库 → 导出 → 加密 → PUT
    let tmp = temp_dir("e2e-src");
    let paths = Paths::under(tmp.join("appdata"));
    paths.ensure().expect("目录");
    let mut conn = db::open(&paths.db).expect("源库");
    lived_in(&mut conn, 12);

    let outcome = backup::run_with(&conn, &paths.tmp, &FixedNow(common::MS), "manual", &r, SECRET);
    assert!(outcome.ok, "备份失败：{}", outcome.error.unwrap_or_default());
    let name = outcome.remote_path.clone().unwrap();
    assert!(
        name.starts_with("anvil-") && name.ends_with(".abackup"),
        "远端文件名不合规矩：{name}"
    );
    let after = r.list().expect("再次 PROPFIND");
    assert!(
        after.iter().any(|e| e.name == name),
        "上传完却列不出来：{name}"
    );
    assert_eq!(
        after.iter().filter(|e| e.name.starts_with("device-")).count(),
        0,
        "远端确实有那台药箱文件，但一个都不许报出来"
    );

    // 下载回来：产物里不该有外部缓存、本机路径、账号
    let sealed = r.get(&name).expect("GET");
    assert_eq!(sealed.len() as u64, outcome.bytes.unwrap(), "尺寸要对得上");
    let db_bytes = gunzip(&crypto::open(SECRET, &sealed).expect("解密"));
    let downloaded = tmp.join("downloaded.db");
    std::fs::write(&downloaded, &db_bytes).unwrap();
    let probe = Connection::open(&downloaded).unwrap();
    assert_eq!(
        probe
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert_eq!(count(&probe, "SELECT count(*) FROM todos"), 12);
    assert_eq!(count(&probe, "SELECT count(*) FROM linked_snapshots"), 0);
    assert_eq!(count(&probe, "SELECT count(*) FROM backup_log"), 0);
    assert_eq!(
        count(
            &probe,
            "SELECT count(*) FROM settings WHERE value LIKE '%hp%' OR value LIKE '%D:\\%' OR value LIKE '%wmddd.online%'"
        ),
        0,
        "产物里不该有别的机器的绝对路径或账号"
    );

    // 换机器还原：全新目录 + 一个旧的 device.id 等着被换掉
    let fresh = temp_dir("e2e-target");
    let target = Paths::under(fresh.join("appdata"));
    target.ensure().expect("目标目录");
    std::fs::write(&target.device_id, "pc-old").unwrap();
    let mut conn2 = db::open(&target.db).expect("目标库");
    lived_in(&mut conn2, 1);
    // 这台机器登录的是另一个账号 —— 还原之后留下的必须是它，不是快照那台机器的
    settings::set_json(
        &conn2,
        settings::KEY_WDAV_EMAIL,
        &json!("this-machine@example.com"),
        common::MS,
        "pc-old",
    )
    .expect("本机邮箱");

    let at = common::MS + 60_000;
    let bytes = backup::restore_with(&mut conn2, &target, &r, SECRET, &name, at).expect("还原");
    assert!(bytes > 0);

    // 还原成功之后，传进来的那个连接必须已经指回真库。以前它停在内存占位上，要等
    // `app.restart()` 才死；中间在 mutex 上排队的每条命令都会报 "no such table"。
    assert_eq!(
        count(&conn2, "SELECT count(*) FROM todos"),
        count(&probe, "SELECT count(*) FROM todos"),
        "restore_with 回来就能用那个连接查，不该逼调用方重开一个"
    );

    let reopened = Connection::open(&target.db).expect("还原后的库");
    let src_version: i64 = probe
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    let dst_version: i64 = reopened
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(src_version, dst_version, "user_version 必须原样带过来");
    assert_eq!(
        count(&reopened, "SELECT count(*) FROM todos"),
        count(&probe, "SELECT count(*) FROM todos"),
        "还原后的待办数与源库一致"
    );

    assert_eq!(
        settings::get_json(&reopened, settings::KEY_WDAV_EMAIL).unwrap(),
        Some(json!("this-machine@example.com")),
        "还原换的是库，不是这台机器的账号：邮箱丢了 = 备份页与药箱页同时变成未配置"
    );
    assert_eq!(
        count(
            &reopened,
            "SELECT count(*) FROM settings WHERE key IN ('linked.blog.repoDir','linked.medicine.dbPath','webdav.email','linked.medicine.email')"
        ),
        4,
        "四项本机设置都得原地留下（快照里根本没有它们）"
    );

    let id = std::fs::read_to_string(&target.device_id).expect("device.id 要还在");
    assert_ne!(id, "pc-old", "整文件恢复会把旧机器的 created_by 带回来，身份必须换新");
    assert!(
        id.starts_with("pc-") && id.len() == 35,
        "新设备 id 的形状不对：{id}"
    );
    assert!(
        target
            .root
            .join(format!("app.pre-restore-{at}.db"))
            .exists(),
        "覆盖前必须留住旧库"
    );

    cleanup(&tmp);
    cleanup(&fresh);
}

#[test]
#[ignore = "需要桩在跑"]
fn wrong_credentials_are_reported_in_chinese() {
    let r = Remote::new(
        &env("ANVIL_WEBDAV_BASE"),
        &env("ANVIL_WDAV_USER"),
        "不对的密码",
    )
    .unwrap();
    let err = r.list().unwrap_err().to_string();
    assert!(err.contains("坚果云拒绝了凭据"), "{err}");
}

#[test]
#[ignore = "需要桩在跑"]
fn the_medicine_sync_directory_is_unreachable() {
    // 边界写进代码：填错目录不是警告，是直接拒绝构造 Remote
    for base in [
        "https://dav.jianguoyun.com/dav/medicine-sync/",
        "https://dav.jianguoyun.com/dav/",
        "https://dav.jianguoyun.com/dav/workbench-backup",
    ] {
        assert!(
            Remote::new(base, "u@example.com", "p").is_err(),
            "{base} 不该被接受"
        );
    }
    // device-* 这种名字，连请求都不该发出去
    let r = remote();
    let err = r.put("device-pc2.json", b"{}").unwrap_err().to_string();
    assert!(err.contains("device-*"), "{err}");
    assert!(r.get("device-pc1.json").is_err(), "药箱的文件不是我们的可读物");
    assert!(r.delete("device-pc1.json").is_err());
    assert!(
        std::fs::exists(std::path::Path::new(&env("ANVIL_STUB_DIR")).join("device-pc1.json"))
            .unwrap(),
        "被拒绝的删除不该真的动到远端"
    );
}
