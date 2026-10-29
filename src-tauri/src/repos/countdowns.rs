//! 倒计时：四种模式共用一张表，到期时刻统一折算进 `ends_at`。
//!
//! 折算放在 `deadline_for` 这个纯函数里，是为了让 daily / interval 这种「滚动到期」
//! 的规则可单测 —— 否则它只能靠等真时间过去来验证。
//!
//! `ends_at` 是**物化**的：每次状态变化与每次读取都会把它刷新。这样 §8.8.4 的第 ③ 条
//! 规则（倒计时 < 2 小时）能直接用一条 SQL 问出来，而不用把全部倒计时读进内存再筛。

use rusqlite::{params, Connection, Row};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::error::{Error, Result};
use crate::model::{cd_mode, cd_state, Countdown, CountdownDraft};
use crate::sync::lww::{touch, Stamp};
use crate::time::{days, Now};

const COLS: &str = "id, rev, created_at, created_by, updated_at, updated_by, deleted_at, \
     label, mode, spec, ends_at, paused, state, remaining_ms";

fn map_row(r: &Row) -> rusqlite::Result<Countdown> {
    let spec_text: String = r.get("spec")?;
    let spec = serde_json::from_str(&spec_text).unwrap_or_else(|_| json!({}));
    Ok(Countdown {
        stamp: Stamp {
            id: r.get("id")?,
            rev: r.get("rev")?,
            created_at: r.get("created_at")?,
            created_by: r.get("created_by")?,
            updated_at: r.get("updated_at")?,
            updated_by: r.get("updated_by")?,
            deleted_at: r.get("deleted_at")?,
        },
        label: r.get("label")?,
        mode: r.get("mode")?,
        spec,
        ends_at: r.get("ends_at")?,
        paused: r.get::<_, i64>("paused")? != 0,
        state: r.get("state")?,
        remaining_ms: r.get("remaining_ms")?,
    })
}

fn num(spec: &Value, key: &str) -> Option<i64> {
    spec.get(key).and_then(|v| v.as_f64()).map(|v| v as i64)
}

/// 「HH:MM」→ 当日已过分钟数。格式不对就 None，让调用方退回 idle 而不是崩。
fn clock_minutes(raw: &str) -> Option<i32> {
    let (h, m) = raw.split_once(':')?;
    let h: i32 = h.parse().ok()?;
    let m: i32 = m.parse().ok()?;
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// 某模式下，`now` 之后的**下一个**到期时刻。跑完一轮的 interval / daily 也走这里滚动。
pub fn deadline_for(mode: &str, spec: &Value, now: &dyn Now) -> Option<i64> {
    let ms = now.ms();
    match mode {
        cd_mode::DURATION => {
            let minutes = num(spec, "minutes")?;
            let started = num(spec, "startedAt")?;
            Some(started + minutes * 60_000)
        }
        cd_mode::TIMED => num(spec, "until"),
        cd_mode::DAILY => {
            let at = spec.get("at")?.as_str().and_then(clock_minutes)?;
            let today = now.day_start(now.today()) + i64::from(at) * 60_000;
            if today > ms {
                return Some(today);
            }
            // 今天的点已经过了 → 明天同一个点。加一天走日历而不是 +86400000，
            // 否则 DST 切换日会漂一小时。
            Some(now.day_start(days::next_day(now.today())) + i64::from(at) * 60_000)
        }
        cd_mode::INTERVAL => {
            let every = num(spec, "everyMinutes")?.max(1) * 60_000;
            let started = num(spec, "startedAt")?;
            if ms < started {
                return Some(started + every);
            }
            Some(started + (ms - started) / every * every + every)
        }
        _ => None,
    }
}

fn write_row(conn: &Connection, cd: &Countdown) -> Result<()> {
    conn.execute(
        "UPDATE countdowns SET rev=?2, updated_at=?3, updated_by=?4, label=?5, spec=?6,
            ends_at=?7, paused=?8, state=?9, remaining_ms=?10
         WHERE id=?1",
        params![
            cd.stamp.id,
            cd.stamp.rev,
            cd.stamp.updated_at,
            cd.stamp.updated_by,
            cd.label,
            cd.spec.to_string(),
            cd.ends_at,
            cd.paused as i64,
            cd.state,
            cd.remaining_ms
        ],
    )?;
    Ok(())
}

fn get_row(conn: &Connection, id: &str) -> Result<Option<Countdown>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM countdowns WHERE id = ?1 AND deleted_at IS NULL"
    ))?;
    let found = stmt
        .query_map(params![id], map_row)?
        .next()
        .transpose()?;
    Ok(found)
}

