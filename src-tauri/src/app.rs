//! 组合根：状态、插件、托盘、窗口事件、命令注册。

use std::sync::atomic::{AtomicU32, AtomicU8};
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
    crate::logger::use_dir(&paths.logs);
    // 新建与否必须在看开库**之前**问一次：`db::open` 会顺手把缺失的文件建出来，
    // 之后就再也分不出「这是用户的库」和「这是还原中断后新建的空库」。
    let opened_fresh = !paths.db.exists();
    let mut conn = db::open(&paths.db)?;
    let applied = db::migrations::migrate(&mut conn)?;
    let device_id = device::load_or_create(&paths.device_id)?;

    // 把「本机自己写过的最晚时刻」垫成时间戳下限，让单调性跨重启也成立（只读查询）。
    // 只算 `updated_by = 本机`：对端设备若时钟超前，它写来的行会把我们的下限顶到未来，
    // 之后本机每一条写入都会白赢过那台机器 —— 那是把别人的错变成自己的优势，比回拨更难查。
    let seen: i64 = conn.query_row(
        "SELECT COALESCE(MAX(updated_at), 0) FROM (
            SELECT updated_at FROM todos WHERE updated_by = ?1
            UNION ALL
            SELECT updated_at FROM classes WHERE updated_by = ?1
         )",
        rusqlite::params![device_id],
        |r| r.get(0),
    )?;
    crate::time::raise_floor(seen);

    if opened_fresh {
        crate::backup::notice_interrupted_restore(&conn, &crate::time::SystemNow, &paths.root);
    }

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
    // 装在最前：bootstrap 里任何一句 log 与任何一次 panic 都要有去处。
    crate::logger::install();
    let (conn, paths, device_id) = match bootstrap() {
        Ok(v) => v,
        Err(e) => {
            // 起不来就没有 UI：eprintln 在 release 里无处可去，所以同一句话必须也落进文件。
            crate::logger::startup_failure(&e.to_string());
            std::process::exit(2);
        }
    };

    let state = AppState {
        db: Mutex::new(conn),
        // 先放默认值：setup 里 `register_at_startup` 成功注册后才换成库里那条。
        // 没注册成功时系统根本不会回调，这份默认值不会被读到。
        hotkey: Mutex::new(crate::hotkeys::DEFAULT_SHOW.to_owned()),
        device_id: Arc::from(device_id.as_str()),
        tray_hint: AtomicU8::new(0),
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
        // 钉子：这个窗口只渲染自己的界面，往站外的导航一律取消（§8.6）。
        // 笔记预览那处点击已经 `preventDefault`，这一层管的是**以后漏掉的那些**，
        // 以及页面自己写 `location.href=` 的情况 —— 被带走的不只是一个页面，是整个界面。
        .plugin(navigation_guard())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            commands::system::context_menu,
            commands::system::open_data_dir,
            commands::system::open_url,
            commands::settings::settings_all,
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::todos::todos_list,
            commands::todos::todos_all,
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
            commands::backup::backup_clear_log,
            commands::backup::backup_list_remote,
            commands::backup::backup_test_connection,
            commands::backup::backup_restore,
            commands::backup::backup_self_test,
            commands::sync::sync_status,
            commands::sync::sync_now,
            commands::sync::sync_clean_peers,
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
            // 开机同步一次（PLAN-MOBILE §3.2 的「打开 app 时」）；上限一次、不循环重试，
            // 理由写在 `sync::schedule` 头上。手动那一半是设置页的「立即同步」。
            crate::sync::schedule::spawn(app.handle().clone());
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

/// 只放行应用自己的界面地址。dev 是 `http://localhost:1420`，装出来的版本在 Windows 上
/// 是 `http://tauri.localhost`（`tauri://` 那条留给别的平台）。
///
/// 判据写成纯函数，是因为这条一旦搞错就是「打开一个空窗口」级别的事故，得能钉住。
fn is_app_url(url: &tauri::Url) -> bool {
    match (url.scheme(), url.host_str()) {
        ("tauri", _) => true,
        ("http" | "https", Some("localhost")) | ("http" | "https", Some("127.0.0.1")) => true,
        ("http" | "https", Some("tauri.localhost")) => true,
        // WebView2 起手会过一次 about:blank
        ("about", None) => true,
        _ => false,
    }
}

/// 全局导航闸门。用插件挂，是因为 `tauri::Builder` 上没有 `on_navigation`，
/// 而插件的这条回调对**每个窗口**都生效 —— 界面被站外地址带走这件事，不该靠每个
/// `<a>` 的点击处理各自记得。
fn navigation_guard<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("navigation-guard")
        .on_navigation(|_webview, url| {
            if is_app_url(url) {
                return true;
            }
            log::warn!("拒绝在应用窗口里打开：{url}");
            false
        })
        .build()
}

#[cfg(test)]
mod tests {
    use super::is_app_url;
    use tauri::Url;

    fn u(s: &str) -> Url {
        Url::parse(s).expect("测试里的地址必须能解析")
    }

    #[test]
    fn the_apps_own_addresses_get_through() {
        for s in [
            "http://localhost:1420/",
            "http://127.0.0.1:1420/#/notes",
            "http://tauri.localhost/index.html",
            "tauri://localhost",
            "about:blank",
        ] {
            assert!(is_app_url(&u(s)), "{s} 是界面自己的地址，拦了就剩一个空窗口");
        }
    }

    #[test]
    fn everything_else_is_refused_a_site_cannot_hijack_the_window() {
        for s in [
            "https://example.net/",
            "https://example.net/x?a=1",
            "http://localhost.evil.com/",
            "file:///C:/Windows/win.ini",
            "mailto:someone@example.com",
        ] {
            assert!(!is_app_url(&u(s)), "{s} 该交给系统浏览器，不是这个窗口");
        }
    }
}
