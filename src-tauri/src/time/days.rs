use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};

/// 本地日历日整数 `yyyymmdd`。可排序、可区间比较，且与 DST/时区换算无关 ——
/// 一个本地自然日就是这一个整数，不会像 `date(done_at/86400000)` 那样把 UTC 早 8 点前的
/// 记录归错天（PLAN §7 的坑）。
pub fn day_of_ms(ms: i64) -> i32 {
    let Some(utc) = DateTime::from_timestamp_millis(ms) else {
        return 0;
    };
    let local = utc.with_timezone(&Local);
    local.year() * 10000 + local.month() as i32 * 100 + local.day() as i32
}

pub fn hour_of_ms(ms: i64) -> i32 {
    DateTime::from_timestamp_millis(ms)
        .map(|utc| utc.with_timezone(&Local).hour() as i32)
        .unwrap_or(0)
}

/// 本地当日 00:00 起的分钟数，供倒计时与「今日几个时段」用。
pub fn minute_of_day_ms(ms: i64) -> i32 {
    DateTime::from_timestamp_millis(ms)
        .map(|utc| {
            let l = utc.with_timezone(&Local);
            l.hour() as i32 * 60 + l.minute() as i32
        })
        .unwrap_or(0)
}

pub fn split_day(day: i32) -> (i32, u32, u32) {
    (day / 10000, ((day / 100) % 100) as u32, (day % 100) as u32)
}

/// 某个本地日的 00:00。模糊时刻（回拨）取较晚的一个，与「今天」的直觉一致。
///
/// 相对某天请用 `shift` / `next_day`，**不要 `today + 5`**：`yyyymmdd` 整数相加会造出
/// `20261033` 这种根本不存在的日子，而它落到 chrono 拿不到时刻、只能回 0（= 1970 年），
/// 于是「5 天后的待办」在界面上变成「逾期最久的一条」。debug 期直接拦下。
pub fn day_start_ms(day: i32) -> i64 {
    let (y, m, d) = split_day(day);
    debug_assert!(
        matches!(Local.with_ymd_and_hms(y, m, d, 12, 0, 0), chrono::LocalResult::Single(_)),
        "非法的本地日整数 {day}：相对天数请用 days::shift 而不是整数相加"
    );
    Local
        .with_ymd_and_hms(y, m, d, 0, 0, 0)
        .latest()
        .map(|t| t.timestamp_millis())
        .unwrap_or(0)
}

/// 某本地日的 23:59:59.999，用作统计的区间右端。
pub fn day_end_ms(day: i32) -> i64 {
    day_start_ms(next_day(day)) - 1
}

/// 上一个/下一个自然日，跨月跨年按日历走。
pub fn prev_day(day: i32) -> i32 {
    shift(day, -1)
}

pub fn next_day(day: i32) -> i32 {
    shift(day, 1)
}

/// 加减 N 个自然日。统计与「近 7 天」这类窗口都靠它，跨月自动走日历。
pub fn shift(day: i32, by: i64) -> i32 {
    let (y, m, d) = split_day(day);
    match Local.with_ymd_and_hms(y, m, d, 12, 0, 0) {
        chrono::LocalResult::Single(t) => day_of_ms(t.timestamp_millis() + by * 86_400_000),
        _ => day,
    }
}

/// 两个自然日之间隔了几天（`to - from`，可为负）。
///
/// **不能直接 `20261029 - 20260930`** —— 那是 99，而真实间隔是 29 天。
/// 「逾期 N 天」「记了 N 天」这类文案全靠这个函数，整数相减是必然会写错的那种错。
pub fn days_between(from_day: i32, to_day: i32) -> i64 {
    let date = |d: i32| {
        let (y, m, dd) = split_day(d);
        Local.with_ymd_and_hms(y, m, dd, 12, 0, 0).latest().map(|t| t.date_naive())
    };
    match (date(from_day), date(to_day)) {
        (Some(a), Some(b)) => (b - a).num_days(),
        _ => 0,
    }
}

/// 该日所在月的第一天。时间轴的「本月」按本地日历切，不按 30 天滚动。
pub fn month_first_day(day: i32) -> i32 {
    let (y, m, _) = split_day(day);
    y * 10000 + m as i32 * 100 + 1
}

/// 该日所在月的最后一天。用「下月一号的前一天」拿，省掉一张月份天数表，
/// 闰年二月因此自动正确。
pub fn month_last_day(day: i32) -> i32 {
    let (y, m, _) = split_day(day);
    let (ny, nm) = if m == 12 { (y + 1, 1u32) } else { (y, m + 1) };
    prev_day(ny * 10000 + nm as i32 * 100 + 1)
}

/// 该日的星期，**1 = 周一 … 7 = 周日**（与界面里 `['一','二','三','四','五','六','日']`
/// 同一约定）。热力图按 7 行铺，行号就是它。
pub fn weekday(day: i32) -> i32 {
    let (y, m, d) = split_day(day);
    Local
        .with_ymd_and_hms(y, m, d, 12, 0, 0)
        .latest()
        .map(|t| t.weekday().number_from_monday() as i32)
        .unwrap_or(1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DaySpan {    pub from: i32,
    pub to: i32,
}

impl DaySpan {
    pub fn contains(&self, day: i32) -> bool {
        day >= self.from && day <= self.to
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn day_arithmetic_crosses_month_boundaries() {
        assert_eq!(next_day(20261031), 20261101);
        assert_eq!(next_day(20261231), 20270101);
        assert_eq!(prev_day(20261101), 20261031);
        assert_eq!(prev_day(20260101), 20251231);
        assert_eq!(split_day(20261028), (2026, 10, 28));
    }

    #[test]
    fn month_bounds_span_whole_months_including_february() {
        assert_eq!(month_first_day(20261028), 20261001);
        assert_eq!(month_last_day(20261028), 20261031);
        // 闰年二月：2028 是闰年，2026 不是
        assert_eq!(month_last_day(20280215), 20280229);
        assert_eq!(month_last_day(20260215), 20260228);
        // 十二月跨年
        assert_eq!(month_first_day(20261231), 20261201);
        assert_eq!(month_last_day(20261231), 20261231);
    }

    #[test]
    fn day_gap_counts_calendar_days_not_integer_subtraction() {
        // 整数相减会给出 99 / 8700 这种数，而真实间隔是 29 与 1
        assert_eq!(days_between(20260930, 20261029), 29);
        assert_eq!(days_between(20261231, 20270101), 1);
        assert_eq!(days_between(20261028, 20261028), 0);
        assert_eq!(days_between(20261029, 20261028), -1);
    }

    #[test]
    fn day_bounds_are_consistent() {
        let d = 20261028;
        let start = day_start_ms(d);
        assert_eq!(day_of_ms(start), d);
        assert_eq!(day_of_ms(start + 86_400_000 - 1), d);
        assert_eq!(day_of_ms(day_end_ms(d)), d);
        assert!(day_start_ms(next_day(d)) > day_end_ms(d));
    }
}
