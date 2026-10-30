//! 开机一次同步（PLAN-MOBILE §3.2「时机：打开 app 时 + 手动按钮」里的 PC 侧那一半）。
//!
//! **重试上限：每次开机只试一次，失败不循环。** 这条是刻意的，不是偷懒：
//! 坚果云两个已知坑（§7 第 2 条：PROPFIND 403、微信二次验证）都是持续性状态，
//! 对着它们重试到死只会烧配额、把日志刷成噪声。失败只在设置页留一行状态
//! （`relay::record` 写进 sync.state），下一次机会是用户点「立即同步」，或明天开机。
//! 与备份那条 60 秒心跳 + 指数退避不同：备份错过今天就是丢数据，同步错过一次
//! 只是手机下次打开时看到的旧一点 —— 而手机本来就被设计成接受这一点（§6 验收口径）。

use std::time::Duration;

use tauri::{AppHandle, Manager};

use crate::state::AppState;
use crate::sync::relay;

/// 启动后等多久再试。与备份调度线程同一节拍：刚起来的 60 秒留给首屏与联动，别抢启动时的磁盘。
const STARTUP_DELAY: Duration = Duration::from_secs(60);

pub fn spawn(app: AppHandle) {
    let builder = std::thread::Builder::new().name("anvil-sync-startup".into());
    if let Err(e) = builder.spawn(move || {
        std::thread::sleep(STARTUP_DELAY);
        let state = app.state::<AppState>();
        let conn = state.lock();
        let outcome = relay::run(&conn, &state.device_id, &crate::time::SystemNow);
        if outcome.skipped {
            log::info!("开机同步跳过：坚果云凭据还没配");
        } else if outcome.ok {
            log::info!(
                "开机同步完成：并更 {} 行，{}（待上传 {}）",
                outcome.applied,
                if outcome.uploaded { "已上传本机那份" } else { "没有本地写入，跳过上传" },
                outcome.pending_after
            );
        } else {
            log::warn!("开机同步没成：{}", outcome.error.unwrap_or_default());
        }
    }) {
        log::warn!("开机同步线程没起来：{e}");
    }
}
