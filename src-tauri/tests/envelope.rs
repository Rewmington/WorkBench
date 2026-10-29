//! M4 验收：信封格式（头部 + Argon2id + AES-GCM + AAD）与「备份产物里到底有什么」。
//!
//! 这里跑的是真 SQL 导出的库，不是假字节 —— 备份最怕的不是加密写错，
//! 而是把药箱健康数据和本机绝对路径一起传上云。

mod common;
use common::{cleanup, count, gunzip, lived_in, temp_dir, SECRET};

use rusqlite::Connection;

use anvil_lib::backup::{crypto, export, file_name};
use anvil_lib::db;
use anvil_lib::model::TodoDraft;
use anvil_lib::repos::todos;
use anvil_lib::time::SystemNow;

#[test]
fn uploaded_artifact_keeps_todos_and_drops_everything_personal() {
    let tmp = temp_dir("envelope-scrub");
    let mut conn = db::open_in_memory().expect("内存库");
    lived_in(&mut conn, 3);

    let plain = export(&conn, &tmp, common::MS).expect("导出");
    let sealed = crypto::seal(SECRET, &plain).expect("封装");
    assert_eq!(
        crypto::open(SECRET, &sealed).expect("打开"),
        plain,
        "解开的必须就是导出去的那份 gzip"
    );
    let restored = gunzip(&plain);

    // 落盘再打开：integrity_check 与查询都要在真文件上跑
    let file = tmp.join("restored.db");
    std::fs::write(&file, &restored).unwrap();
    let probe = Connection::open(&file).expect("还原出来的必须是合法库");
    assert_eq!(
        probe
            .query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "ok"
    );

    assert_eq!(count(&probe, "SELECT count(*) FROM todos"), 3, "待办是备份的意义所在");
    assert_eq!(
        count(&probe, "SELECT count(*) FROM linked_snapshots"),
        0,
        "药箱库存不跟着上云（夹具里那条是云端来源的，一样得剥掉）"
    );
    assert_eq!(count(&probe, "SELECT count(*) FROM backup_log"), 0, "本机流水不该跨机器");
    assert_eq!(
        count(
            &probe,
            "SELECT count(*) FROM settings WHERE key IN ('linked.blog.repoDir','linked.medicine.dbPath','webdav.email','linked.medicine.email')"
        ),
        0,
        "绝对路径与两个邮箱都不能出门"
    );
    assert_eq!(
        count(&probe, "SELECT count(*) FROM settings"),
        2,
        "主题与药箱同步目录都是机器无关的，还原后仍然有效，该留着"
    );

    cleanup(&tmp);
}

#[test]
fn wrong_password_is_a_tag_rejection_not_garbage() {
    let plain = b"SQLite format 3\0 fake".to_vec();
    let sealed = crypto::seal(SECRET, &plain).unwrap();

    let err = crypto::open("stub@example.com:typo", &sealed).unwrap_err();
    assert!(err.to_string().contains("解密失败"), "{err}");

    // 密文最后一个字节翻一位：GCM 标签必须拦住，而不是解出错字节
    let mut flipped = sealed.clone();
    let last = flipped.len() - 1;
    flipped[last] ^= 0x01;
    assert!(crypto::open(SECRET, &flipped).is_err(), "篡改密文必须被标签拒绝");
}

#[test]
fn header_swap_and_truncation_are_refused() {
    let a = crypto::seal(SECRET, b"AAA").unwrap();
    let b = crypto::seal(SECRET, b"BBB").unwrap();

    // 把 B 的 salt 换进 A 的头部：AAD 覆盖 salt，所以解不开
    let salt_at = 8 + 2 + 4 + 4 + 1;
    let mut swapped = a.clone();
    swapped[salt_at..salt_at + 16].copy_from_slice(&b[salt_at..salt_at + 16]);
    assert!(crypto::open(SECRET, &swapped).is_err(), "换 salt 必须检测出来");

    for cut in [8usize, 30, a.len() - 1] {
        assert!(
            crypto::open(SECRET, &a[..cut]).is_err(),
            "截断到 {cut} 字节不该能打开"
        );
    }
}

#[test]
fn a_hundred_kilobyte_database_gzips_and_round_trips() {
    let tmp = temp_dir("envelope-big");
    let mut conn = db::open_in_memory().expect("内存库");
    lived_in(&mut conn, 1500);

    let plain = export(&conn, &tmp, common::MS).expect("导出");
    let db_bytes = gunzip(&plain);
    assert!(
        db_bytes.len() > 100 * 1024,
        "这个夹具应该产出 >100 KB 的库，实际 {} B",
        db_bytes.len()
    );
    assert!(
        plain.len() * 2 < db_bytes.len(),
        "gzip 没压下去：{} B → {} B",
        db_bytes.len(),
        plain.len()
    );

    let sealed = crypto::seal(SECRET, &plain).unwrap();
    assert!(
        sealed.len() >= plain.len() && sealed.len() - plain.len() < 80,
        "信封只该多出头部与标签：{} → {}",
        plain.len(),
        sealed.len()
    );
    assert_eq!(
        gunzip(&crypto::open(SECRET, &sealed).unwrap()),
        db_bytes,
        "往返必须逐字节一致"
    );

    cleanup(&tmp);
}

