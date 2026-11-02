//! 药箱库存与今日时段的算法。
//!
//! 这是**另一个项目的领域逻辑**（`medicine/lib/models.dart`）的直译，不是我们自己的口径：
//! 两个程序对「还剩几天」给出不同数字，用户就会不知道该信谁。所以这里逐条对齐，
//! 并把它的 `test/checkin_slot_test.dart` 三个用例原样搬过来当回归位。
//!
//! 只读：本模块不写药箱的库，也不写它的 `medicine-sync/` 目录（PLAN §6.2 的硬边界）。

use serde::Serialize;

use crate::time::days::{day_of_ms, day_start_ms, days_between, shift};

/// 「近 7 天」这一个窗口长度写在这里，桌面与手机端各自照搬时只有一个数要对。
pub const WEEK_DAYS: i64 = 7;

#[derive(Debug, Clone)]
pub struct Medicine {
    pub id: String,
    pub name: String,
    pub per_box: f64,
    pub remaining: f64,
    pub doses_per_day: f64,
    pub amount_per_dose: f64,
    pub unit: String,
    pub low_days: i32,
    /// 逗号分隔的 "07:30,21:00"；空 = 按次数自动生成
    pub times: String,
    pub created_at: i64,
}

impl Medicine {
    /// 库存以 `remaining`（单位个数）为唯一真相，`boxes` 只是展示值；
    /// 老数据没有 remaining 时才回退成 boxes × per_box。
    ///
    /// 直接吃 `rusqlite::Row`：11 个位置参数的构造函数迟早会有一处顺序写错，
    /// 而按列名取不会。
    pub fn from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Medicine> {
        let per_box: Option<f64> = r.get("per_box")?;
        let boxes: Option<f64> = r.get("boxes")?;
        let remaining: Option<f64> = r.get("remaining")?;
        Ok(Self {
            id: r.get("id")?,
            name: r.get("name")?,
            per_box: per_box.unwrap_or(0.0),
            remaining: remaining.unwrap_or_else(|| boxes.unwrap_or(0.0) * per_box.unwrap_or(0.0)),
            doses_per_day: r.get::<_, Option<f64>>("doses_per_day")?.unwrap_or(0.0),
            amount_per_dose: r.get::<_, Option<f64>>("amount_per_dose")?.unwrap_or(0.0),
            unit: r.get::<_, Option<String>>("unit")?.unwrap_or_else(|| "片".into()),
            low_days: r.get::<_, Option<i32>>("low_days")?.unwrap_or(7),
            times: r.get::<_, Option<String>>("times")?.unwrap_or_default(),
            created_at: r.get::<_, i64>("created_at").unwrap_or(0),
        })
    }

    pub fn daily_use(&self) -> f64 {
        self.doses_per_day * self.amount_per_dose
    }

    /// None = 没设用量，算不出来。这种要说「未设用量」，不能显示成「无限」或「0 天」。
    pub fn days_left(&self) -> Option<f64> {
        if self.daily_use() <= 0.0 {
            None
        } else {
            Some(self.remaining / self.daily_use())
        }
    }

    pub fn is_low(&self) -> bool {
        self.days_left().is_some_and(|d| d <= self.low_days as f64)
    }

    pub fn is_out(&self) -> bool {
        self.remaining <= 0.0
    }

    pub fn schedule_minutes(&self) -> Vec<i32> {
        let mut parsed: Vec<i32> = self
            .times
            .split([',', '，', ';', '；', ' ', '\t', '\n'])
            .filter(|s| s.contains(':'))
            .filter_map(|s| {
                let mut parts = s.split(':');
                let (h, m) = (parts.next()?, parts.next()?);
                if parts.next().is_some() {
                    return None;
                }
                let (h, m) = (h.parse::<i32>().ok()?, m.parse::<i32>().ok()?);
                if h > 23 || m > 59 {
                    return None;
                }
                Some(h * 60 + m)
            })
            .collect();
        if !parsed.is_empty() {
            parsed.sort_unstable();
            return parsed;
        }

        let n = self.doses_per_day.round() as i32;
        if n <= 0 {
            return Vec::new();
        }
        if n == 1 {
            return vec![8 * 60];
        }
        let (start, end) = (7 * 60 + 30, 21 * 60);
        (0..n)
            .map(|i| (start as f64 + (end - start) as f64 * i as f64 / (n - 1) as f64).round() as i32)
            .collect()
    }
}

#[derive(Debug, Clone)]
pub struct Dose {
    pub medicine_id: String,
    pub taken_at: i64,
    pub amount: f64,
}

