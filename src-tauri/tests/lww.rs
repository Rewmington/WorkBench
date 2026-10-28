//! M1 验收：rev / updated_at / 墓碑 / 取消勾选不清 done_at。
//! 全部用 FixedNow，任何一条都不依赖机器时钟。

use rusqlite::Connection;

use anvil_lib::db;
use anvil_lib::model::{zones, Todo, TodoDraft, TodoPatch};
use anvil_lib::repos::{tags, todos};
use anvil_lib::sync::lww::is_newer;
use anvil_lib::time::{days, FixedNow};

const A: &str = "pc-a";
const B: &str = "pc-b";

fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
    days::day_start_ms(y * 10000 + mo as i32 * 100 + d as i32)
        + i64::from(h) * 3_600_000
        + i64::from(mi) * 60_000
}

fn setup() -> Connection {
    let mut conn = db::open_in_memory().expect("内存库");
    db::migrations::migrate(&mut conn).expect("迁移");
    conn
}

fn create(conn: &Connection, ms: i64, dev: &str, title: &str) -> Todo {
    todos::create(
        conn,
        &FixedNow(ms),
        dev,
        TodoDraft {
            title: title.to_owned(),
            ..Default::default()
        },
    )
    .expect("create")
}

fn order(conn: &Connection) -> Vec<String> {
    todos::list(conn, zones::INBOX)
        .unwrap()
        .into_iter()
        .map(|t| t.title)
        .collect()
}

#[test]
fn create_stamps_rev_one_and_local_day_hour() {
    let conn = setup();
    let todo = create(&conn, at(2026, 10, 28, 23, 30), A, "写周报");

    assert_eq!(todo.stamp.rev, 1);
    assert_eq!(todo.stamp.created_by, A);
    assert_eq!(todo.stamp.updated_by, A);
    assert_eq!(todo.done_at, None);
    assert!(todo.stamp.deleted_at.is_none());
    // 本地 23:30 建的：日与小时都按本地算，不受 UTC 偏移影响
    assert_eq!(todo.created_day, Some(20261028));
    assert_eq!(todo.created_hour, Some(23));
    assert_eq!(todo.zone, zones::INBOX);
}

#[test]
fn updates_bump_rev_and_move_the_writer() {
    let conn = setup();
    let base = at(2026, 10, 28, 9, 0);
    let todo = create(&conn, base, A, "买猫粮");

    let patched = todos::update(
        &conn,
        &FixedNow(base + 1_000),
        B,
        &todo.stamp.id,
        TodoPatch {
            priority: Some(0),
            ..Default::default()
        },
    )
    .expect("update 1");
    assert_eq!(patched.stamp.rev, 2);
    assert_eq!(patched.stamp.updated_by, B);
    assert_eq!(patched.stamp.created_by, A, "created_by 不随改写");
    assert_eq!(patched.priority, 0);

    let again = todos::update(
        &conn,
        &FixedNow(base + 2_000),
        A,
        &todo.stamp.id,
        TodoPatch {
            title: Some("买猫粮（大袋）".into()),
            ..Default::default()
        },
    )
    .expect("update 2");
    assert_eq!(again.stamp.rev, 3);
    assert_eq!(again.stamp.updated_at, base + 2_000);
}

