//! 统计视图的读模型（PLAN §7）。
//!
//! 五张图的聚合放在一处、并且**能复用驾驶舱口径的就复用**（今日环直接调
//! `dashboard::open_today` / `done_on`，跨度用 `dashboard::history_days`），
//! 因为 §7 的验收就是「逐图核对聚合 SQL 与列表一致」—— 两处各写一遍谓词，
//! 迟早会出现「图上的数」和「点开列表数出来的条数」差一。
//!
//! 分组一律按 `done_day` / `created_day` 这些**落库的本地自然日整数**，
//! 不在 SQL 里 `date(done_at/86400000)` —— 那会把 UTC 早 8 点前的记录归错天。
//! 唯一需要在查询里换算的是「按时还是补做」，因为 `due_at` 没有对应的日整数列，
//! 于是把 `(done_day, due_at)` 取回 Rust 用 `days::day_of_ms` 比。

use std::collections::BTreeSet;

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::Result;
use crate::repos::dashboard;
use crate::time::{days, Now};

/// 热力图铺 15 周（§7 的 7×15）。少于 15 周会把"有没有连着做"截断。
pub const HEATMAP_WEEKS: i64 = 15;
/// 柱状图看近 30 天：再短就只是"这两天在赶"，看不出硬扛的节奏。
pub const BAR_DAYS: i64 = 30;
/// 标签条看近 90 天：投入分布是慢变量，30 天里一次出差就全歪。
pub const TAG_DAYS: i64 = 90;
/// 时段直方图的门槛（§7：要 ≥ 30 天才有形状）。
pub const HOUR_MIN_DAYS: i64 = 30;
/// 标签条最多画几条。再多就不是分布而是名单了。
pub const TAG_LIMIT: i64 = 8;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeatCell {
    /// 本地自然日 yyyymmdd
    pub day: i32,
    pub done: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Heatmap {
    /// 长度恒为 `7 * HEATMAP_WEEKS`，周一开头、缺数据的天也占一格（done = 0）。
    /// 给全量而不是只给有数据的格子，是为了让前端不必自己对齐列 —— 它一对齐就会错。
    pub cells: Vec<HeatCell>,
    pub start_day: i32,
    pub end_day: i32,
    pub active_days: i64,
    /// 连续做了几天。**止于 `streak_through`**，不是"止于今天"：
    /// 今天还没做不代表昨天的连续断了，说成止于今天就是假报。
    pub streak: i64,
    pub streak_through: Option<i32>,
}

/// 一天的三档。§7 只要"按时 / 补做"两档，但没设截止的待办既不属于前者也不属于后者，
/// 硬塞进任何一档都是谎，所以第三档明写出来。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DayBar {
    pub day: i32,
    pub on_time: i64,
    pub late: i64,
    pub no_due: i64,
}

