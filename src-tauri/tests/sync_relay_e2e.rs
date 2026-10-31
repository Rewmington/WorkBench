//! M11b 验收：对着 `tools/webdav-stub.mjs` 跑一次完整的手机同步 ——
//! MKCOL → PROPFIND → GET 对端 → 合并 → seal_with(同步档) → PUT → clear_dirty(快照戳)。
//!
//! 这个测试默认忽略，因为它要一个外部进程。跑法只有一种（由 `tools/m11b-check.mjs` 拉起）：
//!   node tools/m11b-check.mjs
//! 起桩、传环境变量、收摊都在那儿。合并语义本身是 M11a 的 `sync::merge` 单测钉死的，
//! 这里只钉传输形状与 §6 M11b 点名的三条硬要求。

mod common;
use common::{temp_dir, SECRET};

use std::cell::Cell;

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use anvil_lib::backup::crypto;
use anvil_lib::db;
use anvil_lib::model::TodoDraft;
use anvil_lib::repos::{settings, todos};
use anvil_lib::sync::{merge, relay, snapshot::Snapshot};
use anvil_lib::time::{FixedNow, Now};

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("m11b-check.mjs 没传 {name}"))
}

/// 同步 base 从备份 base 折出来 —— 与 `relay::Remote::from_env` 走的是同一个函数，
/// 桩环境变量在这里被真正行使。
fn remote() -> relay::Remote {
    relay::Remote::from_env(&env("ANVIL_WDAV_USER"), &env("ANVIL_WDAV_PASS"))
        .expect("桩的 base 必须折成合法的 workbench-sync 目录")
}

fn conn() -> Connection {
    let mut c = db::open_in_memory().unwrap();
    db::migrations::migrate(&mut c).unwrap();
    c
}

