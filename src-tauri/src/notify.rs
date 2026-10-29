//! 待办提醒 → 系统通知中心。
//!
//! 不做成页面里的 toast（§8.6 第 12 条）：窗口收进托盘时页面根本没显示，
//! 提醒也就不会响 —— 而那恰恰是提醒最需要响的场合。
//!
//! 回跳靠 `action_type_id` 把 todo id 原样带回来：Windows 上插件会把整份
//! 通知数据回传给 `on_action`，所以不需要拿标题去反查是哪一条 ——
//! 标题会重复，id 不会。

use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::{ActionPerformed, NotificationExt};

use crate::error::Result;
use crate::repos::todos;
use crate::state::AppState;
use crate::time::SystemNow;
use crate::ui::windows;

const TICK: Duration = Duration::from_secs(30);

/// 一次最多弹这么多条：一整天没开机时积压的提醒全糊上去等于刷屏，剩下的下一轮再说。
const BATCH: i64 = 5;

/// 点击通知要回跳到某条待办。payload 是 todo id。
pub const REVEAL: &str = "app://reveal";

pub fn spawn(app: AppHandle) {
    let builder = std::thread::Builder::new().name("anvil-remind".into());
    if let Err(e) = builder.spawn(move || loop_(app)) {
        log::warn!("提醒调度线程没起来：{e}");
    }
}

fn loop_(app: AppHandle) {
    // 起来先等一轮：启动那几秒要给首屏、联动、备份让路
    loop {
        std::thread::sleep(TICK);
        if let Err(e) = tick(&app) {
            log::warn!("提醒这一轮没跑成：{e}");
        }
    }
}

fn tick(app: &AppHandle) -> Result<usize> {
    let state = app.state::<AppState>();
    let now = SystemNow;
    let device = state.device_id.to_string();

    let pending = {
        let conn = state.lock();
        todos::take_due_reminders(&conn, &now, &device, BATCH)?
    };
    // 锁放掉之后再发：通知要等系统返回，握着写库锁等它等于把整个界面卡住。
    for item in &pending {
        notify(app, &item.title, &item.id);
    }
    Ok(pending.len())
}

fn notify(app: &AppHandle, title: &str, todo_id: &str) {
    let r = app
        .notification()
        .builder()
        .title(title)
        .body("设定的提醒时间到了")
        .action_type_id(todo_id)
        .show();
    if let Err(e) = r {
        // 发不出去不重试：标记已经盖了，重试的代价是同一件事响两次
        log::warn!("通知没发出去：{e}");
    }
}

/// 用户在通知中心点了某条提醒：把窗口带回前台，并让界面跳到那条待办。
///
/// 读不到 id（通知在 Action Center 里被系统改写、或压根没注册回调）时仍然唤窗口 ——
/// 「点了提醒结果什么都没发生」是最不能接受的那种失败。
pub fn on_action(app: &AppHandle, performed: &ActionPerformed) {
    windows::show_and_focus(app);
    if let Some(id) = performed
        .notification()
        .and_then(|n| n.action_type_id())
        .map(str::to_owned)
    {
        let _ = app.emit(REVEAL, id);
    }
}

/// 在 setup 里挂上回调。**必须挂**：插件在没有监听者时压根不追踪点击。
pub fn register(app: &AppHandle) {
    let handle = app.clone();
    if let Err(e) = app
        .notification()
        .on_action(move |performed| on_action(&handle, performed))
    {
        log::warn!("通知点击回调没挂上：{e}");
    }
}
