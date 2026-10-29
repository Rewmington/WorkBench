//! M5b：便签 upsert 与倒计时状态机。
//!
//! 倒计时最值得测的是「暂停」——0003 加 `remaining_ms` 就是为了它。
//! 一支 25 分钟的钟暂停 10 分钟再继续，如果剩余时间没冻住，界面会说只剩 15 分钟。

use rusqlite::Connection;
use serde_json::json;

use anvil_lib::db::migrations::migrate;
use anvil_lib::model::{cd_state, CountdownDraft};
use anvil_lib::repos::{countdowns, stickies};
use anvil_lib::time::{days, FixedNow, Now};

const MS: i64 = 1_793_170_800_000; // 2026-10-28T15:00+08:00
const TODAY: i32 = 20261028;
const MIN: i64 = 60_000;

fn db() -> Connection {
    let mut c = Connection::open_in_memory().expect("in-memory");
    migrate(&mut c).expect("migrate");
    c
}

fn noon(day: i32) -> i64 {
    days::day_start_ms(day) + 12 * 3600 * 1000
}

fn at(hour: i64, minute: i64) -> FixedNow {
    FixedNow(days::day_start_ms(TODAY) + hour * 3600 * 1000 + minute * MIN)
}

fn add(c: &Connection, label: &str, mode: &str, spec: serde_json::Value) -> String {
    countdowns::create(
        c,
        &FixedNow(MS),
        "pc-a",
        CountdownDraft { label: label.into(), mode: mode.into(), spec },
    )
    .expect("create")
    .stamp
    .id
}

// ---- deadline 折算 ----

#[test]
fn duration_counts_from_the_moment_it_was_started() {
    let d = json!({ "minutes": 25, "startedAt": MS });
    assert_eq!(countdowns::deadline_for("duration", &d, &FixedNow(MS)), Some(MS + 25 * MIN));
    // 已经跑过头了也报同一个时刻：到没到点是调用方比 now 的事，不在这里做判断
    assert_eq!(countdowns::deadline_for("duration", &d, &FixedNow(MS + 60 * MIN)), Some(MS + 25 * MIN));
}

#[test]
fn timed_uses_the_absolute_moment_as_given() {
    let d = json!({ "until": noon(20261101) });
    assert_eq!(countdowns::deadline_for("timed", &d, &FixedNow(MS)), Some(noon(20261101)));
    assert_eq!(countdowns::deadline_for("timed", &json!({}), &FixedNow(MS)), None);
}

#[test]
fn daily_picks_todays_slot_and_rolls_to_tomorrow_once_past() {
    let d = json!({ "at": "08:30" });
    // 15:00 时今天的 08:30 已经过了 → 明天 08:30
    let tomorrow = days::day_start_ms(days::next_day(TODAY)) + 8 * 3600 * 1000 + 30 * MIN;
    assert_eq!(countdowns::deadline_for("daily", &d, &at(15, 0)), Some(tomorrow));
    // 07:00 时今天的还没到
    assert_eq!(
        countdowns::deadline_for("daily", &d, &at(7, 0)),
        Some(days::day_start_ms(TODAY) + 8 * 3600 * 1000 + 30 * MIN)
    );
}

#[test]
fn daily_rejects_a_malformed_clock_instead_of_guessing() {
    let d = json!({ "at": "25:99" });
    assert_eq!(countdowns::deadline_for("daily", &d, &at(9, 0)), None);
    assert_eq!(countdowns::deadline_for("daily", &json!({ "at": 500 }), &at(9, 0)), None);
}

#[test]
fn interval_lands_on_the_next_multiple_after_now() {
    let started = at(9, 0).ms();
    let d = json!({ "everyMinutes": 45, "startedAt": started });
    assert_eq!(countdowns::deadline_for("interval", &d, &at(9, 10)), Some(started + 45 * MIN));
    assert_eq!(countdowns::deadline_for("interval", &d, &at(9, 45)), Some(started + 90 * MIN));
    assert_eq!(countdowns::deadline_for("interval", &d, &at(10, 0)), Some(started + 90 * MIN));
    // 每 0 分钟会把分母变成 0，夹到 1 而不是崩
    let zero = json!({ "everyMinutes": 0, "startedAt": started });
    assert!(countdowns::deadline_for("interval", &zero, &at(9, 10)).is_some());
}

// ---- 状态机 ----

#[test]
fn starting_stamps_the_origin_into_spec_so_it_survives_a_restore() {
    let c = db();
    let id = add(&c, "番茄", "duration", json!({ "minutes": 25 }));
    let started = countdowns::start(&c, &at(9, 0), "pc-a", &id).expect("start");
    assert_eq!(started.state, cd_state::RUNNING);
    assert_eq!(started.spec["startedAt"], json!(at(9, 0).ms()));
    assert_eq!(started.ends_at, Some(at(9, 0).ms() + 25 * MIN));
    // 没用的键不该被 json! 铺成一片 null
    assert!(started.spec.get("until").is_none(), "spec 里不该留 null 键：{:?}", started.spec);
}

#[test]
fn pausing_freezes_the_remaining_time_and_resuming_gives_it_back() {
    let c = db();
    let id = add(&c, "番茄", "duration", json!({ "minutes": 25 }));
    countdowns::start(&c, &at(9, 0), "pc-a", &id).expect("start");

    // 跑了 10 分钟按暂停
    let p = countdowns::pause(&c, &at(9, 10), "pc-a", &id).expect("pause");
    assert_eq!(p.state, cd_state::PAUSED);
    assert_eq!(p.remaining_ms, Some(15 * MIN), "暂停时必须冻住剩下的 15 分钟");

    // 10 分钟后才回来 —— 这 10 分钟不该算进倒计时
    let r = countdowns::resume(&c, &at(9, 20), "pc-a", &id).expect("resume");
    assert_eq!(r.state, cd_state::RUNNING);
    assert_eq!(r.ends_at, Some(at(9, 20).ms() + 15 * MIN));
    assert_eq!(r.remaining_ms, None, "跑起来了就没有冻住的剩余");
}