#[test]
fn export_is_a_consistent_snapshot_of_a_file_database() {
    // VACUUM INTO 在 WAL 下取的是已提交的一致快照：导出之后新写的事务不该混进来
    let tmp = temp_dir("envelope-wal");
    let mut conn = db::open(&tmp.join("app.db")).expect("文件库");
    lived_in(&mut conn, 20);

    let plain = export(&conn, &tmp, common::MS).expect("导出");
    todos::create(
        &conn,
        &SystemNow,
        "pc-a",
        TodoDraft {
            title: "导出之后才加的".to_owned(),
            ..Default::default()
        },
    )
    .expect("create");

    let sealed = crypto::seal(SECRET, &plain).unwrap();
    let restored = gunzip(&crypto::open(SECRET, &sealed).unwrap());
    let probe_file = tmp.join("probe.db");
    std::fs::write(&probe_file, restored).unwrap();
    let probe = Connection::open(&probe_file).unwrap();
    assert_eq!(
        count(&probe, "SELECT count(*) FROM todos"),
        20,
        "快照必须是导出那一刻的样子"
    );

    cleanup(&tmp);
}

#[test]
fn file_name_is_the_only_handshake_the_remote_sees() {
    let sealed = crypto::seal(SECRET, b"payload").unwrap();
    let name = file_name(common::MS, &sealed);
    assert!(is_expected_name(&name), "{name} 不符合 anvil-<UTC>-<8hex>.abackup");
    assert!(!is_expected_name("device-pc1.json"));
    assert!(!is_expected_name("anvil-20251028T221320Z-12345678.zip"));
}

/// `anvil-YYYYMMDDTHHMMSSZ-<8 hex>.abackup`，不引 regex crate，按位置判。
fn is_expected_name(name: &str) -> bool {
    let Some(stem) = name
        .strip_prefix("anvil-")
        .and_then(|s| s.strip_suffix(".abackup"))
    else {
        return false;
    };
    let Some((stamp, tag)) = stem.split_once('-') else {
        return false;
    };
    let bytes = stamp.as_bytes();
    let stamp_ok = bytes.len() == 16
        && bytes[8] == b'T'
        && bytes[15] == b'Z'
        && bytes
            .iter()
            .enumerate()
            .all(|(i, b)| matches!(i, 8 | 15) || b.is_ascii_digit());
    stamp_ok && tag.len() == 8 && tag.chars().all(|c| c.is_ascii_hexdigit())
}

#[test]
fn golden_blob_still_decrypts() {
    // 离线解密证明：这份 blob 是用当前实现一次性产出的。任何人改动信封布局
    // （加字段、换 KDF 参数顺序、缩窄 AAD 范围）都会让这条测试立刻红 ——
    // 而真正的代价是几个月后那份存在坚果云里的快照解不开。
    use base64::{engine::general_purpose::STANDARD, Engine};
    let blob = STANDARD.decode(GOLDEN_BLOB).expect("golden blob 是合法 base64");
    assert_eq!(&blob[..8], b"ANVILBKP");
    let plain = crypto::open(GOLDEN_SECRET, &blob).expect("golden blob 必须永远解得开");
    assert_eq!(plain, GOLDEN_PLAINTEXT);
}

const GOLDEN_SECRET: &str = "golden@wmddd.online:golden-pass";
const GOLDEN_PLAINTEXT: &[u8] = b"anvil envelope golden plaintext";
const GOLDEN_BLOB: &str = "QU5WSUxCS1ABAQABAAAAAAADAQfhNxvWmZfHL7frIXLkxx3EaScAE+BqlZyV2mGtl21M280HaeyJTdK9SiJBuTFGbGou5KEh+cKMDaLTcxxP53WCd52JHgiQK07dZw==";

/// 信封格式变更时用一次，重新产出上面的 GOLDEN_BLOB：
/// `cargo test --test envelope emit_golden -- --ignored`
#[test]
#[ignore]
fn emit_golden() {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let blob = crypto::seal(GOLDEN_SECRET, GOLDEN_PLAINTEXT).unwrap();
    println!("GOLDEN={}", STANDARD.encode(blob));
}


