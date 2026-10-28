pub mod days;

use days::{day_of_ms, day_start_ms, hour_of_ms};

/// 时间只能从这一个口子进来。repo 内部禁止 `Local::now()` ——
/// 否则「逾期 2 小时」这类判断在单测里不可复现，在真机上也无法验证改系统时间的跟随。
pub trait Now {
    fn ms(&self) -> i64;

    fn day_of(&self, ms: i64) -> i32 {
        day_of_ms(ms)
    }
    fn hour_of(&self, ms: i64) -> i32 {
        hour_of_ms(ms)
    }
    fn today(&self) -> i32 {
        self.day_of(self.ms())
    }
    fn day_start(&self, day: i32) -> i64 {
        day_start_ms(day)
    }
}

pub struct SystemNow;

impl Now for SystemNow {
    fn ms(&self) -> i64 {
        chrono::Local::now().timestamp_millis()
    }
}

/// 单测与统计核对用的固定时刻（毫秒）。
pub struct FixedNow(pub i64);

impl Now for FixedNow {
    fn ms(&self) -> i64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_now_uses_local_calendar_boundaries() {
        // 本地 2026-10-28 23:30（UTC+8）= UTC 15:30 —— 按 UTC 分日会归到同一天，
        // 但本地 23:30 与次日 00:10 必须分家，这正是 PLAN §7 那句 date() 的坑。
        let late_evening = day_start_ms(20261028) + 23 * 3600 * 1000 + 30 * 60 * 1000;
        let early_morning = late_evening + 40 * 60 * 1000;
        assert_eq!(day_of_ms(late_evening), 20261028);
        assert_eq!(day_of_ms(early_morning), 20261029);
        assert_eq!(hour_of_ms(late_evening), 23);
    }
}
