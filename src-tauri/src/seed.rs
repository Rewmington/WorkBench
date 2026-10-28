//! 造数据用的种子。`anvil --seed [天数]`（只在 debug 构建里生效）。
//!
//! 为什么要有这个：M7 的验收是「造 90 天种子数据，逐图核对聚合 SQL 与列表一致」，
//! 而 M5 的验收要看「满数据不出现死白」。两者都需要**可复现**的数据 —— 用随机数但
//! 固定种子，这样两次截图能逐像素对比。

use chrono::Datelike;
use rusqlite::Connection;
use uuid::Uuid;

use crate::error::Result;
use crate::repos::{settings, tags, todos};
use crate::sync::lww::Stamp;
use crate::time::{days, Now};

/// 线性同余：不引 rand 也能每次跑出同一串数，且测试里可预测。
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }

    fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }
}

const TITLES: &[&str] = &[
    "回复消息",
    "写周报",
    "买猫粮",
    "预约牙医",
    "整理备份",
    "交房租",
    "看文档",
    "修 CI",
    "倒垃圾",
    "买电池",
    "约体检",
    "清相册",
    "读论文",
    "升级依赖",
    "写测试",
    "订车票",
];

const TAGS: &[&str] = &["家里", "工作", "跑腿", "电脑"];

/// 生成 `days` 天的历史 + 一批活动待办。返回插了多少条，供日志与断言用。
pub fn populate(
    conn: &mut Connection,
    now: &dyn Now,
    device: &str,
    days_back: i32,
) -> Result<usize> {
    let mut rng = Lcg(20_261_028);
    let today = now.today();
    let mut inserted = 0usize;

    // 已经种过就不再种：重复跑 --seed 会把库塞满重复条目
    let existing: i64 = conn.query_row("SELECT count(*) FROM todos", [], |r| r.get(0))?;
    if existing > 0 {
        return Ok(0);
    }

    let tx = conn.transaction()?;

    for offset in 0..days_back {
        let day = shift_day(today, -offset);
        let day_start = days::day_start_ms(day);
        // 密度故意不均：周末少、周中多，个别天完全空 —— 热力图要能看出断点
        let weekday = weekday_of(day);
        let base: i32 = if weekday >= 5 { 1 } else { 3 };
        let count = base.saturating_sub(rng.below(2) as i32);

        for i in 0..count {
            let created_hour = 8 + rng.below(13) as i32;
            let created_at =
                day_start + i64::from(created_hour) * 3_600_000 + i64::from(i) * 60_000;
            // 约三成是「补做」：截止时间在前一天，完成时间在今天
            let late = rng.below(10) < 3;
            let due_day = if late { shift_day(day, -1) } else { day };
            let due_at = days::day_start_ms(due_day) + 18 * 3_600_000;
            let done_at = created_at + rng.below(3 * 3600) as i64 * 1000;

            let title = TITLES[(rng.below(TITLES.len() as u64)) as usize].to_owned();
            let id = Uuid::now_v7().simple().to_string();
            let stamp = Stamp::new(id.clone(), created_at, device);
            tx.execute(
                "INSERT INTO todos (id, rev, created_at, created_by, updated_at, updated_by,
                                    title, note, done_at, done_day, due_at, priority, pinned,
                                    sort_key, zone, created_day, created_hour)
                 VALUES (?1,1,?2,?3,?4,?3,?5,'',?6,?7,?8,?9,0,?10,'today',?11,?12)",
                rusqlite::params![
                    stamp.id,
                    stamp.created_at,
                    stamp.created_by,
                    done_at,
                    title,
                    done_at,
                    days::day_of_ms(done_at),
                    due_at,
                    rng.below(4) as i32,
                    (i as i64 + 1) * 1024,
                    day,
                    created_hour
                ],
            )?;

            if rng.below(3) == 0 {
                let name = TAGS[(rng.below(TAGS.len() as u64)) as usize];
                tags::set_todo_tags(
                    &tx,
                    &crate::time::FixedNow(created_at),
                    device,
                    &id,
                    &[name.to_owned()],
                )?;
            }
            inserted += 1;
        }
    }

    // 活动待办：让首屏与队列有东西可显示，并覆盖逾期 / 置顶 / 子项 / 无截止这几种状态
    let live = [
        ("today", "把 M2 的验收跑完", 2, 2 * 60),
        ("today", "回复房东的微信", 1, -180),
        ("inbox", "预约牙医（下周三之前）", 0, -2_600),
        ("later", "整理备份目录", 2, 0),
        ("someday", "学一下 3D 环的降级写法", 3, 0),
        ("today", "买猫粮（大袋）", 2, 300),
    ];
    for (i, (zone, title, priority, due_delta_min)) in live.iter().enumerate() {
        let created = now.ms() - (i as i64 + 1) * 3_600_000;
        let due_at = if *due_delta_min == 0 {
            None
        } else {
            Some(now.ms() + i64::from(*due_delta_min) * 60_000)
        };
        todos::create(
            &tx,
            &crate::time::FixedNow(created),
            device,
            crate::model::TodoDraft {
                title: (*title).to_owned(),
                zone: Some((*zone).to_owned()),
                priority: Some(*priority),
                due_at,
                ..Default::default()
            },
        )?;
        inserted += 1;
    }

    // 一条父 + 两条子：详情面板的子项区要有东西
    let parent = todos::create(
        &tx,
        &crate::time::FixedNow(now.ms() - 86_400_000),
        device,
        crate::model::TodoDraft {
            title: "装修：卫浴".to_owned(),
            zone: Some("later".to_owned()),
            ..Default::default()
        },
    )?;
    for child in ["买角阀", "约测量"] {
        todos::create(
            &tx,
            &crate::time::FixedNow(now.ms() - 43_200_000),
            device,
            crate::model::TodoDraft {
                title: child.to_owned(),
                parent_id: Some(parent.stamp.id.clone()),
                zone: Some("later".to_owned()),
                ..Default::default()
            },
        )?;
        inserted += 1;
    }

    tx.commit()?;

    settings::set_json(
        conn,
        "seed.days",
        &serde_json::json!(days_back),
        now.ms(),
        device,
    )?;
    Ok(inserted)
}

fn shift_day(day: i32, delta: i32) -> i32 {
    let mut current = day;
    for _ in 0..delta.abs() {
        current = if delta > 0 {
            days::next_day(current)
        } else {
            days::prev_day(current)
        };
    }
    current
}

/// 1=周一 … 7=周日
fn weekday_of(day: i32) -> i32 {
    let ms = days::day_start_ms(day);
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|utc| {
            utc.with_timezone(&chrono::Local)
                .weekday()
                .number_from_monday() as i32
        })
        .unwrap_or(1)
}