impl Dose {
    /// 0 剂量 = 明确标记「这顿没吃」：不扣库存，也不进依从率的分子分母。
    pub fn is_skipped(&self) -> bool {
        self.amount <= 0.0
    }
}

/// 打卡时刻一对一分配到槽位，**就近优先**。
///
/// 不是「从早到晚依次点亮」：只打了 20:00 的卡时该亮 20:00 那一格，
/// 否则补录清单会误报早晨漏打（药箱源码里专门写了这条理由）。
pub fn match_slots(slot_minutes: &[i32], taken_at: &[i64], day_start_ms: i64) -> Vec<Option<usize>> {
    let slots: Vec<i64> = slot_minutes
        .iter()
        .map(|m| day_start_ms + i64::from(*m) * 60_000)
        .collect();
    let mut result: Vec<Option<usize>> = vec![None; slots.len()];
    let mut used = vec![false; slots.len()];

    let mut ordered: Vec<usize> = (0..taken_at.len()).collect();
    ordered.sort_by_key(|&i| taken_at[i]);

    for log_index in ordered {
        let at = taken_at[log_index];
        let mut best: Option<usize> = None;
        let mut best_dist = i64::MAX;
        for (slot, slot_at) in slots.iter().enumerate() {
            if used[slot] {
                continue;
            }
            let dist = (slot_at - at).abs();
            if dist < best_dist {
                best_dist = dist;
                best = Some(slot);
            }
        }
        if let Some(slot) = best {
            used[slot] = true;
            result[slot] = Some(log_index);
        }
    }
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SlotState {
    /// 已记录
    Taken,
    /// 标记没吃
    Skipped,
    /// 已过点但没有任何记录 —— 界面上这是「待记」，不是「漏」，因为用户还能补
    Pending,
    /// 还没到点
    Upcoming,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlotView {
    pub minute: i32,
    pub label: String,
    pub state: SlotState,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TodayPlan {
    pub medicine_id: String,
    pub name: String,
    pub slots: Vec<SlotView>,
    pub taken: usize,
    /// 已到期、且没被标记没吃的格子数
    pub owed: usize,
    pub skipped: usize,
    pub pending: usize,
}

/// 今天这一支药的时段视图。`now_minutes` 是本地当日已过分钟数。
pub fn today_plan(
    med: &Medicine,
    doses_today: &[Dose],
    day_start_ms: i64,
    now_minutes: i32,
) -> TodayPlan {
    let slots: Vec<i32> = med
        .schedule_minutes()
        .into_iter()
        .filter(|m| day_start_ms + i64::from(*m) * 60_000 >= med.created_at)
        .collect();

    let assigned = match_slots(&slots, &doses_today.iter().map(|d| d.taken_at).collect::<Vec<_>>(), day_start_ms);

    let mut views = Vec::new();
    let (mut taken, mut skipped, mut pending) = (0usize, 0usize, 0usize);

    for (i, minute) in slots.iter().enumerate() {
        let state = match assigned[i] {
            Some(log_index) => {
                if doses_today[log_index].is_skipped() {
                    skipped += 1;
                    SlotState::Skipped
                } else {
                    taken += 1;
                    SlotState::Taken
                }
            }
            None if *minute > now_minutes => SlotState::Upcoming,
            None => {
                pending += 1;
                SlotState::Pending
            }
        };
        views.push(SlotView {
            minute: *minute,
            label: format!("{:02}:{:02}", minute / 60, minute % 60),
            state,
        });
    }

    // 与 computeStats 同口径：标记没吃的那格从「应服」里扣掉
    let owed = slots.len().saturating_sub(skipped);

    TodayPlan {
        medicine_id: med.id.clone(),
        name: med.name.clone(),
        slots: views,
        taken,
        owed,
        skipped,
        pending,
    }
}

/// 一段窗口里的服药统计。字段与算法都照药箱 `PeriodStats`（`models.dart`），
/// 因为这是健康数据：两个程序对同一个「本周依从」给出不同数字，用户就不知道该信谁。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PeriodStats {
    /// 实服次数
    pub taken: usize,
    /// 应服次数：已到期、且没被标记没吃
    pub owed: usize,
    /// 标记「这顿没吃」的次数
    pub skipped: usize,
}

impl PeriodStats {
    /// 药箱 `PeriodStats.rate` 原样：应服为 0 时，只有「全部格子都标了没吃」才算满，
    /// 压根没有安排就是 0 —— 没有安排不能吹成 100% 依从。
    pub fn rate(&self) -> f64 {
        if self.owed == 0 {
            return if self.skipped > 0 { 1.0 } else { 0.0 };
        }
        (self.taken as f64 / self.owed as f64).clamp(0.0, 1.0)
    }

    /// 有没有安排可统计。false 时 rate 那个 0 不是「漏吃」，界面上不能画成空环骗人。
    pub fn has_plan(&self) -> bool {
        self.owed > 0 || self.skipped > 0
    }
}

/// 近 `days` 个自然日（含今天）的依从统计 = 药箱 `computeStats` 的直译。
///
/// 三条不显然的口径跟着它一起搬：
/// - 建档之前的格子不算（昨天才加的药不该背上前几天的漏吃）；
/// - 今天只算已经过点的格子，进行中的那天不扣分；
/// - 实服数封顶到格子数，同一天多打几次不会把依从率刷过 100%。
///
/// `today` 是本地日历日整数（`yyyymmdd`），逐日窗口靠 [`crate::time::days`] 走日历，
/// 不做毫秒加减。
pub fn stats_over(meds: &[Medicine], doses: &[Dose], today: i32, now_minutes: i32, days: i64) -> PeriodStats {
    let (mut taken, mut owed, mut skipped) = (0usize, 0usize, 0usize);

    for back in (0..days).rev() {
        let day = shift(today, -back);
        let day_start = day_start_ms(day);
        let day_end = day_start_ms(shift(day, 1));
        let is_today = back == 0;

        for med in meds {
            let slots: Vec<i32> = med
                .schedule_minutes()
                .into_iter()
                .filter(|m| day_start + i64::from(*m) * 60_000 >= med.created_at)
                .filter(|m| !(is_today && *m > now_minutes))
                .collect();
            if slots.is_empty() {
                continue;
            }
            let that_day: Vec<&Dose> = doses
                .iter()
                .filter(|d| d.medicine_id == med.id && d.taken_at >= day_start && d.taken_at < day_end)
                .collect();
            let hits = that_day.iter().filter(|d| !d.is_skipped()).count();
            let marked = (that_day.len() - hits).clamp(0, slots.len());
            taken += hits.min(slots.len());
            owed += slots.len() - marked;
            skipped += marked;
        }
    }

    PeriodStats { taken, owed, skipped }
}

/// 连续打卡天数 = 药箱 `streakDays` 原样：只看近 7 天里**有非零剂量记录**的连续日子。
///
/// 今天还没打不算断（早上 8 点的药不该在凌晨就把连续天数清零），
/// 但也没算上 —— 从昨天往回数。
pub fn streak_days(doses: &[Dose], today: i32) -> usize {
    let mut counts = vec![0usize; WEEK_DAYS as usize];
    for d in doses {
        if d.is_skipped() {
            continue; // 标记「这顿没吃」不算打卡
        }
        let back = days_between(day_of_ms(d.taken_at), today);
        if !(0..WEEK_DAYS).contains(&back) {
            continue;
        }
        counts[(WEEK_DAYS - 1 - back) as usize] += 1;
    }

    let mut i = WEEK_DAYS as usize - 1;
    if counts[i] == 0 && i > 0 {
        i -= 1; // 今天还没打不算断
    }
    let mut streak = 0;
    loop {
        if counts[i] == 0 {
            break;
        }
        streak += 1;
        if i == 0 {
            break;
        }
        i -= 1;
    }
    streak
}

/// 库存顺序：快没的那支排最前，这一屏才是「该去补货的清单」而不是按录入顺序的流水。
///
/// 口径照药箱自己的统计页（`stats_page.dart`）：算不出天数的记成 9999 沉底，同值按名字 ——
/// 两个程序排出来的顺序得一样，不然用户对照着看会以为其中一边算错了。
pub fn by_stock_risk(meds: &[Medicine]) -> Vec<&Medicine> {
    const UNKNOWN: f64 = 9999.0;
    let mut ordered: Vec<&Medicine> = meds.iter().collect();
    ordered.sort_by(|a, b| {
        a.days_left().unwrap_or(UNKNOWN)
            .partial_cmp(&b.days_left().unwrap_or(UNKNOWN))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.name.cmp(&b.name))
    });
    ordered
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn med(times: &str, doses: f64, amount: f64, remaining: f64) -> Medicine {
        Medicine {
            id: "m1".into(),
            name: "丁螺环酮".into(),
            per_box: 30.0,
            remaining,
            doses_per_day: doses,
            amount_per_dose: amount,
            unit: "片".into(),
            low_days: 7,
            times: times.into(),
            created_at: 0,
        }
    }

    /// 2026-10-20 本地 00:00（测试里用固定偏移算，避免依赖机器时区）
    fn day_start() -> i64 {
        let base = chrono::Local
            .with_ymd_and_hms(2026, 10, 20, 0, 0, 0)
            .latest()
            .unwrap();
        base.timestamp_millis()
    }

    #[test]
    fn days_left_uses_remaining_over_daily_use() {
        let m = med("07:30,21:00", 2.0, 1.0, 60.0);
        assert_eq!(m.daily_use(), 2.0);
        assert_eq!(m.days_left(), Some(30.0));
        assert!(!m.is_low());
        assert!(med("07:30", 2.0, 1.0, 8.0).is_low());
        assert!(med("07:30", 2.0, 1.0, 0.0).is_out());
        // 没设用量：算不出来就说算不出来
        assert_eq!(med("", 0.0, 0.0, 20.0).days_left(), None);
    }

    #[test]
    fn schedule_falls_back_to_even_spread_when_times_empty() {
        assert_eq!(med("07:30,21:00", 2.0, 1.0, 1.0).schedule_minutes(), vec![450, 1260]);
        assert_eq!(med("", 2.0, 1.0, 1.0).schedule_minutes(), vec![450, 1260]);
        assert_eq!(med("", 1.0, 1.0, 1.0).schedule_minutes(), vec![480]);
        assert_eq!(med("", 0.0, 1.0, 1.0).schedule_minutes(), Vec::<i32>::new());
        // 中文逗号与分号都要认，坏时刻丢掉而不是崩
        assert_eq!(med("08:00，20:00；99:99", 3.0, 1.0, 1.0).schedule_minutes(), vec![480, 1200]);
    }

    #[test]
    fn case1_backfilling_morning_does_not_light_up_the_evening_slot() {
        let m = med("07:30,21:00", 2.0, 1.0, 60.0);
        let at_morning = day_start() + i64::from(7 * 60 + 30) * 60_000;
        let plan = today_plan(
            &m,
            &[Dose { medicine_id: "m1".into(), taken_at: at_morning, amount: 1.0 }],
            day_start(),
            15 * 60 + 30,
        );
        assert_eq!(plan.slots.len(), 2);
        assert_eq!(plan.slots[0].state, SlotState::Taken, "点的那一格该亮");
        assert_eq!(plan.slots[1].state, SlotState::Upcoming, "没点的格子不该被就近匹配带走");
        assert_eq!(plan.taken, 1);
        assert_eq!(plan.owed, 2, "晚上那格还没到，仍算应服");
    }

    #[test]
    fn case2_late_press_lands_on_the_slot_not_the_morning_one() {
        let m = med("07:30,21:00", 2.0, 1.0, 60.0);
        let at_evening = day_start() + i64::from(21 * 60) * 60_000;
        let plan = today_plan(
            &m,
            &[Dose { medicine_id: "m1".into(), taken_at: at_evening, amount: 1.0 }],
            day_start(),
            22 * 60 + 5,
        );
        assert_eq!(plan.slots[0].state, SlotState::Pending, "早上的格没打，仍是待记");
        assert_eq!(plan.slots[1].state, SlotState::Taken);
        assert_eq!(plan.pending, 1);
    }

    #[test]
    fn case3_skip_and_check_in_occupy_the_same_slot_without_colliding() {
        let m = med("07:30,21:00", 2.0, 1.0, 60.0);
        let at_evening = day_start() + i64::from(21 * 60) * 60_000;
        let at_morning = day_start() + i64::from(7 * 60 + 30) * 60_000;
        let plan = today_plan(
            &m,
            &[
                Dose { medicine_id: "m1".into(), taken_at: at_evening, amount: 0.0 },
                Dose { medicine_id: "m1".into(), taken_at: at_morning, amount: 1.0 },
            ],
            day_start(),
            15 * 60 + 30,
        );
        assert_eq!(plan.slots[0].state, SlotState::Taken);
        assert_eq!(plan.slots[1].state, SlotState::Skipped);
        assert_eq!(plan.taken, 1);
        assert_eq!(plan.skipped, 1);
        assert_eq!(plan.owed, 1, "标记没吃的那格不进分母");
    }

    #[test]
    fn slots_before_the_medicine_was_created_are_not_counted() {
        let mut m = med("07:30,21:00", 2.0, 1.0, 60.0);
        m.created_at = day_start() + i64::from(12 * 60) * 60_000;
        let plan = today_plan(&m, &[], day_start(), 22 * 60);
        assert_eq!(plan.slots.len(), 1, "早上那格在建档之前，不该算今天的应服");
        assert_eq!(plan.slots[0].minute, 1260);
    }

    /// 本周窗口里第 `back` 天、当日第 `minute` 分钟
    fn week_at(back: i64, minute: i32) -> i64 {
        day_start_ms(shift(20261020, -back)) + i64::from(minute) * 60_000
    }

    fn dose(at_ms: i64, amount: f64) -> Dose {
        Dose { medicine_id: "m1".into(), taken_at: at_ms, amount }
    }

    #[test]
    fn the_one_running_out_soonest_is_first_and_unknown_dose_sinks() {
        let mut soon = med("07:30", 1.0, 1.0, 3.0);
        soon.name = "二甲双胍".into();
        let mut plenty = med("07:30", 1.0, 1.0, 60.0);
        plenty.name = "钙片".into();
        let mut unknown = med("07:30", 0.0, 0.0, 5.0); // 没设用量：算不出还剩几天
        unknown.name = "益生菌".into();

        let all = [plenty, unknown, soon];
        let names: Vec<&str> = by_stock_risk(&all).iter().map(|m| m.name.as_str()).collect();
        assert_eq!(names, vec!["二甲双胍", "钙片", "益生菌"], "按天数升序，算不出的沉底");
    }

    #[test]
    fn week_rate_counts_only_elapsed_slots_and_caps_at_their_number() {
        // 昨天才建档：再往前那几天压根没有格子，不该背漏吃的账
        let mut m = med("07:30,21:00", 2.0, 1.0, 60.0);
        m.created_at = week_at(1, 0);
        // 昨天两格都打、今早打了、晚上那格还没到（现在 15:00），另外早上多补了一次
        let doses = [
            dose(week_at(1, 450), 1.0),
            dose(week_at(1, 1260), 1.0),
            dose(week_at(0, 450), 1.0),
            dose(week_at(0, 500), 1.0),
        ];
        let s = stats_over(&[m], &doses, 20261020, 15 * 60, WEEK_DAYS);
        assert_eq!((s.taken, s.owed), (3, 3), "没到的格子不算欠，多打的那次也不多给");
        assert!((s.rate() - 1.0).abs() < f64::EPSILON, "同一天打两次不能把依从率刷过 100%");
    }

    #[test]
    fn marked_not_taken_leaves_the_week_denominator_but_still_breaks_a_streak() {
        // 昨天那一格标了「这顿没吃」，今天那格还没到点
        let mut m = med("21:00", 1.0, 1.0, 60.0);
        m.created_at = week_at(1, 0);
        let doses = [dose(week_at(1, 1260), 0.0)];
        let s = stats_over(&[m], &doses, 20261020, 20 * 60, WEEK_DAYS);
        assert_eq!((s.taken, s.owed, s.skipped), (0, 0, 1), "标了没吃的那格不进分母");
        assert!(s.has_plan());
        assert!((s.rate() - 1.0).abs() < f64::EPSILON, "整周只有一格且标了没吃＝没有欠，按药箱算满分");
        assert_eq!(streak_days(&doses, 20261020), 0, "但「没吃」不是打卡，连续天数照断");
    }

    #[test]
    fn no_plan_at_all_is_not_a_hundred_percent() {
        let s = stats_over(&[], &[], 20261020, 22 * 60, WEEK_DAYS);
        assert!(!s.has_plan());
        assert_eq!(s.rate(), 0.0, "没安排过药不能说自己是满分依从");
    }

    #[test]
    fn a_day_not_checked_in_yet_does_not_reset_the_streak() {
        // 昨天、前天都打过，今天（早上 8 点的药还没到点）没打
        let doses = [dose(week_at(1, 480), 1.0), dose(week_at(2, 480), 1.0)];
        assert_eq!(streak_days(&doses, 20261020), 2, "今天还没打不算断，但也不提前 +1");

        let with_today = [doses.as_slice(), &[dose(week_at(0, 480), 1.0)]].concat();
        assert_eq!(streak_days(&with_today, 20261020), 3);

        // 中间空了一天
        let gapped = [dose(week_at(0, 480), 1.0), dose(week_at(2, 480), 1.0)];
        assert_eq!(streak_days(&gapped, 20261020), 1);
    }

    #[test]
    fn a_log_older_than_the_window_neither_counts_nor_breaks_anything() {
        let doses = [dose(week_at(0, 480), 1.0), dose(week_at(9, 480), 1.0)];
        assert_eq!(streak_days(&doses, 20261020), 1, "9 天前那条在窗口外，不该被折进连续天数");
    }
}
