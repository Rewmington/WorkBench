//! 药箱库存与今日时段的算法。
//!
//! 这是**另一个项目的领域逻辑**（`medicine/lib/models.dart`）的直译，不是我们自己的口径：
//! 两个程序对「还剩几天」给出不同数字，用户就会不知道该信谁。所以这里逐条对齐，
//! 并把它的 `test/checkin_slot_test.dart` 三个用例原样搬过来当回归位。
//!
//! 只读：本模块不写药箱的库，也不写它的 `medicine-sync/` 目录（PLAN §6.2 的硬边界）。

use serde::Serialize;

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
}
