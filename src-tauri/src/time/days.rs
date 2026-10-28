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
pub fn day_start_ms(day: i32) -> i64 {
    let (y, m, d) = split_day(day);
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

fn shift(day: i32, by: i64) -> i32 {
    let (y, m, d) = split_day(day);
    match Local.with_ymd_and_hms(y, m, d, 12, 0, 0) {
        chrono::LocalResult::Single(t) => day_of_ms(t.timestamp_millis() + by * 86_400_000),
        _ => day,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DaySpan {
    pub from: i32,
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
    fn day_bounds_are_consistent() {
        let d = 20261028;
        let start = day_start_ms(d);
        assert_eq!(day_of_ms(start), d);
        assert_eq!(day_of_ms(start + 86_400_000 - 1), d);
        assert_eq!(day_of_ms(day_end_ms(d)), d);
        assert!(day_start_ms(next_day(d)) > day_end_ms(d));
    }
}
