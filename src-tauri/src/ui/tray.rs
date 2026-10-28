//! 托盘驻留。关掉窗口不等于退出：真退出只有托盘菜单里那条「退出」。

use tauri::menu::{MenuBuilder, MenuItemBuilder};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::ui::{menus, windows};

pub const TRAY_ID: &str = "tray-main";

pub fn build(app: &mut tauri::App) -> tauri::Result<()> {
    let toggle = MenuItemBuilder::with_id(menus::ID_TOGGLE, "显示 / 隐藏 Anvil").build(app)?;
    let settings = MenuItemBuilder::with_id(menus::ID_SETTINGS, "设置").build(app)?;
    let sep = tauri::menu::PredefinedMenuItem::separator(app)?;
    let quit = MenuItemBuilder::with_id(menus::ID_QUIT, "退出").build(app)?;

    let menu = MenuBuilder::new(app)
        .items(&[&toggle, &settings, &sep, &quit])
        .build()?;

    let _tray = TrayIconBuilder::with_id(TRAY_ID)
        .title("Anvil · 工作台")
        .menu(&menu)
        // 左键自己处理成「显示/隐藏」，右键才出菜单。
        .show_menu_on_left_click(false)
        .icon(tauri::include_image!("icons/tray.png"))
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                windows::toggle(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}
