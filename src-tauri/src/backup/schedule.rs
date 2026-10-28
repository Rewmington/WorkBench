//! 每日备份的调度。一条 60 秒的线程，不引 tokio，也不依赖 webview 存活。
//!
//! 「今天还没成功过」是唯一判据，所以电脑关了一整天之后再开机，一分钟内会补上；
//! 失败按指数退避，每次尝试都写 backup_log，**不弹窗**（PLAN §5.2）。

use std::time::{Duration, Instant};

use rusqlite::Connection;
use tauri::{AppHandle, Emitter, Manager};

use crate::backup;
use crate::error::Result;
use crate::state::AppState;
use crate::time::{Now, SystemNow};

const TICK: Duration = Duration::from_secs(60);
const MAX_BACKOFF: Duration = Duration::from_secs(30 * 60);

pub const EVENT: &str = "backup://result";

pub fn spawn(app: AppHandle) {
    let builder = std::thread::Builder::new().name("anvil-backup".into());
    if let Err(e) = builder.spawn(move || loop_(&app)) {
        log::warn!("备份调度线程没起来：{e}");
    }
}

fn loop_(app: &AppHandle) {
    let started = Instant::now();
    let mut next_attempt = started + TICK;

    loop {
        let now_instant = Instant::now();
        if now_instant < next_attempt {
            std::thread::sleep((next_attempt - now_instant).min(TICK));
            continue;
        }
        // 刚起来的 60 秒里先让首屏与联动跑完，别抢启动时的磁盘
        if started.elapsed() < TICK {
            next_attempt = Instant::now() + TICK;
            continue;
        }

        match attempt(app) {
            Ok(Some(outcome)) => {
                let _ = app.emit(EVENT, &outcome);
                if outcome.ok || outcome.skipped {
                    next_attempt = Instant::now() + TICK;
                } else {
                    let fails = app
                        .state::<AppState>()
                            .backup_failures
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    let backoff = TICK * 2u32.pow(fails.min(5));
                    next_attempt = Instant::now() + backoff.min(MAX_BACKOFF);
                    log::warn!("备份失败，{} 秒后重试：{}", backoff.as_secs(), outcome.error.unwrap_or_default());
                }
            }
            Ok(None) => next_attempt = Instant::now() + TICK,
            Err(e) => {
                log::warn!("备份调度读库失败：{e}");
                next_attempt = Instant::now() + TICK;
            }
        }
    }
}

/// 返回 None 表示「今天已经成功过，这次不该跑」。
/// 没配凭据不用在这儿判断 —— `backup::run` 会立刻返回 skipped，不记流水也不退避。
fn attempt(app: &AppHandle) -> Result<Option<backup::Outcome>> {
    let state = app.state::<AppState>();
    let now = SystemNow;

    let done_today = {
        let conn = state.lock();
        last_success(&conn).is_some_and(|at| now.day_of(at) == now.today())
    };
    if done_today {
        return Ok(None);
    }

    let conn = state.lock();
    let outcome = backup::run(&conn, &state.paths.tmp, &now, "daily");
    if outcome.ok {
        state
            .backup_failures
            .store(0, std::sync::atomic::Ordering::Relaxed);
    }
    Ok(Some(outcome))
}

/// 上次成功的时间，供侧栏那张「备份状态」卡用。
pub fn last_success(conn: &Connection) -> Option<i64> {
    conn.query_row(
        "SELECT max(at) FROM backup_log WHERE ok = 1 AND kind IN ('daily','manual')",
        [],
        |r| r.get::<_, Option<i64>>(0),
    )
    .unwrap_or(None)
}
