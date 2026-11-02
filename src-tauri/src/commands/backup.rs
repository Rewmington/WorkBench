use rusqlite::Connection;
use serde_json::json;
use tauri::{AppHandle, Manager, State};

use crate::backup::{self, crypto, schedule, webdav::Remote};
use crate::error::Error;
use crate::repos::settings;
use crate::state::AppState;
use crate::time::{Now, SystemNow};
use crate::win32::creds;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupStatus {
    pub configured: bool,
    /// 凭据管理器里有没有那条应用密码 —— 与邮箱分开设才是真话：
    /// 还原之后邮箱空着而密码还在，这时候界面该说"填个邮箱就行"，不是"什么都没配"。
    pub password_stored: bool,
    pub email: Option<String>,
    pub last_success_at: Option<i64>,
    pub last_error: Option<String>,
    pub last_error_at: Option<i64>,
    pub log: Vec<LogRow>,
    pub paths: crate::state::PathsView,
    pub journal_mode: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogRow {
    pub at: i64,
    pub ok: bool,
    pub bytes: Option<i64>,
    pub remote_path: Option<String>,
    pub error: Option<String>,
    pub kind: String,
}

fn read_status(conn: &Connection, state: &AppState) -> Result<BackupStatus, Error> {
    let mut stmt = conn.prepare(
        "SELECT at, ok, bytes, remote_path, error, kind FROM backup_log ORDER BY at DESC LIMIT 20",
    )?;
    let log: Vec<LogRow> = stmt
        .query_map([], |r| {
            Ok(LogRow {
                at: r.get(0)?,
                ok: r.get::<_, i64>(1)? != 0,
                bytes: r.get(2)?,
                remote_path: r.get(3)?,
                error: r.get(4)?,
                kind: r.get(5)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();

    let email = settings::get_json(conn, settings::KEY_WDAV_EMAIL)?
        .and_then(|v| v.as_str().map(str::to_owned));
    let journal_mode = crate::db::journal_mode(conn);

    Ok(BackupStatus {
        configured: email.is_some() && creds::webdav_password().is_ok(),
        password_stored: creds::webdav_configured(),
        email,
        last_success_at: schedule::last_success(conn),
        last_error: log.iter().find(|l| !l.ok).and_then(|l| l.error.clone()),
        last_error_at: log.iter().find(|l| !l.ok).map(|l| l.at),
        log,
        paths: state.paths.view(&journal_mode),
        journal_mode,
    })
}

#[tauri::command]
pub fn backup_status(state: State<'_, AppState>) -> Result<BackupStatus, Error> {
    let conn = state.lock();
    read_status(&conn, &state)
}

/// 手动备份。命令面板与设置页都指向这里，共用同一条实现。
/// 结果只落在 `backup_log`（侧栏与设置页都从那张表读），不再另存一份 settings 键。
#[tauri::command]
pub async fn backup_now(app: AppHandle) -> Result<backup::Outcome, Error> {
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let now = SystemNow;
        let conn = state.lock();
        backup::run(&conn, &state.paths.tmp, &now, "manual")
    })
    .await
    .map_err(|e| Error::msg(format!("备份任务没跑起来：{e}")))?;
    Ok(outcome)
}

/// 存凭据：邮箱进 settings，应用密码只进 Windows 凭据管理器。
#[tauri::command]
pub fn backup_save_credentials(
    state: State<'_, AppState>,
    email: String,
    password: String,
) -> Result<BackupStatus, Error> {
    let now = SystemNow;
    let email = email.trim().to_owned();
    if email.is_empty() {
        return Err(Error::msg("邮箱不能为空"));
    }
    // 框里空着 = 沿用凭据管理器里那条（换页回来密码框本来就是不回显的）
    if let Some(password) = creds::keep_or_set(&password, creds::webdav_configured())? {
        creds::set_webdav_password(&password)?;
    }

    let conn = state.lock();
    settings::set_json(&conn, settings::KEY_WDAV_EMAIL, &json!(email), now.ms(), &state.device_id)?;
    read_status(&conn, &state)
}

#[tauri::command]
pub fn backup_clear_credentials(state: State<'_, AppState>) -> Result<BackupStatus, Error> {
    creds::clear_webdav_password();
    let conn = state.lock();
    conn.execute("DELETE FROM settings WHERE key = ?1", rusqlite::params![settings::KEY_WDAV_EMAIL])?;
    read_status(&conn, &state)
}

/// 清空本机流水。远端与本地暂存副本都不动 —— 清的是这张列表。
#[tauri::command]
pub fn backup_clear_log(state: State<'_, AppState>) -> Result<BackupStatus, Error> {
    let conn = state.lock();
    backup::clear_log(&conn)?;
    read_status(&conn, &state)
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFile {
    pub name: String,
    pub size: u64,
    /// 头部能不能读出来 —— 读不出来通常意味着不是本机的快照
    pub readable: bool,
}

/// 非 async 的命令在主线程执行，而这里要发 PROPFIND —— 网络一慢就是整个窗口冻住。
/// 与 `backup_restore` 同款结构：`State<'_>` 不是 `'static`，所以在阻塞任务里重新取。
#[tauri::command]
pub async fn backup_list_remote(app: AppHandle) -> Result<Vec<RemoteFile>, Error> {
    let handle = app.clone();
    let files = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<RemoteFile>, Error> {
        let state = handle.state::<AppState>();
        let conn = state.lock();
        Ok(backup::list(&conn)?
            .into_iter()
            .map(|f| {
                let readable = f.name.starts_with("anvil-");
                RemoteFile {
                    name: f.name,
                    size: f.size,
                    readable,
                }
            })
            .collect())
    })
    .await
    .map_err(|e| Error::msg(format!("列远端快照没跑起来：{e}")))??;
    Ok(files)
}

/// 「测试连接」：只列目录，不写任何东西。用户填完凭据后最想要的一个按钮。
/// 两次网络往返（MKCOL + PROPFIND），同样不能摆在主线程上。
#[tauri::command]
pub async fn backup_test_connection(app: AppHandle) -> Result<usize, Error> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<usize, Error> {
        let state = handle.state::<AppState>();
        let email = backup::webdav_email(&state.lock())?;
        let password = creds::webdav_password()?;
        let remote = Remote::from_env(&email, &password)?;
        remote.ensure_dir()?;
        Ok(remote.list()?.len())
    })
    .await
    .map_err(|e| Error::msg(format!("测试连接没跑起来：{e}")))?
}

/// 换机还原的最后一步是重启应用：库文件被整个换掉了，
/// 继续用旧连接跑下去只会读到已删除的 inode。
#[tauri::command]
pub async fn backup_restore(app: AppHandle, name: String) -> Result<backup::Outcome, Error> {
    let handle = app.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let state = handle.state::<AppState>();
        let now = SystemNow;
        let mut conn = state.lock();
        backup::restore(&mut conn, &state.paths, &name, &now)
    })
    .await
    .map_err(|e| Error::msg(format!("还原任务没跑起来：{e}")))??;

    if outcome.ok {
        app.restart();
    }
    Ok(outcome)
}

/// 加解密往返自证：不需要网络就能证明「换机能解开」这条链路。
/// 不联网但**很费 CPU** —— 日备那一档 Argon2id 是 64 MiB × 3 轮，一次要派生两把，
/// 放在主线程上就是几秒的窗口冻结，所以照样走 `spawn_blocking`。
#[tauri::command]
pub async fn backup_self_test(email: String, password: String) -> Result<bool, Error> {
    tauri::async_runtime::spawn_blocking(move || -> Result<bool, Error> {
        let sample = b"anvil self test payload".to_vec();
        let sealed = crypto::seal(&format!("{email}:{password}"), &sample)?;
        let opened = crypto::open(&format!("{email}:{password}"), &sealed)?;
        Ok(opened == sample)
    })
    .await
    .map_err(|e| Error::msg(format!("自证没跑起来：{e}")))?
}
