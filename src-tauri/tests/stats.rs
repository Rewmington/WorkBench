//! M7：统计聚合。
//!
//! §9 的验收是「逐图核对聚合 SQL 与列表一致」，`tools/verify.mjs` 拿真库对总数；
//! 这一层管的是**形状与口径**：网格对不对齐周一、按时/补做按日历还是按毫秒、
//! 多标签会不会重复计、门槛够不够。这几条错了，图上的数看着仍然很顺眼。

use rusqlite::Connection;

use anvil_lib::db::migrations::migrate;
use anvil_lib::model::TodoDraft;
use anvil_lib::repos::{dashboard, stats, tags, timetable, todos};
use anvil_lib::time::{days, FixedNow};

const TODAY: i32 = 20261028; // 周三
const DEVICE: &str = "test-device";
const HOUR: i64 = 3_600_000;

fn db() -> Connection {
    let mut c = Connection::open_in_memory().expect("in-memory");
    migrate(&mut c).expect("migrate");
    c
}

fn at(day: i32, hour: i64) -> FixedNow {
    FixedNow(days::day_start_ms(day) + hour * HOUR)
}

/// 走真实写入路径造一条「某天完成」的记录：创建 → 设截止 → 勾选完成。
/// 不直接 INSERT，那样测的是夹具而不是 `done_day` 的落库口径。
fn completed_on(
    conn: &Connection,
    created_day: i32,
    done_day: i32,
    due_day: Option<i32>,
    title: &str,
) -> String {
    let due_at = due_day.map(|d| days::day_start_ms(d) + 18 * HOUR);
    let made = todos::create(
        conn,
        &at(created_day, 9),
        DEVICE,
        TodoDraft {
            title: title.to_owned(),
            due_at,
            ..Default::default()
        },
    )
    .expect("create");
    todos::set_done(conn, &at(done_day, 23), DEVICE, &made.stamp.id, true).expect("set_done");
    made.stamp.id
}

fn tag(conn: &Connection, id: &str, day: i32, names: &[&str]) {
    let owned: Vec<String> = names.iter().map(|s| s.to_string()).collect();
    tags::set_todo_tags(conn, &at(day, 12), DEVICE, id, &owned).expect("tag");
}

#[test]
fn heatmap_is_a_monday_aligned_grid_that_includes_empty_days() {
    let conn = db();
    let two_back = days::shift(TODAY, -2);
    completed_on(&conn, TODAY, TODAY, Some(TODAY), "今天做的");
    completed_on(&conn, two_back, two_back, Some(two_back), "前天做的");

    let r = stats::report(&conn, &at(TODAY, 12)).expect("report");
    assert_eq!(
        r.heatmap.cells.len(),
        (stats::HEATMAP_WEEKS * 7) as usize,
        "网格要铺满 15 周"
    );
    assert_eq!(days::weekday(r.heatmap.start_day), 1, "网格必须从周一起");
    assert!(
        r.heatmap.end_day >= TODAY,
        "最后一列要含今天：{} < {TODAY}",
        r.heatmap.end_day
    );

    let at_day = |d: i32| {
        r.heatmap
            .cells
            .iter()
            .find(|c| c.day == d)
            .map(|c| c.done)
            .expect("格子存在")
    };
    assert_eq!(at_day(TODAY), 1);
    assert_eq!(at_day(two_back), 1);
    // 空白天也占一格 —— 热力图看的就是断点，缺格等于把断点藏起来
    assert_eq!(at_day(days::shift(TODAY, -1)), 0);
    assert_eq!(r.heatmap.active_days, 2);
}

#[test]
fn streak_counts_consecutive_days_and_stops_at_the_last_active_one() {
    let conn = db();
    for back in 1..=3 {
        let day = days::shift(TODAY, -back);
        completed_on(&conn, day, day, Some(day), &format!("D-{back}"));
    }
    let r = stats::report(&conn, &at(TODAY, 12)).expect("report");
    // 今天还没做，但连续不该被说成断在今天 —— 止于昨天才是真话
    assert_eq!(r.heatmap.streak, 3);
    assert_eq!(r.heatmap.streak_through, Some(days::shift(TODAY, -1)));

    completed_on(&conn, TODAY, TODAY, Some(TODAY), "今天也做了");
    let r = stats::report(&conn, &at(TODAY, 12)).expect("report");
    assert_eq!(r.heatmap.streak, 4);
    assert_eq!(r.heatmap.streak_through, Some(TODAY));
}

