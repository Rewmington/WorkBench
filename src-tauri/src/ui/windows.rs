//! 窗口生命周期与显示状态。

use tauri::{AppHandle, CloseRequestApi, Emitter, Manager, Window};

use crate::state::AppState;

pub const MAIN: &str = "main";

const HINT: &str = "已收进托盘，仍在后台运行。托盘菜单里选「退出」才会真的关掉。";

pub fn show_and_focus(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
        deliver_pending_hint(app);
    }
}

/// 那句话要在窗口**又出现**时才说。原先是 hide 之后立刻 emit：藏起来的页面里 toast 的
/// 六秒计时器照跑，等用户把窗口唤回来，那行字早自己走完了 —— 这条提示等于从没生效过。
fn deliver_pending_hint(app: &AppHandle) {
    let state = app.state::<AppState>();
    if spend_hint(&state.tray_hint) {
        let _ = app.emit(crate::ui::menus::HINT_EVENT, HINT);
    }
}

/// 1（欠着一句）→ 2（说过了）并且返回 true；其余状态返回 false。
/// 用 `compare_exchange` 而不是先读后写：快捷键和单实例聚焦可能在同一瞬间各唤一次窗，
/// 两次都读到 1 就会把"只打扰一次"变成弹两遍。
fn spend_hint(hint: &std::sync::atomic::AtomicU8) -> bool {
    hint.compare_exchange(
        1,
        2,
        std::sync::atomic::Ordering::Relaxed,
        std::sync::atomic::Ordering::Relaxed,
    )
    .is_ok()
}

pub fn hide(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN) {
        let _ = w.hide();
    }
}

pub fn toggle(app: &AppHandle) {
    match app.get_webview_window(MAIN) {
        Some(w) if w.is_visible().unwrap_or(false) => hide(app),
        Some(_) => show_and_focus(app),
        None => {}
    }
}

/// 关窗口 = 收进托盘。第一次这么做时挂一句提示（不弹窗，§2 原则 5），只挂不发音。
pub fn on_close_requested(window: &Window, api: &CloseRequestApi) {
    api.prevent_close();

    let state = window.app_handle().state::<AppState>();
    owe_hint(&state.tray_hint);
    window.hide().ok();
}

/// 0（从没收过）→ 1（欠一句）并返回 true；1 和 2 都不动。
fn owe_hint(hint: &std::sync::atomic::AtomicU8) -> bool {
    hint.compare_exchange(
        0,
        1,
        std::sync::atomic::Ordering::Relaxed,
        std::sync::atomic::Ordering::Relaxed,
    )
    .is_ok()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU8;

    use super::*;

    /// 状态机是这一层唯一的行为，抽成两个纯函数就能脱离真窗口钉住它。
    /// 真窗口那边还剩一条边只能在那儿看：toast 在隐藏态到底跑不跑计时器。
    #[test]
    fn the_hint_is_owed_once_spent_once_and_never_again() {
        let hint = AtomicU8::new(0);
        assert!(!spend_hint(&hint), "还没收进托盘，唤窗时不该说这句");

        // 收进托盘：只有第一次欠下一句，之后每次关窗都不该再欠。
        assert!(owe_hint(&hint));
        assert!(!owe_hint(&hint), "已经欠着或说过了，就别再欠");
        assert_eq!(hint.load(std::sync::atomic::Ordering::Relaxed), 1);

        assert!(spend_hint(&hint), "第一次把窗口唤回来时把那句说掉");
        assert!(!spend_hint(&hint), "说过一次之后不再打扰");
        assert_eq!(hint.load(std::sync::atomic::Ordering::Relaxed), 2);
        assert!(!owe_hint(&hint), "说过了就不再挂新的");
    }
}
