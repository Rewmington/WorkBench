//! 全局快捷键：注册、改绑、冲突回滚。
//!
//! 判据只有一条：**注册失败就是被占用**。Windows 没有"查询某组合键归谁"的 API，
//! 所以不猜、不解释，把注册失败原样变成一句人话，并把原来的绑定还回去 ——
//! 改绑失败后留着新值不管，等于让用户下次按键才发现快捷键坏了。

use rusqlite::Connection;
use serde_json::{json, Value};
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

use crate::error::{Error, Result};
use crate::repos::settings;
use crate::state::AppState;
use crate::time::{Now, SystemNow};
use crate::ui::windows;

/// v1 只有一个可绑的动作：把窗口唤到前台。
pub const ACTION_SHOW: &str = "show";
pub const DEFAULT_SHOW: &str = "Alt+Shift+A";

fn parse(spec: &str) -> Result<Shortcut> {
    spec.trim()
        .parse::<Shortcut>()
        .map_err(|e| Error::msg(format!("组合键格式不认识：{e}")))
}

/// 从 settings 读出当前绑定。读不到或形状不对都退回默认值 ——
/// 一个坏 JSON 不该让应用起不来。
pub fn combo_for(conn: &Connection, action: &str) -> String {
    settings::get_json(conn, settings::KEY_HOTKEYS)
        .ok()
        .flatten()
        .and_then(|v| v.get(action).and_then(Value::as_str).map(|s| s.to_owned()))
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SHOW.to_owned())
}

fn write_combo(conn: &Connection, action: &str, combo: &str) -> Result<()> {
    let mut map = settings::get_json(conn, settings::KEY_HOTKEYS)
        .ok()
        .flatten()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    map.insert(action.to_owned(), json!(combo));
    settings::set_json(
        conn,
        settings::KEY_HOTKEYS,
        &Value::Object(map),
        SystemNow.ms(),
        "local",
    )
}

/// 把「当前注册生效的组合键」记进内存。注册成功的两个出口都要写：启动那一次与换绑那一次。
/// 回调那边只读这份，不读库 —— 原因写在 `AppState::hotkey` 的注释上。
fn remember(app: &AppHandle, combo: &str) {
    let state = app.state::<AppState>();
    let mut held = state
        .hotkey
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *held = combo.to_owned();
}

/// 启动时注册。失败只记日志：别的程序占了这个键，不该阻止应用启动，
/// 设置页会显示当前绑定并允许改。
pub fn register_at_startup(app: &AppHandle, conn: &Connection) {
    let combo = combo_for(conn, ACTION_SHOW);
    if let Ok(shortcut) = parse(&combo) {
        if let Err(e) = app.global_shortcut().register(shortcut) {
            log::warn!("全局快捷键 {combo} 注册失败（多半已被别的程序占用）：{e}");
        } else {
            remember(app, &combo);
        }
    }
}

/// 换绑。成功返回最终生效的组合键；失败时恢复旧绑定并回一个可判别的错误。
pub fn rebind(app: &AppHandle, conn: &Connection, action: &str, combo: &str) -> Result<String> {
    let combo = combo.trim().to_owned();
    let shortcut = parse(&combo)?;
    let previous = combo_for(conn, action);

    app.global_shortcut()
        .unregister_all()
        .map_err(|e| Error::msg(format!("解绑旧快捷键失败：{e}")))?;

    if let Err(e) = app.global_shortcut().register(shortcut) {
        log::warn!("全局快捷键 {combo} 注册失败：{e}");
        // 回滚。留着没人认领的新值，用户要到下次按键才发现快捷键坏了。
        if let Ok(old) = parse(&previous) {
            let _ = app.global_shortcut().register(old);
        }
        return Err(Error::msg(format!("「{combo}」已被其他程序占用")));
    }

    write_combo(conn, action, &combo)?;
    // 注册已经生效，内存里那份要跟着走 —— 回调比的是这个值，晚一步就会把刚按下的键判成"别人的"。
    remember(app, &combo);
    Ok(combo)
}

/// 按下的是不是这条绑定。比对解析后的 `Shortcut` 而不是字符串：
/// `HotKey::to_string()` 写的是 `shift+alt+KeyA`，而设置里存的是 `Alt+Shift+A`，
/// 拿字符串比永远不相等 —— 这个 bug 只有在真机上按下快捷键才会暴露，浏览器里演不出来。
pub fn matches(registered: &str, pressed: &Shortcut) -> bool {
    parse(registered).map(|want| want == *pressed).unwrap_or(false)
}

/// 插件回调。只认按下（抬起再触发一次等于按一下弹两次），且只认当前生效那条绑定
/// —— 否则将来加第二个动作时会互相踩。
///
/// 这里**一次数据库都不碰**：回调跑在主线程，而 `db` 那把锁会在同步期间被网络 IO 握着不放。
pub fn on_event(app: &AppHandle, shortcut: &Shortcut, event: ShortcutEvent) {
    if event.state != ShortcutState::Pressed {
        return;
    }
    let registered = {
        let state = app.state::<AppState>();
        let held = state
            .hotkey
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        held.clone()
    };
    if matches(&registered, shortcut) {
        windows::show_and_focus(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 注释里那个"只有真机能暴露"的格式差，现在由这条测试盯着。
    #[test]
    fn the_same_combo_matches_whatever_shape_it_is_written_in() {
        for written in ["Alt+Shift+A", "shift+alt+KeyA", "SHIFT+ALT+A", " alt+shift+a "] {
            let pressed = parse("Alt+Shift+A").expect("默认绑定必须解得开");
            assert!(matches(written, &pressed), "{written} 就是同一个组合键");
        }
    }

    #[test]
    fn a_different_combo_does_not_match() {
        let pressed = parse("Alt+Shift+A").expect("默认绑定必须解得开");
        assert!(!matches("Ctrl+Shift+A", &pressed), "换绑之后旧键不该还能唤窗口");
        // 设置里那条读不出来时不该有动作，而不是"随便按什么都唤窗"。
        assert!(!matches("", &pressed));
        assert!(!matches("不是组合键", &pressed));
    }
}