#[test]
fn a_gap_breaks_the_streak() {
    let conn = db();
    for back in [1, 3, 4] {
        let day = days::shift(TODAY, -back);
        completed_on(&conn, day, day, Some(day), &format!("D-{back}"));
    }
    let r = stats::report(&conn, &at(TODAY, 12)).expect("report");
    assert_eq!(r.heatmap.streak, 1);
    assert_eq!(r.heatmap.streak_through, Some(days::shift(TODAY, -1)));
}

#[test]
fn bars_split_on_time_late_and_no_due_by_calendar_day_not_by_millisecond() {
    let conn = db();
    // 截止 23:00、完成 23:30：毫秒上算迟到，日历上就是按时
    let on_time = todos::create(
        &conn,
        &at(TODAY, 9),
        DEVICE,
        TodoDraft {
            title: "压线完成".to_owned(),
            due_at: Some(days::day_start_ms(TODAY) + 23 * HOUR),
            ..Default::default()
        },
    )
    .expect("create");
    todos::set_done(
        &conn,
        &at(TODAY, 23),
        DEVICE,
        &on_time.stamp.id,
        true,
    )
    .expect("done");
    // 截止昨天、今天才做完 = 补做
    completed_on(&conn, TODAY, TODAY, Some(days::prev_day(TODAY)), "补做");
    // 压根没设截止
    completed_on(&conn, TODAY, TODAY, None, "无截止");

    let r = stats::report(&conn, &at(TODAY, 12)).expect("report");
    assert_eq!(r.bars.len(), stats::BAR_DAYS as usize);
    let today = r.bars.iter().find(|b| b.day == TODAY).expect("今天的柱");
    assert_eq!((today.on_time, today.late, today.no_due), (1, 1, 1));
    let sum: i64 = r.bars.iter().map(|b| b.total()).sum();
    assert_eq!(sum, 3, "窗口内的柱高之和必须等于列表里数出来的完成条数");
}

#[test]
fn a_multi_tag_todo_counts_in_every_tag_and_untagged_stays_visible() {
    let conn = db();
    let both = completed_on(&conn, TODAY, TODAY, Some(TODAY), "挂两个标签");
    tag(&conn, &both, TODAY, &["工作", "跑腿"]);
    completed_on(&conn, TODAY, TODAY, Some(TODAY), "没标签");

    let r = stats::report(&conn, &at(TODAY, 12)).expect("report");
    let of = |name: &str| {
        r.tags
            .iter()
            .find(|t| t.tag == name)
            .map(|t| t.done)
            .unwrap_or(0)
    };
    assert_eq!(of("工作"), 1);
    assert_eq!(of("跑腿"), 1);
    assert_eq!(of("无标签"), 1);
    // 标签条加起来 3 > 总数 2 —— 多对多本来就这样，界面要把这句话说出来
    assert_eq!(r.tags_total, 2);
    assert!(r.tags.iter().map(|t| t.done).sum::<i64>() > r.tags_total);
}

#[test]
fn the_hour_histogram_waits_for_thirty_days_of_history() {
    let conn = db();
    completed_on(&conn, TODAY, TODAY, Some(TODAY), "今天");
    let r = stats::report(&conn, &at(TODAY, 12)).expect("report");
    assert_eq!(r.hours.len(), 24);
    assert!(!r.hours_enough, "只记了 1 天就说得出时段分布是骗人");
    assert_eq!(r.history_days, 1);

    completed_on(&conn, days::shift(TODAY, -40), TODAY, Some(TODAY), "40 天前建的");
    let r = stats::report(&conn, &at(TODAY, 12)).expect("report");
    assert!(r.hours_enough, "跨度 {} 天该放行门槛 30 天", r.history_days);
    assert_eq!(r.history_days, 41);
}

#[test]
fn the_gauge_reads_the_same_numbers_as_the_dashboard() {
    let conn = db();
    // 两条今天到期没做完 + 一条今天做完
    todos::create(
        &conn,
        &at(TODAY, 8),
        DEVICE,
        TodoDraft {
            title: "还没做 1".to_owned(),
            due_at: Some(days::day_start_ms(TODAY) + 18 * HOUR),
            ..Default::default()
        },
    )
    .expect("create");
    todos::create(
        &conn,
        &at(TODAY, 8),
        DEVICE,
        TodoDraft {
            title: "还没做 2".to_owned(),
            zone: Some("today".to_owned()),
            ..Default::default()
        },
    )
    .expect("create");
    completed_on(&conn, TODAY, TODAY, Some(TODAY), "做完了");

    let now = at(TODAY, 12);
    let r = stats::report(&conn, &now).expect("report");
    let s = dashboard::summary(&conn, &now, 8).expect("summary");
    assert_eq!((r.gauge.open, r.gauge.done), (2, 1));
    // 同一条 SQL：驾驶舱的 TODAY / DONE 两张卡与统计页的环不可能对不上
    assert_eq!(r.gauge.open, s.today.value);
    assert_eq!(r.gauge.done, s.done.value);
}

