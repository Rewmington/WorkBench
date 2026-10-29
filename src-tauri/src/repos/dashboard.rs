//! 工作台首屏的读模型（PLAN §4「工作台」+ §8.8）。
//!
//! 这里只做**聚合**：环要的那几条待办仍由 `todos::focus` 出，用的是同一份列与
//! 行映射，于是「KPI 数字」与「点开看到的列表」不会各算各的对不上。
//!
//! 变化量只给**历史撑得住**的那一个：完成数能算昨天（`done_day` 是落库的整数），
//! 而「昨天的未整理捕获有多少条」没有任何地方记过 —— 没有历史表却假装能算，
//! 是 §2 原则 5 明确禁止的那类假状态。所以其余三项给 `note`（同一条 SQL 顺手
//! 能算出的次级事实），不给 `delta`。

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::Result;
use crate::model::todo::zones;
use crate::model::Todo;
use crate::repos::todos;
use crate::time::{days, Now};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Kpi {
    pub value: i64,
    pub delta: Option<i64>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayCount {
    /// 本地自然日 yyyymmdd
    pub day: i32,
    pub done: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthProgress {
    pub done: i64,
    /// 本月到期但还没做完的。进度条分母由这两项组成 —— 不引入用户从没设过的「目标数」。
    pub open: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueGroup {
    pub key: String,
    pub label: String,
    pub items: Vec<Todo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub today: Kpi,
    pub inbox: Kpi,
    pub done: Kpi,
    pub late: Kpi,
    /// 3D 卡片环的卡源（§8.8.1：面数 < 3 退化成横排，0 面折叠成虚线条）
    pub focus: Vec<Todo>,
    pub groups: Vec<QueueGroup>,
    pub timeline: Vec<DayCount>,
    pub month: MonthProgress,
    /// 这个库记了多少个自然日。< 60 时时间轴不画趋势线只画点（§8.8.5）。
    pub history_days: i64,
}

fn count(conn: &Connection, sql: &str, p: &[&dyn rusqlite::ToSql]) -> Result<i64> {
    Ok(conn.query_row(sql, p, |r| r.get(0))?)
}

/// 逾期最久的那条逾了几天；没有逾期 → None。
///
/// 天数走 `days::days_between` 而不是 `today - that_day`：后者对 20260930 给出 99。
fn oldest_overdue(conn: &Connection, now: &dyn Now) -> Result<Option<i64>> {
    let oldest: Option<i64> = conn
        .query_row(
            "SELECT min(due_at) FROM todos
             WHERE deleted_at IS NULL AND completed = 0 AND due_at IS NOT NULL AND due_at < ?1",
            params![now.day_start(now.today())],
            |r| r.get(0),
        )
        .unwrap_or(None);
    let Some(at) = oldest else { return Ok(None) };
    let n = days::days_between(now.day_of(at), now.today());
    Ok((n > 0).then_some(n))
}

/// 今日该做的还剩几件 / 已经做掉几件。统计页的今日环用它，
/// 于是「环上的两段」与驾驶舱的 TODAY / DONE 两张卡是同一个查询算出来的。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayCounts {
    pub open: i64,
    pub done: i64,
}

/// 「今天要做的」= 今天到期且没做完，加上主动放进 today 区、还没定截止的。
///
/// 单独成函数是为了让统计页的今日环**用同一条 SQL**：两处各写一遍谓词，
/// 迟早会出现「驾驶舱说 3 件、统计页说 4 件」这种最难查的对不上。
pub fn open_today(conn: &Connection, now: &dyn Now) -> Result<i64> {
    let today = now.today();
    count(
        conn,
        "SELECT count(*) FROM todos
         WHERE deleted_at IS NULL AND completed = 0
           AND ((due_at BETWEEN ?1 AND ?2) OR (zone = ?3 AND due_at IS NULL))",
        &[&now.day_start(today), &days::day_end_ms(today), &zones::TODAY],
    )
}

/// 某个本地日**当前处于完成态**的条数。取消勾选过的不算 ——
/// 口径与 KPI 完全一致：数字必须等于列表里能数出来的条数。
pub fn done_on(conn: &Connection, day: i32) -> Result<i64> {
    count(
        conn,
        "SELECT count(*) FROM todos WHERE deleted_at IS NULL AND completed = 1 AND done_day = ?1",
        &[&i64::from(day)],
    )
}

/// 这个库记了多少个自然日（含今天）。空库 → 0。
///
/// 统计页的门槛（时段直方图要 ≥ 30 天、时间轴要 ≥ 60 天才画趋势）全押在这一个数上，
/// 所以它必须是唯一的口径 —— 两处各算一次会出现"门槛说够、图却是空的"。
pub fn history_days(conn: &Connection, now: &dyn Now) -> Result<i64> {
    Ok(conn
        .query_row(
            "SELECT min(created_day) FROM todos WHERE deleted_at IS NULL",
            [],
            |r| r.get::<_, Option<i64>>(0),
        )
        .unwrap_or(None)
        .map(|first| days::days_between(first as i32, now.today()) + 1)
        .unwrap_or(0))
}

pub fn summary(conn: &Connection, now: &dyn Now, focus_limit: i64) -> Result<Summary> {
    let today = now.today();
    let t0 = now.day_start(today);

    // 「今天要做的」= 今天到期且没做完，加上主动放进 today 区、还没定截止的
    let today_value = open_today(conn, now)?;

    let inbox_value = count(
        conn,
        "SELECT count(*) FROM todos WHERE deleted_at IS NULL AND completed = 0 AND zone = ?1",
        &[&zones::INBOX],
    )?;
    let inbox_week = count(
        conn,
        "SELECT count(*) FROM todos
         WHERE deleted_at IS NULL AND completed = 0 AND zone = ?1 AND created_day >= ?2",
        &[&zones::INBOX, &i64::from(days::shift(today, -6))],
    )?;

    // 完成口径一律只看**当前完成态**：`done_day` 记的是最后一次完成发生在哪天，
    // 但取消勾选之后它就不该再算进「今天完成了几件」—— 数字必须等于列表里看到的条数。
    let done_today = done_on(conn, today)?;
    let done_yesterday = done_on(conn, days::prev_day(today))?;

    let late_value = count(
        conn,
        "SELECT count(*) FROM todos
         WHERE deleted_at IS NULL AND completed = 0 AND due_at IS NOT NULL AND due_at < ?1",
        &[&t0],
    )?;
    let late_note = oldest_overdue(conn, now)?.map(|d| format!("最久的一条逾期 {d} 天"));

    let focus = todos::focus(conn, now, focus_limit)?;
    let groups = queue_groups(conn, now)?;

    let month_from = i64::from(days::month_first_day(today));
    let month_to = i64::from(days::month_last_day(today));
    let mut timeline = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT done_day, count(*) FROM todos
         WHERE deleted_at IS NULL AND completed = 1 AND done_day BETWEEN ?1 AND ?2
         GROUP BY done_day ORDER BY done_day",
    )?;
    for row in stmt.query_map(params![month_from, month_to], |r| {
        Ok(DayCount { day: r.get::<_, i64>(0)? as i32, done: r.get(1)? })
    })? {
        timeline.push(row?);
    }
    drop(stmt);

    let month_open = count(
        conn,
        "SELECT count(*) FROM todos
         WHERE deleted_at IS NULL AND completed = 0 AND due_at BETWEEN ?1 AND ?2",
        &[&now.day_start(days::month_first_day(today)), &days::day_end_ms(days::month_last_day(today))],
    )?;

    let history_days = history_days(conn, now)?;

    Ok(Summary {
        today: Kpi { value: today_value, delta: None, note: None },
        inbox: Kpi {
            value: inbox_value,
            delta: None,
            note: (inbox_week > 0).then(|| format!("近 7 天新捕 {inbox_week} 条")),
        },
        done: Kpi { value: done_today, delta: Some(done_today - done_yesterday), note: None },
        late: Kpi { value: late_value, delta: None, note: late_note },
        focus,
        groups,
        timeline,
        month: MonthProgress { done: done_today, open: month_open },
        history_days,
    })
}

/// 队列分组：逾期 / 今天 / 稍后。
///
/// 一次取回再在 Rust 里分派，而不是三条 SQL 各查一遍 —— 后者会让同一条待办
/// 因为边界写法不同同时出现在两组，那种「列表 3 条、徽章 5 条」最难查。
fn queue_groups(conn: &Connection, now: &dyn Now) -> Result<Vec<QueueGroup>> {
    let t0 = now.day_start(now.today());
    let t1 = days::day_end_ms(now.today());

    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM todos
         WHERE deleted_at IS NULL AND completed = 0 AND parent_id IS NULL
         ORDER BY pinned DESC, priority ASC, sort_key ASC",
        todos::COLS
    ))?;
    let rows = stmt.query_map([], todos::map_row)?;
    let mut all = Vec::new();
    for row in rows {
        all.push(row?);
    }
    drop(stmt);
    todos::with_tags(conn, &mut all)?;

    let mut late = Vec::new();
    let mut due_today = Vec::new();
    let mut later = Vec::new();
    for t in all {
        match t.due_at {
            Some(due) if due < t0 => late.push(t),
            Some(due) if due <= t1 => due_today.push(t),
            _ => later.push(t),
        }
    }

    Ok(vec![
        QueueGroup { key: "late".into(), label: "逾期".into(), items: late },
        QueueGroup { key: "today".into(), label: "今天".into(), items: due_today },
        QueueGroup { key: "later".into(), label: "稍后".into(), items: later },
    ])
}
