//! M5 首屏聚合与「下一步建议」规则引擎。
//!
//! 时间全部走 `FixedNow`：这些断言要能在任何时区、任何一天跑出同一个结果，
//! 否则「改系统时间后界面跟着变」那条验收就没有对照物。

use rusqlite::{params, Connection};

use anvil_lib::db::migrations::migrate;
use anvil_lib::model::todo::zones;
use anvil_lib::model::TodoDraft;
use anvil_lib::repos::{dashboard, todos};
use anvil_lib::suggest;
use anvil_lib::time::{days, FixedNow};

/// 2026-10-28T15:00+08:00，本地日界 20261028。与 tests/common 的夹具同一时刻。
const MS: i64 = 1_793_170_800_000;
const TODAY: i32 = 20261028;

fn db() -> Connection {
    let mut c = Connection::open_in_memory().expect("in-memory");
    migrate(&mut c).expect("migrate");
    c
}

fn noon(day: i32) -> i64 {
    days::day_start_ms(day) + 12 * 3600 * 1000
}

/// 相对今天偏移 N 个自然日。**必须走 `shift`**：`yyyymmdd` 整数相加会造出
/// `20261033` 这种日子，`day_start_ms` 拿不到时刻只能回 0（1970 年），
/// 「5 天后」就会变成「逾期最久」—— `day_start_ms` 里的 debug_assert 就是为拦这个。
fn rel(n: i64) -> i32 {
    days::shift(TODAY, n)
}

/// 建一条待办，`at` 是它的**创建**时刻。
fn add(c: &Connection, title: &str, at: i64, patch: impl FnOnce(&mut TodoDraft)) -> String {
    let mut draft = TodoDraft { title: title.into(), ..Default::default() };
    patch(&mut draft);
    todos::create(c, &FixedNow(at), "pc-a", draft).expect("create").stamp.id
}

fn finish(c: &Connection, id: &str, at: i64) {
    todos::set_done(c, &FixedNow(at), "pc-a", id, true).expect("done");
}

fn due(d: i32) -> impl FnOnce(&mut TodoDraft) {
    move |t| t.due_at = Some(noon(d))
}
fn in_zone(z: &'static str) -> impl FnOnce(&mut TodoDraft) {
    move |t| t.zone = Some(z.into())
}

/// 插一条倒计时。M5b 才有 repo，这里直接写表 —— 规则读的就是这张表。
fn countdown(c: &Connection, label: &str, state: &str, paused: bool, ends_at: i64) {
    c.execute(
        "INSERT INTO countdowns (id, rev, created_at, created_by, updated_at, updated_by,
                                 label, mode, spec, ends_at, paused, state)
         VALUES ('c1',1,1,'pc-a',1,'pc-a',?1,'duration','{}',?2,?3,?4)",
        params![label, ends_at, paused as i64, state],
    )
    .expect("insert countdown");
}

/// 塞一条药箱缓存。`status` 用 'ok' 才算读到了，missing/error 一律不当数。
fn medicine(c: &Connection, status: &str, payload: &str) {
    c.execute(
        "INSERT INTO linked_snapshots (source, kind, fetched_at, sig, status, detail, payload_json)
         VALUES ('medicine','today',1,'s',?1,'',?2)",
        params![status, payload],
    )
    .expect("insert snapshot");
}

#[test]
fn kpi_counts_use_local_day_boundaries() {
    let c = db();
    add(&c, "今天到期", MS, due(rel(0)));
    add(&c, "昨天到期", MS, due(rel(-1)));
    add(&c, "明天到期", MS, due(rel(1)));
    add(&c, "排进今天", MS, in_zone(zones::TODAY));
    add(&c, "随手记", MS, in_zone(zones::INBOX));

    let s = dashboard::summary(&c, &FixedNow(MS), 8).expect("summary");
    assert_eq!(s.today.value, 2, "今天到期 + 排进今天");
    assert_eq!(s.late.value, 1);
    assert_eq!(s.done.value, 0);
    // 建条目默认就落在收件箱（todos::create 的 zone 兜底），所以定了日子的三条
    // 「既是今天要做的、也还是未整理的」—— 两个 KPI 各答各的问题，不该互斥。
    assert_eq!(s.inbox.value, 4);
    assert_eq!(s.inbox.note.as_deref(), Some("近 7 天新捕 4 条"));
}