#[test]
fn an_empty_library_reports_no_activity_rather_than_divide_by_zero() {
    let conn = db();
    let r = stats::report(&conn, &at(TODAY, 12)).expect("report");
    assert!(!r.has_activity);
    assert!(!r.hours_enough);
    assert_eq!(r.history_days, 0);
    assert_eq!(r.heatmap.active_days, 0);
    assert_eq!(r.heatmap.streak, 0);
    assert_eq!(r.heatmap.streak_through, None);
    assert_eq!(r.gauge.open, 0);
    assert_eq!(r.gauge.done, 0);
    assert!(r.tags.is_empty());
    assert_eq!(r.tags_total, 0);
    assert_eq!(r.bars.iter().map(|b| b.total()).sum::<i64>(), 0);
}

#[test]
fn the_hour_histogram_uses_the_same_deleted_at_ruling_as_every_other_chart() {
    let conn = db();
    completed_on(&conn, TODAY, TODAY, Some(TODAY), "留着");
    let b = completed_on(&conn, TODAY, TODAY, Some(TODAY), "删掉");
    todos::soft_delete(&conn, &at(TODAY, 13), DEVICE, &b).expect("soft_delete");

    let r = stats::report(&conn, &at(TODAY, 12)).expect("report");
    let total: i64 = r.hours.iter().map(|h| h.created).sum();
    assert!(total == 1, "墓碑不该再算进「什么时候在建待办」，实际 {total}");
}

#[test]
fn undone_and_trashed_rows_never_reach_the_charts() {
    let conn = db();
    let gone = completed_on(&conn, TODAY, TODAY, Some(TODAY), "删掉的完成项");
    todos::soft_delete(&conn, &at(TODAY, 13), DEVICE, &gone).expect("delete");
    let open = completed_on(&conn, TODAY, TODAY, Some(TODAY), "先做完又取消");
    todos::set_done(&conn, &at(TODAY, 14), DEVICE, &open, false).expect("un-complete");

    let r = stats::report(&conn, &at(TODAY, 12)).expect("report");
    assert!(!r.has_activity, "取消勾选之后 done_day 还留着，但当前状态不是完成态");
    assert_eq!(r.bars.iter().map(|b| b.total()).sum::<i64>(), 0);
    assert_eq!(r.tags_total, 0);
}

/// 一整个学期导进来，统计页的每一个完成数都不许动（PLAN §6.7 边界 2）。
/// 今天当然不会动 —— 两张表。这条钉的是以后有人图省事拿 UNION 把 classes 混进聚合，
/// 那时"上满一学期课"会变成"完成了几百件事"，是最难看出来的一种谎。
#[test]
fn importing_a_whole_term_moves_no_completion_number() {
    let conn = db();
    completed_on(&conn, TODAY, TODAY, Some(TODAY), "按时做完的一条");
    let before = stats::report(&conn, &at(TODAY, 20)).expect("report");

    let csv = r#"学生个人课表,,,,,,,
学年学期：2026-2027-1,,,,,,
,星期一,星期二,星期三,星期四,星期五,星期六,星期日
"第一大节
08:00-09:40","
编译原理
张老师
1-16[周]
[主校区]一教101
[01-02]节",,,,,,
"第二大节
10:00-11:40",,,"
线性代数
王老师
1-16[周]
[主校区]一教105
[05-06]节",,,,
"#;
    let report = timetable::import(&conn, &at(TODAY, 20), DEVICE, csv).expect("import");
    assert!(report.rows >= 2, "夹具得真导进东西，不然这条测的是个空：{report:?}");

    let after = stats::report(&conn, &at(TODAY, 20)).expect("report");
    assert_eq!(after.gauge.done, before.gauge.done, "今日环的完成数不含课");
    assert_eq!(after.gauge.open, before.gauge.open, "课也不进「今天要做的」");
    assert_eq!(after.tags_total, before.tags_total);
    assert_eq!(
        after.bars.iter().map(|b| b.total()).sum::<i64>(),
        before.bars.iter().map(|b| b.total()).sum::<i64>(),
        "完成日历上多出来的那几格就是课"
    );
    assert_eq!(after.history_days, before.history_days);
}
