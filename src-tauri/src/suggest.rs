//! 「下一步建议」= 五条硬规则按序取前三（PLAN §8.8.4）。
//!
//! 保留参考图那块版式，但里面**没有模型、没有网络**：每条规则是一个
//! `fn(&Connection, &dyn Now) -> Option<Suggestion>`，全部本地 SQL + 比较，
//! 因此可单测、可复现（时间从 `Now` 注入，不在这里读系统钟）。
//!
//! 规则集中在下面的 `RULES` 表里 —— 加一条规则就往表里加一行，
//! 组件只认 `Suggestion` 的形状，不认规则本身。

use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::Result;
use crate::model::todo::zones;
use crate::time::Now;

/// 一次最多给三条。再多就和「规则驱动」这件事本身一样，变成噪声。
pub const LIMIT: usize = 3;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
    /// 规则 id：稳定、可埋点、可单测断言，不要让前端去猜是哪条规则命中
    pub rule: &'static str,
    pub headline: String,
    pub detail: String,
    /// 点一下直达的对象与视图。两者都可以为 None（例如只让人去整理捕获）。
    pub todo_id: Option<String>,
    pub view: Option<String>,
}

type Rule = (&'static str, fn(&Connection, &dyn Now) -> Result<Option<Suggestion>>);

/// 顺序即优先级（§8.8.4 ①→⑤）。
pub const RULES: &[Rule] = &[
    ("overdue-oldest", overdue_oldest),
    ("due-today-untouched", due_today_untouched),
    ("countdown-short", countdown_short),
    ("medicine-low", medicine_low),
    ("inbox-pileup", inbox_pileup),
];

pub fn top(conn: &Connection, now: &dyn Now) -> Result<Vec<Suggestion>> {
    let mut out = Vec::new();
    for (_, rule) in RULES {
        if out.len() >= LIMIT {
            break;
        }
        if let Some(s) = rule(conn, now)? {
            out.push(s);
        }
    }
    Ok(out)
}

/// ① 逾期最久的一条。逾期是「已经在欠」，比任何新任务都优先。
fn overdue_oldest(conn: &Connection, now: &dyn Now) -> Result<Option<Suggestion>> {
    let hit = conn
        .query_row(
            "SELECT id, title, due_at FROM todos
             WHERE deleted_at IS NULL AND completed = 0 AND due_at IS NOT NULL AND due_at < ?1
             ORDER BY due_at ASC LIMIT 1",
            params![now.day_start(now.today())],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)),
        )
        .ok();
    let Some((id, title, due)) = hit else { return Ok(None) };
    let days_late = crate::time::days::days_between(now.day_of(due), now.today()).max(0);
    Ok(Some(Suggestion {
        rule: "overdue-oldest",
        headline: title,
        detail: if days_late <= 0 { "今天已经过了截止时间".into() } else { format!("已逾期 {days_late} 天，欠得最久的一条") },
        todo_id: Some(id),
        view: Some("todos".into()),
    }))
}

/// ② 今天到期且**建完就没动过**的最高优先级项。
/// 「没动过」= `updated_at = created_at`：一次写入都没发生，说明真的还没开始。
fn due_today_untouched(conn: &Connection, now: &dyn Now) -> Result<Option<Suggestion>> {
    let t0 = now.day_start(now.today());
    let t1 = crate::time::days::day_end_ms(now.today());
    let hit = conn
        .query_row(
            "SELECT id, title FROM todos
             WHERE deleted_at IS NULL AND completed = 0
               AND due_at BETWEEN ?1 AND ?2 AND updated_at = created_at
             ORDER BY priority ASC, sort_key ASC LIMIT 1",
            params![t0, t1],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .ok();
    Ok(hit.map(|(id, title)| Suggestion {
        rule: "due-today-untouched",
        headline: title,
        detail: "今天到期，建完还没动过".into(),
        todo_id: Some(id),
        view: Some("todos".into()),
    }))
}

/// ③ 剩余不足 2 小时的运行中倒计时。
fn countdown_short(conn: &Connection, now: &dyn Now) -> Result<Option<Suggestion>> {
    let ms = now.ms();
    let hit = conn
        .query_row(
            "SELECT label, ends_at FROM countdowns
             WHERE deleted_at IS NULL AND state = 'running' AND paused = 0
               AND ends_at IS NOT NULL AND ends_at > ?1 AND ends_at <= ?2
             ORDER BY ends_at ASC LIMIT 1",
            params![ms, ms + 2 * 3600 * 1000],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)),
        )
        .ok();
    let Some((label, ends)) = hit else { return Ok(None) };
    let mins = ((ends - ms) / 60_000).max(0);
    Ok(Some(Suggestion {
        rule: "countdown-short",
        headline: if label.trim().is_empty() { "倒计时".into() } else { label },
        detail: format!("只剩 {} 分", if mins < 1 { 0 } else { mins }),
        todo_id: None,
        view: Some("progress".into()),
    }))
}

/// ④ 药箱库存不足 2 天（只读源，§6.1）。读不到缓存就什么也不说 —— 绝不猜。
fn medicine_low(conn: &Connection, _now: &dyn Now) -> Result<Option<Suggestion>> {
    let payload: Option<String> = conn
        .query_row(
            "SELECT payload_json FROM linked_snapshots
             WHERE source = 'medicine' AND kind = 'today' AND status = 'ok'",
            [],
            |r| r.get(0),
        )
        .ok();
    let Some(json) = payload else { return Ok(None) };

    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(v) => v,
        Err(_) => return Ok(None),
    };
    let worst = value
        .get("items")
        .and_then(|v| v.as_array())
        .and_then(|items| {
            items
                .iter()
                .filter_map(|it| {
                    let left = it.get("daysLeft")?.as_f64()?;
                    if left >= 2.0 {
                        return None;
                    }
                    Some((left, it.get("name")?.as_str()?.to_owned()))
                })
                .min_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal))
        });

    Ok(worst.map(|(left, name)| Suggestion {
        rule: "medicine-low",
        headline: format!("{name} 快吃完了"),
        detail: format!("库存约够 {left:.1} 天，可以生成一条「补买」待办"),
        todo_id: None,
        view: Some("medicine".into()),
    }))
}

/// ⑤ 未整理捕获超过 3 条。inbox 堆着不处理是这类系统最常见的死法。
fn inbox_pileup(conn: &Connection, _now: &dyn Now) -> Result<Option<Suggestion>> {
    let n: i64 = conn
        .query_row(
            "SELECT count(*) FROM todos WHERE deleted_at IS NULL AND completed = 0 AND zone = ?1",
            params![zones::INBOX],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok((n > 3).then(|| Suggestion {
        rule: "inbox-pileup",
        headline: "整理捕获".into(),
        detail: format!("{n} 条还在收件箱里没定日子"),
        todo_id: None,
        view: Some("todos".into()),
    }))
}