/// 每个用例先把同步目录里的 `device-*.enc` 扫干净（桩把文件平铺在磁盘上，没有内存态），
/// 但**留下**非 device-* 的预放杂物 —— `list()` 的后缀钉子必须一直看得见它们。
fn wipe_device_files() {
    let dir = env("ANVIL_SYNC_DIR");
    for entry in std::fs::read_dir(&dir).expect("同步目录应存在") {
        let entry = entry.unwrap();
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with(relay::FILE_PREFIX)
        {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// 一行完整的 todos 同步行，形状与 `snapshot::dump` 发出去的一致（没有 dirty）。
fn phone_row(id: &str, at: i64, by: &str, title: &str) -> Map<String, Value> {
    json!({"id": id, "rev": 1, "created_at": at, "created_by": by,
           "updated_at": at, "updated_by": by, "deleted_at": Value::Null,
           "title": title, "note": "", "done_at": Value::Null, "done_day": Value::Null,
           "due_at": Value::Null, "remind_at": Value::Null, "priority": 2, "pinned": 0,
           "parent_id": Value::Null, "sort_key": 1024.0, "zone": "inbox",
           "created_day": 20261030, "created_hour": 9, "completed": 0,
           "reminded_at": Value::Null})
        .as_object()
        .unwrap()
        .clone()
}

/// 手机侧那份文件的字节：Snapshot → gzip → **同步档**信封（§4 实测补记那一条）。
fn phone_envelope(device: &str, sealed_at: i64, rows: Vec<Map<String, Value>>) -> Vec<u8> {
    let mut tables = std::collections::BTreeMap::new();
    tables.insert("todos".to_owned(), rows);
    let snap = Snapshot {
        device: device.to_owned(),
        sealed_at,
        tables,
        meta: std::collections::BTreeMap::new(),
    };
    crypto::seal_with(
        SECRET,
        &snap.to_bytes().unwrap(),
        crypto::SYNC_M_KIB,
        crypto::SYNC_T_COST,
        crypto::SYNC_P_COST,
    )
    .unwrap()
}

const MS: i64 = common::MS;

#[test]
#[ignore = "需要 tools/webdav-stub.mjs 在跑；请用 node tools/m11b-check.mjs"]
fn phone_snapshot_merges_in_and_the_pc_uploads_its_own_envelope() {
    wipe_device_files();
    let r = remote();
    r.ensure_dir().expect("MKCOL（已存在时 405 也算过）");
    r.ensure_dir().expect("第二次 MKCOL 必须把 405 当已存在，不是失败");

    // 远端先有一台手机发布的文件
    let phone = phone_row("e2e-phone-1", MS - 1_000, "phone-a", "手机写的待办");
    r.put("device-phone-a.enc", &phone_envelope("phone-a", MS - 1_000, vec![phone]))
        .unwrap();

    // 本机有一条没传出去的写入
    let c = conn();
    let local = todos::create(
        &c,
        &FixedNow(MS),
        "pc-e2e",
        TodoDraft {
            title: "PC 上还没传的".into(),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(merge::pending(&c).unwrap(), 1);

    let out = relay::run_with(&c, "pc-e2e", SECRET, &r, &FixedNow(MS));
    assert!(out.ok, "同步失败：{}", out.error.unwrap_or_default());
    assert_eq!(out.applied, 1, "手机那条应并进本地");
    assert!(out.uploaded && !out.put_skipped, "pending>0 必须 PUT");
    assert_eq!(out.pending_after, 0, "PUT 成功后按快照戳清脏");
    assert_eq!(out.sealed_at, Some(MS));
    assert_eq!(out.peers, vec!["phone-a".to_owned()], "listing 不含自己那份");

    // 手机那条真的落了库
    let titles: Vec<String> = todos::list(&c, anvil_lib::model::zones::INBOX)
        .unwrap()
        .into_iter()
        .map(|t| t.title)
        .collect();
    assert!(titles.contains(&"手机写的待办".to_owned()), "{titles:?}");

    // 本机那份：信封头部的 kdf 参数必须是**同步档**（§4：手机开它不能被迫用 64 MiB）
    let blob = r.get(&relay::file_name("pc-e2e")).expect("自己那份应在");
    assert_eq!(&blob[..8], b"ANVILBKP", "信封魔数");
    let m_kib = u32::from_be_bytes([blob[10], blob[11], blob[12], blob[13]]);
    assert_eq!(m_kib, crypto::SYNC_M_KIB, "同步必须用 seal_with 的同步档成本");
    let back = Snapshot::from_bytes(&crypto::open(SECRET, &blob).unwrap()).unwrap();
    let ids: Vec<&str> = back
        .rows("todos")
        .iter()
        .filter_map(|row| row.get("id").and_then(Value::as_str))
        .collect();
    assert!(ids.contains(&"e2e-phone-1"), "合并进来的行要随自己这份再发出去");
    assert!(ids.contains(&local.stamp.id.as_str()));
    for row in back.rows("todos") {
        assert!(!row.contains_key("dirty"), "本机状态不外传");
    }

    // 远端 listing：只认 device-*.enc；桩预放的 stray-note.txt 与任何邮箱字样都不许出现。
    // （§6 M11b：设备 listing 出得来，邮箱不进文件名 —— KDF secret 是 email:password，
    // 文件名一旦带上它就等于把账号写在了公开目录里。）
    let names: Vec<String> = r.list().unwrap().into_iter().map(|e| e.name).collect();
    assert!(names.contains(&"device-phone-a.enc".to_owned()), "{names:?}");
    assert!(names.contains(&relay::file_name("pc-e2e")), "{names:?}");
    assert!(!names.iter().any(|n| n.contains("stray")), "非 device-*.enc 不该被列出来：{names:?}");
    assert!(
        names.iter().all(|n| n.starts_with("device-") && n.ends_with(".enc")),
        "{names:?}"
    );
    assert!(
        names.iter().all(|n| !n.contains('@') && !n.contains("example.com")),
        "邮箱不许进文件名：{names:?}"
    );

    // 第二次同步：没有本地写入 → 一句 PUT 都不发（M11b 点名的省配额钉子）。
    let out2 = relay::run_with(&c, "pc-e2e", SECRET, &r, &FixedNow(MS + 60_000));
    assert!(out2.ok);
    assert!(!out2.uploaded && out2.put_skipped, "pending==0 必须跳过 PUT");
    assert_eq!(out2.pending_after, 0);

    // 状态落进了 settings：界面那句「上次同步于」与 listing 的唯一来源
    let st = settings::get_json(&c, settings::KEY_SYNC_STATE)
        .unwrap()
        .expect("成功同步要留状态");
    assert_eq!(st.get("lastSyncAt").and_then(Value::as_i64), Some(MS + 60_000));
    assert!(st.get("lastError").map(Value::is_null).unwrap_or(true));
    let peers: Vec<&str> = st
        .get("peers")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    assert_eq!(peers, ["phone-a"]);
}

/// §6 M11b 点名的第一条：`dirty=0` 时跳过 PUT，有测试钉住。
/// 空库（pending 恰为 0，且远端还没有本机文件）：整条链跑完也不许产生 `device-<本机>.enc`。
#[test]
#[ignore = "需要桩在跑"]
fn a_quiet_machine_sends_nothing_but_the_listing_still_happens() {
    wipe_device_files();
    let r = remote();
    r.ensure_dir().unwrap();

    // 远端有别人的文件 → 还是要 GET 要合并；「跳过 PUT」不等于「跳过同步」
    let phone = phone_row("e2e-quiet-1", MS - 1_000, "phone-b", "静默期也要收得进来");
    r.put("device-phone-b.enc", &phone_envelope("phone-b", MS - 1_000, vec![phone]))
        .unwrap();

    let c = conn();
    assert_eq!(merge::pending(&c).unwrap(), 0, "夹具前提：没有本地欠账");
    let out = relay::run_with(&c, "pc-quiet", SECRET, &r, &FixedNow(MS));
    assert!(out.ok, "{}", out.error.unwrap_or_default());
    assert_eq!(out.applied, 1, "对端那份照常并进");
    assert!(!out.uploaded && out.put_skipped);

    let names: Vec<String> = r.list().unwrap().into_iter().map(|e| e.name).collect();
    assert!(names.contains(&"device-phone-b.enc".to_owned()), "{names:?}");
    assert!(
        !names.contains(&relay::file_name("pc-quiet")),
        "没有本地写入却 PUT 了：{names:?}"
    );
}

/// §6 M11b 点名的第二条 + §3.3 第 2 条：`clear_dirty` 用的是**那份快照自己的**
/// `sealed_at`，不许传 now。夹具让「PUT 在路上时新写一条」真的发生在 dump 之后，
/// 并让假时钟在这之后继续走 —— 若代码错把 now 传进 clear_dirty，混进去的行会被清掉。
struct ClockWritingBehind {
    peer: Connection,
    sealed: i64,
    calls: Cell<usize>,
    injected: Cell<bool>,
}

impl Now for ClockWritingBehind {
    fn ms(&self) -> i64 {
        let n = self.calls.get();
        self.calls.set(n + 1);
        match n {
            // 第 1 次：run_with 的 start
            0 => self.sealed - 10_000,
            // 第 2 次：dump 的 sealed_at。返回**之前**先模拟用户在序列化之后新写一条
            // （updated_at 比快照戳还新），它就是必须留在 outbox 里的那一条。
            1 => {
                if !self.injected.get() {
                    self.peer
                        .execute(
                            "INSERT INTO todos (id, rev, created_at, created_by, updated_at,
                                updated_by, title, sort_key, zone, completed, dirty)
                             VALUES ('e2e-midwrite',1,?1,'pc-mid',?1,'pc-mid','PUT路上写的',2048,'inbox',0,1)",
                            [self.sealed + 1_000],
                        )
                        .unwrap();
                    self.injected.set(true);
                }
                self.sealed
            }
            // 之后的任何调用都返回更大的墙钟：谁拿它去 clear_dirty，混进去的行就会被误清
            _ => self.sealed + 9_000_000,
        }
    }
}

#[test]
#[ignore = "需要桩在跑"]
fn a_row_written_after_the_snapshot_stamp_stays_pending() {
    wipe_device_files();
    let r = remote();
    r.ensure_dir().unwrap();

    // 这个用例需要两个连接摸同一个库，所以用临时文件库而不是 :memory:
    let dir = temp_dir("midwrite");
    let db_path = dir.join("app.db");
    let mut c = db::open(&db_path).expect("文件库");
    db::migrations::migrate(&mut c).unwrap();
    let peer = Connection::open(&db_path).unwrap();

    todos::create(
        &c,
        &FixedNow(MS),
        "pc-mid",
        TodoDraft {
            title: "快照里带着的".into(),
            ..Default::default()
        },
    )
    .unwrap();

    let clock = ClockWritingBehind {
        peer,
        sealed: MS,
        calls: Cell::new(0),
        injected: Cell::new(false),
    };
    let out = relay::run_with(&c, "pc-mid", SECRET, &r, &clock);
    assert!(out.ok, "{}", out.error.unwrap_or_default());
    assert!(out.uploaded, "有欠账才进得来这段");
    assert_eq!(out.sealed_at, Some(MS), "快照戳取的是第 2 次 now.ms()");
    assert_eq!(
        out.pending_after, 1,
        "updated_at 比快照戳新的那条必须还脏着（药箱 db.dart:246 的教训）"
    );
    let still: i64 = c
        .query_row("SELECT dirty FROM todos WHERE id = 'e2e-midwrite'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(still, 1);
    std::fs::remove_dir_all(&dir).ok();
}

/// §6 M11b：凭据被拒（403/401）只留一行状态，不 panic、不弹窗。
/// 桩对错误密码回 401 —— 与坚果云的真实行为同一类。
#[test]
#[ignore = "需要桩在跑"]
fn rejected_credentials_leave_one_line_and_no_panic() {
    let base = relay::sync_base_from_backup_base(&env("ANVIL_WEBDAV_BASE"));
    let r = relay::Remote::new(&base, &env("ANVIL_WDAV_USER"), "不对的密码").unwrap();
    let c = conn();
    todos::create(
        &c,
        &FixedNow(MS),
        "pc-deny",
        TodoDraft {
            title: "被拒了也别忘了传".into(),
            ..Default::default()
        },
    )
    .unwrap();

    let out = relay::run_with(&c, "pc-deny", SECRET, &r, &FixedNow(MS));
    assert!(!out.ok);
    assert!(!out.skipped, "被拒不等于没配");
    let line = out.error.clone().unwrap_or_default();
    assert!(line.contains("坚果云拒绝了凭据"), "{line}");
    assert!(!line.contains('\n'), "状态必须是一行话");
    assert_eq!(out.pending_after, 1, "被拒不改本机欠账");
    // 没成功就不许伪造「上次同步于」
    let st = relay::stored_state(&c).unwrap();
    assert!(st.get("lastSyncAt").and_then(Value::as_i64).is_none());
    assert_eq!(st.get("lastError").and_then(Value::as_str), Some(line.as_str()));
}

/// 「只有课表、没有待办」也要发布：否则手机永远拿不到课表。首次发完，课表没变就跳过。
#[test]
#[ignore = "需要桩在跑"]
fn classes_alone_trigger_a_publish() {
    wipe_device_files();
    let r = remote();
    r.ensure_dir().unwrap();

    let c = conn();
    c.execute(
        "INSERT INTO classes (id, rev, created_at, created_by, updated_at, updated_by,
                              deleted_at, term, name, teacher, room, day,
                              start_min, end_min, period, weeks)
         VALUES ('cls-only',1,?1,'pc-a',?1,'pc-a',NULL,'2026-2027-1','数据结构','','',3,480,580,'','1-16')",
        [MS],
    )
    .unwrap();
    assert_eq!(merge::pending(&c).unwrap(), 0, "夹具前提：没有待办欠账");

    let out = relay::run_with(&c, "pc-cls", SECRET, &r, &FixedNow(MS));
    assert!(out.ok, "{}", out.error.unwrap_or_default());
    assert!(out.uploaded && !out.put_skipped, "课表在库里就得发");

    let blob = r.get(&relay::file_name("pc-cls")).unwrap();
    let snap = Snapshot::from_bytes(&crypto::open(SECRET, &blob).unwrap()).unwrap();
    assert_eq!(snap.rows("classes").len(), 1, "课表要进快照");
    assert!(snap.rows("todos").is_empty());

    let out2 = relay::run_with(&c, "pc-cls", SECRET, &r, &FixedNow(MS + 60_000));
    assert!(!out2.uploaded && out2.put_skipped, "课表没变化就别重复发");
}