fn require(conn: &Connection, id: &str) -> Result<Countdown> {
    get_row(conn, id)?.ok_or_else(|| Error::msg("这个倒计时不存在或已被删除"))
}

pub fn list(conn: &Connection, now: &dyn Now) -> Result<Vec<Countdown>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM countdowns WHERE deleted_at IS NULL ORDER BY created_at ASC"
    ))?;
    let rows = stmt.query_map([], map_row)?;
    let mut out = Vec::new();
    for row in rows {
        out.push(row?);
    }
    drop(stmt);
    // 到期时刻是滚动的：读一次就把过期的 daily / interval 推到下一轮，
    // 并把走完的 duration 落成 finished。
    for cd in &mut out {
        roll(cd, conn, now)?;
    }
    Ok(out)
}

/// 推进一支钟。只在 `ends_at` 真的变了的时候写库，避免每次读都推高 rev。
fn roll(cd: &mut Countdown, conn: &Connection, now: &dyn Now) -> Result<()> {
    if cd.paused || cd.state == cd_state::IDLE {
        return Ok(());
    }
    let Some(next) = deadline_for(&cd.mode, &cd.spec, now) else { return Ok(()) };
    if cd.mode == cd_mode::DURATION || cd.mode == cd_mode::TIMED {
        // 这两种模式没有下一轮：到点就是 finished
        if now.ms() >= next && cd.state != cd_state::FINISHED {
            cd.state = cd_state::FINISHED.into();
            cd.ends_at = Some(next);
            write_row(conn, cd)?;
        }
        return Ok(());
    }
    if cd.ends_at != Some(next) {
        cd.ends_at = Some(next);
        write_row(conn, cd)?;
    }
    Ok(())
}

pub fn create(conn: &Connection, now: &dyn Now, device: &str, draft: CountdownDraft) -> Result<Countdown> {
    if !matches!(
        draft.mode.as_str(),
        cd_mode::DURATION | cd_mode::TIMED | cd_mode::DAILY | cd_mode::INTERVAL
    ) {
        return Err(Error::msg("未知的倒计时模式"));
    }
    let label = draft.label.trim().to_owned();
    if label.is_empty() {
        return Err(Error::msg("倒计时要有个名字"));
    }
    let ms = now.ms();
    let stamp = Stamp::new(Uuid::now_v7().simple().to_string(), ms, device);
    conn.execute(
        "INSERT INTO countdowns (id, rev, created_at, created_by, updated_at, updated_by,
                                 deleted_at, label, mode, spec, ends_at, paused, state, remaining_ms)
         VALUES (?1,?2,?3,?4,?5,?6,NULL,?7,?8,?9,NULL,0,?10,NULL)",
        params![
            stamp.id,
            stamp.rev,
            stamp.created_at,
            stamp.created_by,
            stamp.updated_at,
            stamp.updated_by,
            label,
            draft.mode,
            draft.spec.to_string(),
            cd_state::IDLE
        ],
    )?;
    Ok(Countdown {
        stamp,
        label,
        mode: draft.mode,
        spec: draft.spec,
        ends_at: None,
        paused: false,
        state: cd_state::IDLE.into(),
        remaining_ms: None,
    })
}

