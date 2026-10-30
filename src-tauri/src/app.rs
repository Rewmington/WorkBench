//! 组合根：状态、插件、托盘、窗口事件、命令注册。

use std::sync::atomic::{AtomicBool, AtomicU32};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use tauri::{Manager, Window, WindowEvent};

use crate::commands;
use crate::db;
use crate::error::Result;
use crate::state::{AppState, Paths};
use crate::sync::device;
use crate::ui::{menus, tray, windows};

#[cfg(debug_assertions)]
fn seed_flag() -> Option<i32> {
    // 走环境变量而不是 argv：`tauri dev` 会把 argv 插到 `cargo run` 自己那串参数前面，
    // 传参顺序在 CLI 各版本间不稳，env 没有这个问题。
    let from_env = std::env::var("ANVIL_SEED").ok();
    let from_arg = std::env::args()
        .skip_while(|a| a != "--seed")
        .nth(1)
        .filter(|v| !v.starts_with('-'));
    let raw = from_env.or(from_arg)?;
    raw.parse::<i32>().ok().filter(|v| (7..=720).contains(v))
}

fn bootstrap() -> Result<(Connection, Paths, String)> {
    let paths = Paths::local_app_data()?;
    paths.ensure()?;
    let mut conn = db::open(&paths.db)?;
    let applied = db::migrations::migrate(&mut conn)?;
    let device_id = device::load_or_create(&paths.device_id)?;

    // 只给 debug 构建用的造数据入口（M5 的满数据截图、M7 的逐图核对都靠它）
    #[cfg(debug_assertions)]
    if let Some(days_back) = seed_flag() {
        let n = crate::seed::populate(&mut conn, &crate::time::SystemNow, &device_id, days_back)?;
        log::info!("种子数据：写入 {n} 条（{days_back} 天）");
    }

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
        backup_failures: AtomicU32::new(0),
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
        .plugin(tauri_plugin_global_shortcut::Builder::new().with_handler(crate::hotkeys::on_event).build())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::system::paths,
            commands::system::device_id,
            commands::system::context_menu,
            commands::system::open_data_dir,
            commands::system::open_url,
            commands::settings::settings_all,
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::todos::todos_list,
            commands::todos::todos_all,
            commands::todos::todos_list_undone,
            commands::todos::todos_get,
            commands::todos::todos_children,
            commands::todos::todos_create,
            commands::todos::todos_update,
            commands::todos::todos_set_done,
            commands::todos::todos_delete,
            commands::todos::todos_restore,
            commands::todos::todos_reorder,
            commands::todos::todos_set_tags,
            commands::launcher::resource_add_from_path,
            commands::launcher::resource_pick,
            commands::launcher::resource_add_web,
            commands::launcher::resources_list,
            commands::launcher::resource_launch,
            commands::launcher::resource_rename,
            commands::launcher::resource_move,
            commands::launcher::resource_delete,
            commands::launcher::resource_set_secret,
            commands::launcher::resource_reveal_secret,
            commands::launcher::zones_list,
            commands::launcher::zone_add,
            commands::launcher::zone_delete,
            commands::palette::palette_search,
            commands::dashboard::dash_summary,
            commands::dashboard::dash_suggestions,
            commands::stats::stats_report,
            commands::widgets::stickies_list,
            commands::widgets::sticky_save,
            commands::widgets::sticky_clear,
            commands::widgets::countdowns_list,
            commands::widgets::countdown_add,
            commands::widgets::countdown_start,
            commands::widgets::countdown_pause,
            commands::widgets::countdown_resume,
            commands::widgets::countdown_reset,
            commands::widgets::countdown_delete,
            commands::widgets::sys_load,
            commands::notes::notes_list,
            commands::notes::notes_trashed,
            commands::notes::notes_get,
            commands::notes::notes_create,
            commands::notes::notes_update,
            commands::notes::notes_set_tags,
            commands::notes::notes_trash,
            commands::notes::notes_restore,
            commands::notes::notes_purge,
            commands::notes::folders_list,
            commands::notes::folder_add,
            commands::notes::folder_rename,
            commands::notes::folder_move,
            commands::notes::folder_delete,
            commands::hotkeys::hotkeys_list,
            commands::hotkeys::hotkeys_set,
            commands::hotkeys::hotkeys_reset,
            commands::hotkeys::autostart_get,
            commands::hotkeys::autostart_set,
            commands::linked::linked_state,
            commands::linked::linked_refresh_blog,
            commands::linked::linked_refresh_medicine,
            commands::linked::linked_check_online,
            commands::linked::linked_restock_todo,
            commands::linked::linked_set_blog_dir,
            commands::linked::linked_set_medicine_db,
            commands::linked::linked_set_medicine_password,
            commands::linked::linked_clear_medicine_password,
            commands::linked::linked_set_medicine_dir,
            commands::timetable::timetable_state,
            commands::timetable::timetable_set_week1,
            commands::timetable::timetable_week,
            commands::timetable::timetable_import,
            commands::backup::backup_status,
            commands::backup::backup_now,
            commands::backup::backup_save_credentials,
            commands::backup::backup_clear_credentials,
            commands::backup::backup_list_remote,
            commands::backup::backup_test_connection,
            commands::backup::backup_restore,
            commands::backup::backup_self_test,
        ])
        // 托盘与右键菜单的点击都汇到这一个分发口。
        .on_menu_event(|app, event| menus::handle(app, event.id().as_ref()))
        .setup(|app| {
            tray::build(app)?;
            {
                let handle = app.handle().clone();
                let state = app.state::<AppState>();
                let conn = state.lock();
                crate::hotkeys::register_at_startup(&handle, &conn);
            }
            crate::backup::schedule::spawn(app.handle().clone());
            crate::notify::register(app.handle());
            crate::notify::spawn(app.handle().clone());
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
