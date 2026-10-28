//! 「拖 exe 出图标」这条验收里，能自动化的部分：路径 → 类型判定 → 图标提取 →
//! PNG 落盘 → 库里可查。真正无法自动化的只有「用鼠标从 Explorer 拖进窗口」那一下
//! （注入系统级拖拽会打到用户当时正在用的窗口），那部分由 Tauri 的 onDragDropEvent
//! 直接调用这里的同一个函数，路径一致。

use anvil_lib::repos::resources;
use anvil_lib::time::{FixedNow, Now};
use anvil_lib::{db, win32};

fn notepad() -> Option<String> {
    let root = std::env::var("SystemRoot").ok()?;
    let path = std::path::Path::new(&root)
        .join("System32")
        .join("notepad.exe");
    path.exists().then(|| path.display().to_string())
}

#[test]
fn adding_a_real_exe_stores_an_icon_that_can_be_read_back() {
    let Some(target) = notepad() else {
        eprintln!("跳过：这台机器上没有 notepad.exe");
        return;
    };

    let dir = tempfile::tempdir().unwrap();
    let icons = dir.path().join("icons");
    std::fs::create_dir_all(&icons).unwrap();

    let bytes = win32::icons::extract_png(&target).expect("提取图标");
    let file = icons.join("notepad.png");
    std::fs::write(&file, &bytes).unwrap();

    assert!(file.exists());
    assert_eq!(&bytes[..8], &[0x89, b'P', b'N', b'G', 13, 10, 26, 10]);

    let mut conn = db::open_in_memory().unwrap();
    db::migrations::migrate(&mut conn).unwrap();
    let now = FixedNow(1_764_000_000_000);

    let row = resources::create(
        &conn,
        &now,
        "pc-test",
        resources::ResourceDraft {
            kind: "app".into(),
            name: "记事本".into(),
            target: target.clone(),
            args: String::new(),
            icon_path: Some(file.display().to_string()),
            zone_id: None,
            note: String::new(),
            secret: None,
        },
    )
    .unwrap();

    let listed = resources::list(&conn).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].icon_path.as_deref(),
        Some(file.display().to_string().as_str())
    );
    assert_eq!(listed[0].use_count, 0);

    // 启动一次之后，「常用」排序的依据才存在
    resources::record_launch(&conn, &FixedNow(now.ms() + 1000), "pc-test", &row.stamp.id).unwrap();
    assert_eq!(resources::list(&conn).unwrap()[0].use_count, 1);
}

#[test]
fn a_target_that_no_longer_exists_is_refused_at_add_time() {
    let mut conn = db::open_in_memory().unwrap();
    db::migrations::migrate(&mut conn).unwrap();
    let now = FixedNow(1);

    let err = resources::create(
        &conn,
        &now,
        "pc-test",
        resources::ResourceDraft {
            kind: "app".into(),
            name: "不存在".into(),
            target: r"Z:\gone\nothing.exe".into(),
            args: String::new(),
            icon_path: None,
            zone_id: None,
            note: String::new(),
            secret: None,
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("不存在"), "{err}");
}

#[test]
fn web_targets_skip_the_filesystem_check() {
    let mut conn = db::open_in_memory().unwrap();
    db::migrations::migrate(&mut conn).unwrap();
    let row = resources::create(
        &conn,
        &FixedNow(1),
        "pc-test",
        resources::ResourceDraft {
            kind: "web".into(),
            name: "博客".into(),
            target: "https://wmddd.online".into(),
            args: String::new(),
            icon_path: None,
            zone_id: None,
            note: String::new(),
            secret: None,
        },
    )
    .unwrap();
    assert_eq!(row.kind, "web");
}

#[test]
fn secret_note_is_encrypted_at_rest_and_readable_on_this_machine() {
    let mut conn = db::open_in_memory().unwrap();
    db::migrations::migrate(&mut conn).unwrap();
    let now = FixedNow(1);

    let row = resources::create(
        &conn,
        &now,
        "pc-test",
        resources::ResourceDraft {
            kind: "web".into(),
            name: "路由器".into(),
            target: "http://192.168.1.1".into(),
            args: String::new(),
            icon_path: None,
            zone_id: None,
            note: String::new(),
            secret: Some("admin / 某密码".into()),
        },
    )
    .unwrap();

    // 库里那一列不该看得见明文
    let raw: String = conn
        .query_row(
            "SELECT secret_cipher FROM resources WHERE id = ?1",
            rusqlite::params![row.stamp.id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(raw.starts_with("v1:"), "密文该带版本与 key_id 前缀：{raw}");
    assert!(!raw.contains("admin"), "明文不该出现在库里");

    assert_eq!(
        resources::reveal_secret(&conn, &row.stamp.id)
            .unwrap()
            .as_deref(),
        Some("admin / 某密码")
    );
}

#[test]
fn zones_list_counts_only_live_items() {
    let mut conn = db::open_in_memory().unwrap();
    db::migrations::migrate(&mut conn).unwrap();
    let now = FixedNow(1);

    let zone = resources::add_zone(&conn, &now, "pc-test", "开发").unwrap();
    for name in ["VS Code", "终端"] {
        resources::create(
            &conn,
            &now,
            "pc-test",
            resources::ResourceDraft {
                kind: "web".into(),
                name: name.into(),
                target: "https://example.com".into(),
                args: String::new(),
                icon_path: None,
                zone_id: Some(zone.id.clone()),
                note: String::new(),
                secret: None,
            },
        )
        .unwrap();
    }
    assert_eq!(resources::list_zones(&conn).unwrap()[0].item_count, 2);

    // 删分区不能连带删掉用户攒的条目：条目落回未分区
    resources::delete_zone(&conn, &now, "pc-test", &zone.id).unwrap();
    assert!(resources::list_zones(&conn).unwrap().is_empty());
    assert_eq!(resources::list(&conn).unwrap().len(), 2);
    assert!(resources::list(&conn)
        .unwrap()
        .iter()
        .all(|r| r.zone_id.is_none()));
}