/// 开始 / 重新开始。duration 与 interval 把 `startedAt` 写进 spec，
/// 于是「从哪儿算起」这件事跟着行一起被备份与同步，不依赖本机某处的临时状态。
pub fn start(conn: &Connection, now: &dyn Now, device: &str, id: &str) -> Result<Countdown> {
    let ms = now.ms();
    let mut cd = require(conn, id)?;
    if matches!(cd.mode.as_str(), cd_mode::DURATION | cd_mode::INTERVAL) {
        cd.spec = json!({
            "minutes": num(&cd.spec, "minutes"),
            "everyMinutes": num(&cd.spec, "everyMinutes"),
            "startedAt": ms,
            "until": cd.spec.get("until").cloned(),
            "at": cd.spec.get("at").cloned(),
        });
        // 把上面 json! 里为 None 的键剔掉，免得 spec 里堆一串 null
        if let Some(map) = cd.spec.as_object_mut() {
            map.retain(|_, v| !v.is_null());
        }
    }
    cd.state = cd_state::RUNNING.into();
    cd.paused = false;
    cd.remaining_ms = None;
    cd.ends_at = deadline_for(&cd.mode, &cd.spec, now);
    touch(&mut cd.stamp, ms, device);
    write_row(conn, &cd)?;
    Ok(cd)
}

/// 暂停 = 冻住剩余毫秒。见 0003：不冻的话「暂停」只是个骗人的按钮。
pub fn pause(conn: &Connection, now: &dyn Now, device: &str, id: &str) -> Result<Countdown> {
    let ms = now.ms();
    let mut cd = require(conn, id)?;
    if cd.paused || cd.state != cd_state::RUNNING {
        return Ok(cd);
    }
    cd.remaining_ms = Some(cd.ends_at.map(|e| (e - ms).max(0)).unwrap_or(0));
    cd.paused = true;
    cd.state = cd_state::PAUSED.into();
    touch(&mut cd.stamp, ms, device);
    write_row(conn, &cd)?;
    Ok(cd)
}

/// 继续 = 用冻住的剩余重算到期时刻。
pub fn resume(conn: &Connection, now: &dyn Now, device: &str, id: &str) -> Result<Countdown> {
    let ms = now.ms();
    let mut cd = require(conn, id)?;
    if !cd.paused {
        return Ok(cd);
    }
    let remaining = cd.remaining_ms.take().unwrap_or(0);
    cd.ends_at = match cd.mode.as_str() {
        cd_mode::DURATION | cd_mode::TIMED => Some(ms + remaining),
        // 滚动型没有「剩余」可言：继续就是回到下一轮，重算即可
        _ => deadline_for(&cd.mode, &cd.spec, now),
    };
    cd.paused = false;
    cd.state = cd_state::RUNNING.into();
    touch(&mut cd.stamp, ms, device);
    write_row(conn, &cd)?;
    Ok(cd)
}

pub fn reset(conn: &Connection, now: &dyn Now, device: &str, id: &str) -> Result<Countdown> {
    let ms = now.ms();
    let mut cd = require(conn, id)?;
    cd.state = cd_state::IDLE.into();
    cd.paused = false;
    cd.ends_at = None;
    cd.remaining_ms = None;
    touch(&mut cd.stamp, ms, device);
    write_row(conn, &cd)?;
    Ok(cd)
}

pub fn remove(conn: &Connection, now: &dyn Now, device: &str, id: &str) -> Result<()> {
    let ms = now.ms();
    let mut cd = require(conn, id)?;
    cd.stamp.deleted_at = Some(ms);
    touch(&mut cd.stamp, ms, device);
    conn.execute(
        "UPDATE countdowns SET rev=?2, updated_at=?3, updated_by=?4, deleted_at=?5 WHERE id=?1",
        params![cd.stamp.id, cd.stamp.rev, ms, device, ms],
    )?;
    Ok(())
}