#[test]
fn done_is_the_only_kpi_with_a_delta_and_it_compares_yesterday() {
    let c = db();
    let y = days::prev_day(TODAY);
    let d1 = add(&c, "昨天完成 1", MS, |_| {});
    let d2 = add(&c, "昨天完成 2", MS, |_| {});
    finish(&c, &d1, noon(y));
    finish(&c, &d2, noon(y));
    let d3 = add(&c, "今天完成", MS, |_| {});
    finish(&c, &d3, MS);

    let s = dashboard::summary(&c, &FixedNow(MS), 8).expect("summary");
    assert_eq!((s.done.value, s.done.delta), (1, Some(-1)), "今天 1 条、昨天 2 条");
    // 没有历史表就不假装知道昨天的库存量（§2 原则 5）
    assert_eq!(s.inbox.delta, None);
    assert_eq!(s.today.delta, None);
    assert_eq!(s.late.delta, None);
}

#[test]
fn overdue_age_counts_calendar_days_not_integer_difference() {
    let c = db();
    // 9 月 30 日到期：整数相减给出 20261028-20260930 = 98，真实间隔是 28 天
    add(&c, "九月底就该做完", MS, due(20260930));

    let s = dashboard::summary(&c, &FixedNow(MS), 8).expect("summary");
    assert_eq!(s.late.note.as_deref(), Some("最久的一条逾期 28 天"));
}

#[test]
fn unchecking_returns_the_item_to_the_queue_but_keeps_when_it_was_done() {
    // 0002 拆开的两件事：completed 是当前状态，done_at/done_day 是最后一次完成的时间。
    // 「今天完成 N 件」只看 completed，所以这个数字永远等于已完成列表里看到的条数。
    let c = db();
    let id = add(&c, "做完又反悔", MS, due(rel(0)));
    finish(&c, &id, MS);
    todos::set_done(&c, &FixedNow(MS + 60_000), "pc-a", &id, false).expect("un-done");

    let s = dashboard::summary(&c, &FixedNow(MS), 8).expect("summary");
    assert_eq!(s.done.value, 0, "取消勾选后不该还记在完成数里");
    assert_eq!(s.today.value, 1, "它重新回到今天要做的");
    assert_eq!(s.focus.iter().filter(|t| t.stamp.id == id).count(), 1, "环上也该重新出现");

    let kept = todos::get(&c, &id).expect("get").expect("还在");
    assert_eq!(kept.done_at, Some(MS), "但「最后一次完成于何时」不能丢（§5.1）");
    assert!(!kept.completed);
}

#[test]
fn focus_excludes_children_and_caps_at_the_limit() {
    let c = db();
    let parent = add(&c, "父项", MS, due(TODAY));
    add(&c, "子项", MS, |t| {
        t.parent_id = Some(parent.clone());
        t.due_at = Some(noon(TODAY));
    });
    for i in 0..12 {
        add(&c, &format!("逾期 {i}"), MS, due(rel(-1 - i)));
    }

    let s = dashboard::summary(&c, &FixedNow(MS), 8).expect("summary");
    assert_eq!(s.focus.len(), 8, "环最多 8 张卡");
    assert!(s.focus.iter().all(|t| t.parent_id.is_none()), "子项不进环");
    assert!(s.focus.iter().any(|t| t.stamp.id == parent));
}

