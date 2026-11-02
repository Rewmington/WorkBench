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

/// 本机写出时间戳的**单调下限**。`Now` 是时间唯一的入口（见上面那段注释），
/// 所以只需要在这一处夹住。
static FLOOR_MS: std::sync::atomic::AtomicI64 = std::sync::atomic::AtomicI64::new(0);
static ROLLBACK_WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// 墙钟与下限之间取哪一个。**同一毫秒的多次写入不该 +1**，只有真回拨才前进一格。
/// 抽成纯函数是因为墙钟没法在测试里伪造，而这条规则本身可以。
fn forward_of(wall: i64, floor: i64) -> i64 {
    if wall >= floor {
        wall
    } else {
        floor + 1
    }
}

/// 用本机已经写过的最旧基准垫高下限，让单调性跨重启也成立（**只读**，不改库）。
///
/// 调用方给的是**本机自己**写过的最大 `updated_at`，不是全库最大值：对端设备如果时钟
/// 超前，它写来的行会把我们的下限顶到未来，之后本机每一条写入都会白赢过那台机器 ——
/// 那是把别人的错变成自己的优势，比回拨更难查。
pub fn raise_floor(ms: i64) {
    FLOOR_MS.fetch_max(ms, std::sync::atomic::Ordering::Relaxed);
}

impl Now for SystemNow {
    fn ms(&self) -> i64 {
        let wall = chrono::Local::now().timestamp_millis();
        let floor = FLOOR_MS.load(std::sync::atomic::Ordering::Relaxed);
        let issued = forward_of(wall, floor);
        FLOOR_MS.fetch_max(issued, std::sync::atomic::Ordering::Relaxed);
        if issued != wall && !ROLLBACK_WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            // 说一次就够：真实时间迟早追上，但用户需要知道这段时间戳为什么「跑在未来」。
            log::warn!(
                "系统时间({wall}) 比本机写过的最晚时刻({floor}) 更早 —— 可能是时钟被改回去或虚拟机恢复。\
                 这台机器写出的时间戳已改为继续递增，等真实时间追上就恢复正常；\
                 否则新改动会被旧数据覆盖，而那条还会被标成「已同步」从此不再重发。"
            );
        }
        issued
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
    fn a_rolled_back_clock_never_issues_an_older_timestamp() {
        // 墙钟没法伪造，但这条规则本身能钉住：`SystemNow::ms()` 用的就是它
        assert_eq!(forward_of(200, 100), 200, "正常前进就照墙钟");
        assert_eq!(forward_of(100, 100), 100, "同一毫秒里的多次写入不该 +1");
        assert_eq!(forward_of(50, 100), 101, "回拨时往前走一格，而不是退回 50");
        assert_eq!(forward_of(i64::MIN, 100), 101);
    }

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