#[test]
fn patch_can_clear_a_field_rather_than_leave_it() {
    let conn = setup();
    let base = at(2026, 10, 28, 9, 0);
    let due = at(2026, 10, 30, 18, 0);
    let todo = todos::create(
        &conn,
        &FixedNow(base),
        A,
        TodoDraft {
            title: "交税".into(),
            due_at: Some(due),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(todo.due_at, Some(due));

    let cleared = todos::update(
        &conn,
        &FixedNow(base + 1),
        A,
        &todo.stamp.id,
        TodoPatch {
            due_at: Some(None),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(cleared.due_at, None, "Some(None) 是清空，不是不改");
}

#[test]
fn tie_on_updated_at_is_resolved_by_device_id() {
    let same = at(2026, 10, 28, 12, 0);
    assert!(is_newer((same, B), (same, A)));
    assert!(!is_newer((same, A), (same, B)));
    assert!(!is_newer((same, A), (same, A)), "自己不该赢自己");
    assert!(
        !is_newer((100, "pc-z"), (900, "pc-a")),
        "设备 id 大不能翻越时间戳"
    );
}

#[test]
fn tombstones_hide_rows_and_restore_brings_them_back() {
    let conn = setup();
    let base = at(2026, 10, 28, 9, 0);
    let todo = create(&conn, base, A, "删我");

    let affected = todos::soft_delete(&conn, &FixedNow(base + 5), B, &todo.stamp.id).unwrap();
    assert_eq!(affected, vec![todo.stamp.id.clone()]);
    assert!(order(&conn).is_empty(), "墓碑行不得出现在列表里");
    assert!(todos::get(&conn, &todo.stamp.id).unwrap().is_none());

    let raw: Option<i64> = conn
        .query_row(
            "SELECT deleted_at FROM todos WHERE id = ?1",
            rusqlite::params![todo.stamp.id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(raw, Some(base + 5), "行还在，只是带墓碑");

    todos::restore(&conn, &FixedNow(base + 9), B, &affected).unwrap();
    let revived = todos::get(&conn, &todo.stamp.id).unwrap().expect("还原");
    assert!(revived.stamp.deleted_at.is_none());
    assert_eq!(revived.stamp.rev, 3, "删除 bump 一次，还原再 bump 一次");
}

#[test]
fn unchecking_keeps_done_at_and_only_bumps_rev() {
    let conn = setup();
    let base = at(2026, 10, 28, 9, 0);
    let todo = create(&conn, base, A, "刷牙");

    let done = todos::set_done(&conn, &FixedNow(base + 60_000), A, &todo.stamp.id, true).unwrap();
    assert_eq!(done.done_at, Some(base + 60_000));
    assert_eq!(done.done_day, Some(20261028));
    assert_eq!(done.stamp.rev, 2);

    let undone =
        todos::set_done(&conn, &FixedNow(base + 120_000), B, &todo.stamp.id, false).unwrap();
    // PLAN §5.1 点名的语义：取消勾选不清空完成时间，否则历史统计少一格
    assert_eq!(undone.done_at, Some(base + 60_000));
    assert_eq!(undone.done_day, Some(20261028));
    assert_eq!(undone.stamp.rev, 3);
    assert_eq!(undone.stamp.updated_by, B);

    let again = todos::set_done(&conn, &FixedNow(base + 180_000), A, &todo.stamp.id, true).unwrap();
    assert_eq!(
        again.done_at,
        Some(base + 180_000),
        "重新勾选要刷新完成时间"
    );
}

#[test]
fn completion_late_in_the_evening_lands_on_the_local_day() {
    let conn = setup();
    let late = at(2026, 10, 28, 23, 50);
    let todo = create(&conn, at(2026, 10, 27, 8, 0), A, "夜里补的活");
    let done = todos::set_done(&conn, &FixedNow(late), A, &todo.stamp.id, true).unwrap();
    assert_eq!(done.done_day, Some(20261028));
    assert_eq!(days::day_of_ms(late), 20261028);
}

#[test]
fn reorder_inserts_between_neighbours_and_rebalances_without_losing_order() {
    let conn = setup();
    let base = at(2026, 10, 28, 9, 0);
    let mut ids = Vec::new();
    for i in 0..4 {
        ids.push(create(&conn, base, A, &format!("第 {i} 项")).stamp.id);
    }
    let keys = || -> Vec<f64> {
        todos::list(&conn, zones::INBOX)
            .unwrap()
            .into_iter()
            .map(|t| t.sort_key)
            .collect()
    };
    assert_eq!(keys(), vec![1024.0, 2048.0, 3072.0, 4096.0]);

    // 一直把第 4 项塞到第 1、2 项之间：同一道缝隙反复二分，最终必然撞浮点精度
    for _ in 0..60 {
        todos::reorder(
            &conn,
            &FixedNow(base),
            A,
            &ids[3],
            Some(&ids[0]),
            Some(&ids[1]),
        )
        .unwrap();
    }
    let after = keys();
    assert_eq!(after.len(), 4);
    assert!(
        after.windows(2).all(|w| w[0] < w[1]),
        "重排后键必须严格递增：{after:?}"
    );
    assert_eq!(
        order(&conn),
        vec!["第 0 项", "第 3 项", "第 1 项", "第 2 项"],
        "反复插队后顺序仍要如实反映最后一次拖动"
    );
}

#[test]
fn deleting_a_parent_takes_the_children_with_it() {
    let conn = setup();
    let base = at(2026, 10, 28, 9, 0);
    let parent = create(&conn, base, A, "装修");
    let child = todos::create(
        &conn,
        &FixedNow(base),
        A,
        TodoDraft {
            title: "买漆".into(),
            parent_id: Some(parent.stamp.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();
    let grand = todos::create(
        &conn,
        &FixedNow(base),
        A,
        TodoDraft {
            title: "买滚筒".into(),
            parent_id: Some(child.stamp.id.clone()),
            ..Default::default()
        },
    )
    .unwrap();

    let mut affected = todos::soft_delete(&conn, &FixedNow(base + 1), A, &parent.stamp.id).unwrap();
    affected.sort();
    let mut want = vec![
        parent.stamp.id.clone(),
        child.stamp.id.clone(),
        grand.stamp.id.clone(),
    ];
    want.sort();
    assert_eq!(affected, want, "孙代也得跟着进墓碑，撤销时一起还原");

    todos::restore(&conn, &FixedNow(base + 2), A, &affected).unwrap();
    assert_eq!(todos::list(&conn, zones::INBOX).unwrap().len(), 3);
    assert_eq!(todos::children(&conn, &child.stamp.id).unwrap().len(), 1);
}

#[test]
fn interleaved_writers_keep_rev_and_clock_monotonic() {
    let conn = setup();
    let base = at(2026, 10, 28, 9, 0);
    let todo = create(&conn, base, A, "两人改");

    let mut last = todo.stamp.updated_at;
    let mut rev = todo.stamp.rev;
    for i in 1..=12i64 {
        let dev = if i % 2 == 0 { B } else { A };
        let ms = base + i * 10;
        let updated = todos::update(
            &conn,
            &FixedNow(ms),
            dev,
            &todo.stamp.id,
            TodoPatch {
                note: Some(Some(format!("第 {i} 次改"))),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(updated.stamp.updated_at > last, "时间戳必须单调");
        assert_eq!(updated.stamp.rev, rev + 1, "每次写入恰好 +1");
        last = updated.stamp.updated_at;
        rev = updated.stamp.rev;
    }
}

#[test]
fn tags_are_trimmed_deduped_and_replaceable() {
    let conn = setup();
    let base = at(2026, 10, 28, 9, 0);
    let todo = todos::create(
        &conn,
        &FixedNow(base),
        A,
        TodoDraft {
            title: "贴标签".into(),
            tags: vec!["家里".into(), " 家里 ".into(), "  ".into()],
            ..Default::default()
        },
    )
    .unwrap();

    let listed = todos::get(&conn, &todo.stamp.id).unwrap().unwrap();
    assert_eq!(
        listed.tags,
        vec!["家里".to_owned()],
        "同名标签只算一次，空名丢掉"
    );

    tags::set_todo_tags(
        &conn,
        &FixedNow(base + 1),
        A,
        &todo.stamp.id,
        &["工作".into()],
    )
    .unwrap();
    let after = todos::get(&conn, &todo.stamp.id).unwrap().unwrap();
    assert_eq!(
        after.tags,
        vec!["工作".to_owned()],
        "整组替换：旧的软删，新的挂上"
    );
}

#[test]
fn empty_title_is_refused_rather_than_stored() {
    let conn = setup();
    let err = todos::create(
        &conn,
        &FixedNow(at(2026, 10, 28, 9, 0)),
        A,
        TodoDraft {
            title: "   ".into(),
            ..Default::default()
        },
    )
    .unwrap_err();
    assert!(err.to_string().contains("标题"), "报错要能直接进 UI：{err}");
}

#[test]
fn search_matches_title_and_note_but_skips_tombstones() {
    let conn = setup();
    let base = at(2026, 10, 28, 9, 0);
    let hit = todos::create(
        &conn,
        &FixedNow(base),
        A,
        TodoDraft {
            title: "预约牙医".into(),
            note: "下周三之前".into(),
            ..Default::default()
        },
    )
    .unwrap();
    create(&conn, base, A, "无关项");
    let gone = create(&conn, base, A, "牙医报销");
    todos::soft_delete(&conn, &FixedNow(base), A, &gone.stamp.id).unwrap();

    let found = todos::search(&conn, "牙医", 10).unwrap();
    assert_eq!(found.len(), 1, "墓碑行不该被搜到");
    assert_eq!(found[0].stamp.id, hit.stamp.id);
}