impl DayBar {
    pub fn total(&self) -> i64 {
        self.on_time + self.late + self.no_due
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagBar {
    pub tag: String,
    pub done: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HourBar {
    pub hour: i32,
    pub created: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub today_day: i32,
    pub history_days: i64,
    pub heatmap: Heatmap,
    pub bars: Vec<DayBar>,
    /// 今日环的两段。分母不是"目标数"—— 这个应用里没有目标数这个东西。
    pub gauge: dashboard::TodayCounts,
    pub tags: Vec<TagBar>,
    /// 近 90 天完成的总条数。一条待办可以挂多个标签，所以各条加起来会超过它。
    pub tags_total: i64,
    pub hours: Vec<HourBar>,
    /// 时段直方图够不够画。false 时界面写「需 ≥ N 天，当前 M 天」。
    pub hours_enough: bool,
    /// 门槛天数跟着数据走，而不是让前端抄一份常量 —— 改了这里的数，
    /// 界面那句话不能还说旧的。
    pub hour_min_days: i64,
    /// 库里有没有任何完成记录 —— 决定前四张图该不该出空态。
    pub has_activity: bool,
}

pub fn report(conn: &Connection, now: &dyn Now) -> Result<Report> {
    let today = now.today();
    let history_days = dashboard::history_days(conn, now)?;
    let done_days = done_day_set(conn)?;
    let has_activity = !done_days.is_empty();

    Ok(Report {
        today_day: today,
        history_days,
        heatmap: heatmap(conn, today, &done_days)?,
        bars: day_bars(conn, today)?,
        gauge: dashboard::TodayCounts {
            open: dashboard::open_today(conn, now)?,
            done: dashboard::done_on(conn, today)?,
        },
        tags: tag_bars(conn, today)?,
        tags_total: count_in_window(conn, today, TAG_DAYS)?,
        hours: hour_bars(conn)?,
        hours_enough: history_days >= HOUR_MIN_DAYS,
        hour_min_days: HOUR_MIN_DAYS,
        has_activity,
    })
}

/// 所有**当前完成态**的本地自然日集合（升序）。连续天数与活跃天数都从它算。
fn done_day_set(conn: &Connection) -> Result<BTreeSet<i32>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT done_day FROM todos
         WHERE deleted_at IS NULL AND completed = 1 AND done_day IS NOT NULL",
    )?;
    let rows = stmt.query_map([], |r| r.get::<_, i64>(0))?;
    let mut out = BTreeSet::new();
    for row in rows {
        out.insert(row? as i32);
    }
    Ok(out)
}

fn heatmap(conn: &Connection, today: i32, done_days: &BTreeSet<i32>) -> Result<Heatmap> {
    // 周一对齐：网格从"14 周前所在的周一"铺到"本周日"，最后一列必然含今天。
    let anchor = days::shift(today, -(HEATMAP_WEEKS * 7 - 7));
    let start = days::shift(anchor, 1 - i64::from(days::weekday(anchor)));
    let end = days::shift(start, HEATMAP_WEEKS * 7 - 1);

    let mut counts = std::collections::HashMap::new();
    let mut stmt = conn.prepare(
        "SELECT done_day, count(*) FROM todos
         WHERE deleted_at IS NULL AND completed = 1 AND done_day BETWEEN ?1 AND ?2
         GROUP BY done_day",
    )?;
    for row in stmt.query_map(params![i64::from(start), i64::from(end)], |r| {
        Ok((r.get::<_, i64>(0)? as i32, r.get::<_, i64>(1)?))
    })? {
        let (day, n) = row?;
        counts.insert(day, n);
    }
    drop(stmt);

    let cells: Vec<HeatCell> = (0..HEATMAP_WEEKS * 7)
        .map(|i| {
            let day = days::shift(start, i);
            HeatCell { day, done: counts.get(&day).copied().unwrap_or(0) }
        })
        .collect();

    let active_days = cells.iter().filter(|c| c.done > 0).count() as i64;
    let (streak, streak_through) = current_streak(done_days, today);

    Ok(Heatmap {
        cells,
        start_day: start,
        end_day: end,
        active_days,
        streak,
        streak_through,
    })
}

/// 从最近一个有记录的那天往回数连续。今天没做不算断，只是"截至昨天"。
fn current_streak(done_days: &BTreeSet<i32>, today: i32) -> (i64, Option<i32>) {
    let Some(&last) = done_days.iter().next_back() else {
        return (0, None);
    };
    // 未来的完成记录（改了系统时间或种了未来数据）不该让"今天"的连续看起来更长
    if last > today {
        return (0, None);
    }
    let mut n = 1i64;
    let mut cursor = last;
    while done_days.contains(&(days::prev_day(cursor))) {
        cursor = days::prev_day(cursor);
        n += 1;
    }
    (n, Some(last))
}

fn day_bars(conn: &Connection, today: i32) -> Result<Vec<DayBar>> {
    let start = days::shift(today, -(BAR_DAYS - 1));
    let mut stmt = conn.prepare(
        "SELECT done_day, due_at FROM todos
         WHERE deleted_at IS NULL AND completed = 1 AND done_day BETWEEN ?1 AND ?2",
    )?;
    let rows = stmt.query_map(params![i64::from(start), i64::from(today)], |r| {
        Ok((
            r.get::<_, i64>(0)? as i32,
            r.get::<_, Option<i64>>(1)?,
        ))
    })?;

    let mut bars: Vec<DayBar> = (0..BAR_DAYS)
        .map(|i| DayBar { day: days::shift(start, i), on_time: 0, late: 0, no_due: 0 })
        .collect();
    let index: std::collections::HashMap<i32, usize> =
        bars.iter().enumerate().map(|(k, b)| (b.day, k)).collect();

    for row in rows {
        let (done_day, due_at) = row?;
        let Some(&i) = index.get(&done_day) else { continue };
        match due_at {
            None => bars[i].no_due += 1,
            // 按时 = 在截止当天或更早完成。两边都比本地自然日，不比毫秒：
            // 「12 月 1 日 23:00 截止、12 月 1 日 23:30 做完」在毫秒上算迟到，在日历上不是。
            Some(due) => {
                if done_day <= days::day_of_ms(due) {
                    bars[i].on_time += 1
                } else {
                    bars[i].late += 1
                }
            }
        }
    }
    Ok(bars)
}

fn tag_bars(conn: &Connection, today: i32) -> Result<Vec<TagBar>> {
    let start = i64::from(days::shift(today, -(TAG_DAYS - 1)));
    let mut stmt = conn.prepare(
        "SELECT t.name, count(*) FROM todos d
         JOIN todo_tag_links l ON l.todo_id = d.id AND l.deleted_at IS NULL
         JOIN tags t ON t.id = l.tag_id AND t.deleted_at IS NULL
         WHERE d.deleted_at IS NULL AND d.completed = 1 AND d.done_day >= ?1
         GROUP BY t.name ORDER BY count(*) DESC, t.name ASC LIMIT ?2",
    )?;
    let rows = stmt.query_map(params![start, TAG_LIMIT], |r| {
        Ok(TagBar { tag: r.get(0)?, done: r.get(1)? })
    })?;
    let mut out: Vec<TagBar> = rows.collect::<rusqlite::Result<Vec<_>>>()?;

    // 「无标签」单独补在最后：它不参竞争 top 8，但藏掉它就等于假装每条待办都归得了类。
    let untagged: i64 = conn.query_row(
        "SELECT count(*) FROM todos d
         WHERE d.deleted_at IS NULL AND d.completed = 1 AND d.done_day >= ?1
           AND NOT EXISTS (
             SELECT 1 FROM todo_tag_links l
             JOIN tags t ON t.id = l.tag_id AND t.deleted_at IS NULL
             WHERE l.todo_id = d.id AND l.deleted_at IS NULL)",
        params![start],
        |r| r.get(0),
    )?;
    if untagged > 0 {
        out.push(TagBar { tag: "无标签".to_owned(), done: untagged });
    }
    Ok(out)
}

fn count_in_window(conn: &Connection, today: i32, window: i64) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT count(*) FROM todos
         WHERE deleted_at IS NULL AND completed = 1 AND done_day >= ?1",
        params![i64::from(days::shift(today, -(window - 1)))],
        |r| r.get(0),
    )?)
}

/// 建待办的时段分布。`created_hour` 是落库的本地小时整数，不需要换算。
///
/// 过滤墓碑与本页其余四张图保持同一口径：一张图算"现在库里的东西"、
/// 另一张偷偷把删掉的也算进来，两句话就打架了。
fn hour_bars(conn: &Connection) -> Result<Vec<HourBar>> {
    let mut stmt = conn.prepare(
        "SELECT created_hour, count(*) FROM todos
         WHERE deleted_at IS NULL AND created_hour IS NOT NULL
         GROUP BY created_hour",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)? as i32, r.get::<_, i64>(1)?)))?;
    let mut counts = [0i64; 24];
    for row in rows {
        let (h, n) = row?;
        if (0..24).contains(&h) {
            counts[h as usize] = n;
        }
    }
    Ok(counts
        .iter()
        .enumerate()
        .map(|(h, n)| HourBar { hour: h as i32, created: *n })
        .collect())
}
