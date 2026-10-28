//! 组合根：状态、插件、托盘、窗口事件、命令注册。

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tauri::{Window, WindowEvent};

use crate::commands;
use crate::db;
use crate::error::Result;
use crate::state::{AppState, Paths};
use crate::sync::device;
use crate::ui::{menus, tray, windows};

fn bootstrap() -> Result<(Connection, Paths, String)> {
    let paths = Paths::local_app_data()?;
    paths.ensure()?;
    let mut conn = db::open(&paths.db)?;
    let applied = db::migrations::migrate(&mut conn)?;
    let device_id = device::load_or_create(&paths.device_id)?;

    // 「数据位置可感知」从日志就开始：一眼能看出库没落到 Roaming。
    log::info!(
        "数据根目录 {}（迁移 {} 条，journal_mode={}，设备 {}）",
        paths.root.display(),
        applied,
        db::journal_mode(&conn),
        device_id
    );

    Ok((conn, paths, device_id))
}

pub fn run() {
    let (conn, paths, device_id) = match bootstrap() {
        Ok(v) => v,
        Err(e) => {
            // 起不来就没有 UI 可显示，只能走 stderr + 非零退出码。
            eprintln!("Anvil 启动失败：{e}");
            std::process::exit(2);
        }
    };

    let state = AppState {
        db: Mutex::new(conn),
        device_id: Arc::from(device_id.as_str()),
        tray_hint_shown: AtomicBool::new(false),
        paths,
    };

    tauri::Builder::default()
        // 第一个插件，无条件装：已开则聚焦而不是新窗（§8.6）。
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::show_and_focus(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::system::paths,
            commands::system::device_id,
            commands::system::context_menu,
            commands::settings::settings_all,
            commands::settings::settings_get,
            commands::settings::settings_set,
        ])
        // 托盘与右键菜单的点击都汇到这一个分发口。
        .on_menu_event(|app, event| menus::handle(app, event.id().as_ref()))
        .setup(|app| {
            tray::build(app)?;
            Ok(())
        })
        .on_window_event(|window: &Window, event: &WindowEvent| match event {
            // 关窗口 = 收进托盘；真退出只在托盘菜单里。
            WindowEvent::CloseRequested { api, .. } => windows::on_close_requested(window, api),
            // 环的巡航要在失焦时真停（§8.8.1）；焦点状态由前端订阅窗口事件，这里只留线索。
            WindowEvent::Focused(focused) => {
                log::debug!("窗口焦点 = {focused}");
            }
            _ => {}
        })
        .build(tauri::generate_context!())
        .expect("构建 Anvil 失败")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                log::info!("Anvil 退出");
            }
        });
}
