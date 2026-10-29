//! M6e：待办提醒的取数与盖章。
//!
//! 「通知真的响了」只能人耳验收（Windows 通知中心不在测试里），这一层能证明的是它的前提：
//! 一条提醒**只会被取出一次**，改期能重新武装，做完或删掉就闭嘴。
//! 这两条只要错一条，用户看到的就是「提醒不响」或者「同一件事刷屏」。

use rusqlite::Connection;

use anvil_lib::db::migrations::migrate;
use anvil_lib::model::{TodoDraft, TodoPatch};
use anvil_lib::repos::todos;
use anvil_lib::time::{days, FixedNow, Now};

const TODAY: i32 = 20261028;
const MIN: i64 = 60_000;
const DEVICE: &str = "test-device";

fn db() -> Connection {
    let mut c = Connection::open_in_memory().expect("in-memory");
    migrate(&mut c).expect("migrate");
    c
}

fn at(hour: i64, minute: i64) -> FixedNow {
    FixedNow(days::day_start_ms(TODAY) + hour * 3600 * 1000 + minute * MIN)
}

fn clock(hour: i64, minute: i64) -> i64 {
    days::day_start_ms(TODAY) + hour * 3600 * 1000 + minute * MIN
}

fn add(conn: &Connection, now: &dyn Now, title: &str, remind_at: Option<i64>) -> String {
    todos::create(
        conn,
        now,
        DEVICE,
        TodoDraft {
            title: title.to_owned(),
            remind_at,
            ..Default::default()
        },
    )
    .expect("create")
    .stamp
    .id
}

fn due(conn: &Connection, now: &dyn Now, limit: i64) -> Vec<String> {
    todos::take_due_reminders(conn, now, DEVICE, limit)
        .expect("take")
        .into_iter()
        .map(|r| r.id)
        .collect()
}

#[test]
fn a_due_reminder_fires_exactly_once() {
    let conn = db();
    let now = at(9, 0);
    let id = add(&conn, &now, "交周报", Some(clock(8, 30)));

    assert_eq!(due(&conn, &now, 5), vec![id]);
    // 第二轮必须闭嘴：盖了章还不认，通知中心就变成刷屏机器
    assert!(due(&conn, &now, 5).is_empty());
    assert!(due(&conn, &at(18, 0), 5).is_empty());
}

#[test]
fn a_future_reminder_stays_quiet_until_it_is_due() {
    let conn = db();
    add(&conn, &at(9, 0), "牙医", Some(clock(15, 0)));

    assert!(due(&conn, &at(9, 30), 5).is_empty());
    assert_eq!(due(&conn, &at(15, 1), 5).len(), 1);
}

#[test]
fn moving_the_reminder_arms_it_again() {
    let conn = db();
    let now = at(9, 0);
    let id = add(&conn, &now, "交周报", Some(clock(8, 30)));
    assert_eq!(due(&conn, &now, 5).len(), 1);

    // 用户把提醒推到 10 点，就是想 10 点再被叫一次
    todos::update(
        &conn,
        &at(9, 5),
        DEVICE,
        &id,
        TodoPatch {
            remind_at: Some(Some(clock(10, 0))),
            ..Default::default()
        },
    )
    .expect("update");

    assert!(due(&conn, &at(9, 30), 5).is_empty());
    assert_eq!(due(&conn, &at(10, 30), 5), vec![id]);
}

#[test]
fn clearing_the_reminder_silences_it() {
    let conn = db();
    let now = at(9, 0);
    let id = add(&conn, &now, "顺手的事", Some(clock(8, 30)));

    todos::update(
        &conn,
        &now,
        DEVICE,
        &id,
        TodoPatch {
            remind_at: Some(None),
            ..Default::default()
        },
    )
    .expect("update");

    assert!(due(&conn, &at(23, 0), 5).is_empty());
}

#[test]
fn finishing_or_trashing_a_todo_silences_its_reminder() {
    let conn = db();
    let now = at(9, 0);
    let done = add(&conn, &now, "已经做完的", Some(clock(8, 30)));
    let gone = add(&conn, &now, "已经删掉的", Some(clock(8, 30)));

    todos::set_done(&conn, &now, DEVICE, &done, true).expect("set_done");
    todos::soft_delete(&conn, &now, DEVICE, &gone).expect("soft_delete");

    assert!(due(&conn, &now, 5).is_empty());
}

#[test]
fn the_batch_caps_the_queue_and_the_next_round_picks_up_the_rest() {
    let conn = db();
    let now = at(23, 0);
    // 一天没开机，攒下 7 条过期提醒
    for i in 0..7i64 {
        add(&conn, &now, &format!("补提醒 {i}"), Some(clock(8, i)));
    }

    assert_eq!(due(&conn, &now, 5).len(), 5);
    assert_eq!(due(&conn, &now, 5).len(), 2);
    assert!(due(&conn, &now, 5).is_empty());
}