#[test]
fn focus_prefers_pinned_then_priority() {
    let c = db();
    let low = add(&c, "钉住的低优先级", MS, |t| {
        t.due_at = Some(noon(rel(0)));
        t.priority = Some(3);
    });
    add(&c, "没钉的紧急", MS, |t| {
        t.due_at = Some(noon(rel(0)));
        t.priority = Some(0);
    });
    todos::update(
        &c,
        &FixedNow(MS + 1000),
        "pc-a",
        &low,
        anvil_lib::model::TodoPatch { pinned: Some(true), ..Default::default() },
    )
    .expect("pin");

    let s = dashboard::summary(&c, &FixedNow(MS), 8).expect("summary");
    assert_eq!(s.focus.len(), 2);
    assert_eq!(s.focus[0].stamp.id, low, "置顶压过优先级 —— 顺序是人给的，不该被算掉");
}

#[test]
fn a_todo_lands_in_exactly_one_queue_group() {
    let c = db();
    add(&c, "逾期", MS, due(rel(-3)));
    add(&c, "今天", MS, due(rel(0)));
    add(&c, "以后", MS, due(rel(5)));
    add(&c, "没定日子", MS, in_zone(zones::INBOX));

    let s = dashboard::summary(&c, &FixedNow(MS), 8).expect("summary");
    let mut ids: Vec<&str> = s.groups.iter().flat_map(|g| &g.items).map(|t| t.stamp.id.as_str()).collect();
    let total = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), total, "同一条不该出现在两组");
    let by_key = |k: &str| s.groups.iter().find(|g| g.key == k).map(|g| g.items.len()).unwrap_or(0);
    assert_eq!((by_key("late"), by_key("today"), by_key("later")), (1, 1, 2));
}

#[test]
fn timeline_covers_the_whole_local_month_and_history_spans_the_first_entry() {
    let c = db();
    let first = add(&c, "月初建的", noon(20261001), |_| {});
    finish(&c, &first, noon(20261001));
    let d = add(&c, "今天完成", MS, |_| {});
    finish(&c, &d, MS);
    // 九月建、九月完成：既不该混进本月时间轴，又是 history_days 的真正起点
    let sep = add(&c, "九月完成", noon(20260915), |_| {});
    finish(&c, &sep, noon(20260915));

    let s = dashboard::summary(&c, &FixedNow(MS), 8).expect("summary");
    assert_eq!(s.timeline.iter().map(|d| d.day).collect::<Vec<_>>(), vec![20261001, TODAY]);
    // 09-15 到 10-28 跨 43 天，含首尾两个自然日 → 44
    assert_eq!(s.history_days, 44);
    assert!(s.history_days < 60, "60 天门槛：这条断言的意义在于它真的没到");
}

// ---- 建议规则 ----

fn rules_of(c: &Connection) -> Vec<&'static str> {
    suggest::top(c, &FixedNow(MS)).expect("suggest").iter().map(|s| s.rule).collect()
}

#[test]
fn rule_1_overdue_oldest_wins_and_names_the_todo() {
    let c = db();
    let old = add(&c, "欠最久的", MS, due(20260920));
    add(&c, "欠得短的", MS, due(rel(-1)));

    let out = suggest::top(&c, &FixedNow(MS)).expect("suggest");
    assert_eq!(out[0].rule, "overdue-oldest");
    assert_eq!(out[0].headline, "欠最久的");
    assert_eq!(out[0].todo_id.as_deref(), Some(old.as_str()));
}

#[test]
fn rule_2_only_picks_todos_that_were_never_touched() {
    let c = db();
    let untouched = add(&c, "今天到期没动过", MS, due(TODAY));
    let worked = add(&c, "今天到期但改过了", MS, due(TODAY));
    // 一次 set_done(false) 就把 rev 推到 2、updated_at 挪走 → 视为「动过了」
    todos::set_done(&c, &FixedNow(MS + 1000), "pc-a", &worked, false).expect("touch");

    let out = suggest::top(&c, &FixedNow(MS)).expect("suggest");
    let hit = out.iter().find(|s| s.rule == "due-today-untouched").expect("rule 2");
    assert_eq!(hit.todo_id.as_deref(), Some(untouched.as_str()));
}