#[test]
fn pause_and_resume_are_idempotent() {
    let c = db();
    let id = add(&c, "番茄", "duration", json!({ "minutes": 5 }));
    let started = countdowns::start(&c, &at(9, 0), "pc-a", &id).expect("start");

    let again = countdowns::pause(&c, &at(9, 1), "pc-a", &id).expect("pause");
    let double = countdowns::pause(&c, &at(9, 2), "pc-a", &id).expect("pause");
    assert_eq!(double.remaining_ms, again.remaining_ms, "重复暂停不该把剩余改掉");
    assert_eq!(double.stamp.rev, again.stamp.rev, "没生效的操作不该推 rev");

    // idle 状态的钟没有暂停可言
    let idle = add(&c, "没启动过", "duration", json!({ "minutes": 5 }));
    let untouched = countdowns::pause(&c, &at(9, 3), "pc-a", &idle).expect("pause idle");
    assert_eq!(untouched.state, cd_state::IDLE);
    assert!(!untouched.paused);
    assert_eq!(started.state, cd_state::RUNNING);
}

#[test]
fn a_duration_rolls_to_finished_and_a_daily_rolls_to_its_next_slot() {
    let c = db();
    let done = add(&c, "短促", "duration", json!({ "minutes": 1 }));
    countdowns::start(&c, &at(9, 0), "pc-a", &done).expect("start");
    let listed = countdowns::list(&c, &at(9, 30)).expect("list");
    let d = listed.iter().find(|x| x.stamp.id == done).expect("在列表里");
    assert_eq!(d.state, cd_state::FINISHED);

    let every = add(&c, "喝水", "interval", json!({ "everyMinutes": 45 }));
    countdowns::start(&c, &at(9, 0), "pc-a", &every).expect("start");
    let listed = countdowns::list(&c, &at(10, 0)).expect("list");
    let w = listed.iter().find(|x| x.stamp.id == every).expect("在列表里");
    assert_eq!(w.ends_at, Some(at(9, 0).ms() + 90 * MIN), "interval 要滚到下一轮");
    assert_eq!(w.state, cd_state::RUNNING, "滚动型不会走到 finished");
}

#[test]
fn unknown_mode_is_refused_at_the_door() {
    let c = db();
    let err = countdowns::create(&c, &FixedNow(MS), "pc-a", CountdownDraft {
        label: "怪".into(),
        mode: "fortnite".into(),
        spec: json!({}),
    });
    assert!(err.is_err());
    let blank = countdowns::create(&c, &FixedNow(MS), "pc-a", CountdownDraft {
        label: "   ".into(),
        mode: "duration".into(),
        spec: json!({ "minutes": 5 }),
    });
    assert!(blank.is_err(), "没名字的倒计时在环上会变成一张无法辨认的卡");
}

// ---- 便签 ----

#[test]
fn sticky_save_is_an_upsert_on_the_slot() {
    let c = db();
    let first = stickies::save(&c, &at(9, 0), "pc-a", 1, "买猫粮").expect("save");
    let second = stickies::save(&c, &at(9, 5), "pc-a", 1, "买猫粮和罐头").expect("save");
    assert_eq!(first.stamp.id, second.stamp.id, "同一个槽位不该长出第二行");
    assert_eq!(second.stamp.rev, 2);
    assert_eq!(stickies::list(&c).expect("list").len(), 1);
}

#[test]
fn an_unchanged_save_does_not_bump_the_rev() {
    // 前端 600 ms 防抖，多余的一次落盘不该在「最后写入」上留下痕迹
    let c = db();
    stickies::save(&c, &at(9, 0), "pc-a", 2, "同一段话").expect("first");
    let again = stickies::save(&c, &at(9, 9), "pc-a", 2, "同一段话").expect("second");
    assert_eq!(again.stamp.rev, 1);
    assert_eq!(again.stamp.updated_at, at(9, 0).ms());
}

#[test]
fn blank_content_keeps_the_row_but_clear_removes_it() {
    let c = db();
    stickies::save(&c, &at(9, 0), "pc-a", 3, "").expect("save blank");
    assert_eq!(stickies::list(&c).expect("list").len(), 1, "清空正文是「此刻没写字」，不是「这个槽位不存在」");

    assert!(stickies::clear(&c, &at(9, 1), "pc-a", 3).expect("clear"));
    assert!(stickies::list(&c).expect("list").is_empty());
    // 清掉之后同一个槽位可以重新占
    let fresh = stickies::save(&c, &at(9, 2), "pc-a", 3, "新的").expect("re-save");
    assert_eq!(fresh.stamp.rev, 1, "槽位腾出来后是一条新记录");
    assert!(!stickies::clear(&c, &at(9, 3), "pc-a", 42).expect("clear empty slot"), "空槽位没什么可清");
}

#[test]
fn slots_are_unique_among_live_rows_only() {
    let c = db();
    stickies::save(&c, &at(9, 0), "pc-a", 1, "a").expect("a");
    stickies::clear(&c, &at(9, 1), "pc-a", 1).expect("clear");
    stickies::save(&c, &at(9, 2), "pc-a", 1, "b").expect("b");
    // 墓碑行 + 新行同时存在，靠的是 uq_sticky_slot 的部分索引只约束活行
    let live = stickies::list(&c).expect("list");
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].content, "b");
}
