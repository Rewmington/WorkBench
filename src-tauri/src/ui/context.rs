//! 原生右键菜单。网页应用最露馅的地方之一就是右键弹出浏览器菜单（§8.6），
//! 这里一律走 Win32 原生菜单：前端拦掉默认 contextmenu，只告诉后端「点的是哪一类东西」。
//!
//! 菜单项的**响应**不在这里 —— Tauri 2 的菜单事件走全局 `on_menu_event`，
//! 分发集中在 `ui::menus::handle`，托盘与右键共用同一套 id。

use tauri::menu::{ContextMenu, MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{Manager, Window};

use crate::error::Result;

/// 只说明点的是哪一类东西，具体条目内容由调用点决定；输入区多给剪贴板类预定义项。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKind {
    Shell,
    Input,
}

impl MenuKind {
    pub fn parse(raw: &str) -> Self {
        match raw {
            "input" => Self::Input,
            _ => Self::Shell,
        }
    }
}

/// `window` 由 Tauri 从调用方注入，所以这里不需要 `Manager::get_window`（那个还在 unstable feature 后面）。
pub fn popup(window: Window, kind: MenuKind) -> Result<()> {
    let app = window.app_handle();

    let mut builder = MenuBuilder::new(app);

    builder = match kind {
        MenuKind::Input => builder
            .item(&PredefinedMenuItem::cut(app, Some("剪切"))?)
            .item(&PredefinedMenuItem::copy(app, Some("复制"))?)
            .item(&PredefinedMenuItem::paste(app, Some("粘贴"))?)
            .item(&PredefinedMenuItem::select_all(app, Some("全选"))?),
        MenuKind::Shell => {
            let copy = MenuItemBuilder::with_id(super::menus::ID_COPY, "复制选中内容").build(app)?;
            builder.item(&copy)
        }
    };

    let open_dir = MenuItemBuilder::with_id(super::menus::ID_OPEN_DIR, "打开数据文件夹")
        .build(app)?;
    let reload = MenuItemBuilder::with_id(super::menus::ID_RELOAD, "重新加载").build(app)?;
    let settings = MenuItemBuilder::with_id(super::menus::ID_SETTINGS, "设置").build(app)?;

    let window_submenu = SubmenuBuilder::new(app, "窗口")
        .item(&PredefinedMenuItem::minimize(app, Some("最小化"))?)
        .item(&PredefinedMenuItem::maximize(app, Some("最大化"))?)
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&reload)
        .build()?;

    let menu = builder
        .item(&PredefinedMenuItem::separator(app)?)
        .item(&open_dir)
        .item(&settings)
        .item(&window_submenu)
        .build()?;

    // Windows 上 muda 按当前光标位置弹出，不需要自己换算坐标。
    menu.popup(window.clone())?;
    Ok(())
}