#[test]
fn rule_3_fires_only_for_a_running_countdown_under_two_hours() {
    let c = db();
    assert!(!rules_of(&c).contains(&"countdown-short"), "空库不该有倒计时建议");

    let running = db();
    countdown(&running, "健身", "running", false, MS + 3600 * 1000);
    let out = suggest::top(&running, &FixedNow(MS)).expect("suggest");
    let hit = out.iter().find(|s| s.rule == "countdown-short").expect("rule 3");
    assert_eq!(hit.detail, "只剩 60 分");

    let far = db();
    countdown(&far, "下周", "running", false, MS + 3 * 3600 * 1000);
    assert!(!rules_of(&far).contains(&"countdown-short"), "还有 3 小时不该催");

    let paused = db();
    countdown(&paused, "暂停的", "paused", true, MS + 600_000);
    assert!(!rules_of(&paused).contains(&"countdown-short"), "暂停的倒计时不该催");
}

#[test]
fn rule_4_reads_the_medicine_cache_and_stays_silent_without_it() {
    let c = db();
    assert!(!rules_of(&c).contains(&"medicine-low"), "没有药箱缓存就什么都不说");

    let low = db();
    medicine(
        &low,
        "ok",
        r#"{"items":[{"name":"布洛芬","daysLeft":1.4},{"name":"维生素D","daysLeft":30.0}]}"#,
    );
    let out = suggest::top(&low, &FixedNow(MS)).expect("suggest");
    let hit = out.iter().find(|s| s.rule == "medicine-low").expect("rule 4");
    assert_eq!(hit.headline, "布洛芬 快吃完了");
    assert_eq!(hit.detail, "库存约够 1.4 天，可以生成一条「补买」待办");

    let fine = db();
    medicine(&fine, "ok", r#"{"items":[{"name":"布洛芬","daysLeft":9.0}]}"#);
    assert!(!rules_of(&fine).contains(&"medicine-low"));

    // 读失败的状态不能当成「库存充足」，也不能编出数字来
    let missing = db();
    medicine(&missing, "missing", r#"{"items":[]}"#);
    assert!(!rules_of(&missing).contains(&"medicine-low"));

    let broken = db();
    medicine(&broken, "ok", "{不是 json");
    assert!(!rules_of(&broken).contains(&"medicine-low"), "缓存坏了也不能编建议");
}

#[test]
fn rule_5_needs_more_than_three_captures() {
    let c = db();
    for i in 0..3 {
        add(&c, &format!("捕获 {i}"), MS, in_zone(zones::INBOX));
    }
    assert!(!rules_of(&c).contains(&"inbox-pileup"), "正好 3 条还不算堆");
    add(&c, "第 4 条", MS, in_zone(zones::INBOX));
    assert!(rules_of(&c).contains(&"inbox-pileup"));
}

#[test]
fn at_most_three_in_declaration_order() {
    let c = db();
    add(&c, "欠着", MS, due(20260901)); // ①
    add(&c, "今天没动", MS, due(TODAY)); // ②
    add(&c, "明天", MS, due(rel(1)));
    for i in 0..5 {
        add(&c, &format!("捕获 {i}"), MS, in_zone(zones::INBOX)); // ⑤
    }
    countdown(&c, "番茄", "running", false, MS + 30 * 60_000); // ③

    let out = rules_of(&c);
    assert_eq!(out.len(), suggest::LIMIT);
    assert_eq!(out, vec!["overdue-oldest", "due-today-untouched", "countdown-short"]);
}

#[test]
fn empty_database_produces_no_suggestions_and_no_absurd_numbers() {
    let c = db();
    assert!(suggest::top(&c, &FixedNow(MS)).expect("suggest").is_empty());
    let s = dashboard::summary(&c, &FixedNow(MS), 8).expect("summary");
    assert_eq!((s.today.value, s.done.value, s.late.value, s.inbox.value), (0, 0, 0, 0));
    assert_eq!(s.history_days, 0, "一条记录都没有时不要报 202 万天");
    assert!(s.timeline.is_empty());
    assert!(s.focus.is_empty());
    assert!(s.groups.iter().all(|g| g.items.is_empty()));
}
