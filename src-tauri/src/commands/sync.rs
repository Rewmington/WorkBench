//! 手机同步的命令面（M11b）。
//!
//! 两条命令、两个分工：
//! - `sync_status` **不碰网络**：读 settings 里那份 sync.state + `merge::pending` 的计数。
//!   界面每次进这一页都能立刻画出来，与 `linked_state` 同一条纪律。
//! - `sync_now` 是手动按钮那一下。网络类命令照 `db/mod.rs` 顶上的纪律走
//!   `spawn_blocking`，锁只在阻塞任务自己的作用域里取（与 `backup_now` 同款结构：
//!   同步的形状本身就要求 IO 与库读写交替，见 `sync::relay::run_with`）。
//!
//! 计数与状态一律走 `sync::merge::pending` 与 `sync::relay::stored_state`，
//! 这里不写第二条计数 SQL（PLAN-MOBILE §6 M11b 点名的纪律）。

use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::backup;
use crate::error::Error;
use crate::state::AppState;
use crate::sync::{merge, relay};
use crate::time::SystemNow;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    /// 与备份同一组凭据：邮箱在 settings、应用密码在凭据管理器，两个都齐才算配好。
    pub configured: bool,
    /// 本机设备 id —— 界面上「这台机器」那一行，也解释远端 listing 里谁是自己。
    pub my_device: String,
    /// 最后一次**成功**同步的时刻；「上次同步于 12:03」的那半句。
    pub last_sync_at: Option<i64>,
    /// 最近一次失败的一句话。界面只在它比 `last_sync_at` 新时才显示（备份页同款判据）。
    pub last_error: Option<String>,
    pub last_error_at: Option<i64>,
    /// 「N 条待上传」的唯一计数来源：`merge::pending`。
    pub pending: i64,
    /// 远端设备 listing（最后一次成功列目录看到的别人的设备 id，不含本机）。
    pub peers: Vec<String>,
}

fn view(conn: &Connection, device: &str) -> Result<SyncStatus, Error> {
    let st = relay::stored_state(conn)?;
    let peers = st
        .get("peers")
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    Ok(SyncStatus {
        configured: backup::credentials(conn).is_some(),
        my_device: device.to_owned(),
        last_sync_at: st.get("lastSyncAt").and_then(serde_json::Value::as_i64),
        last_error: st
            .get("lastError")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned),
        last_error_at: st.get("lastErrorAt").and_then(serde_json::Value::as_i64),
        pending: merge::pending(conn)?,
        peers,
    })
}

#[tauri::command]
pub fn sync_status(state: State<'_, AppState>) -> Result<SyncStatus, Error> {
    let conn = state.lock();
    view(&conn, &state.device_id)
}

/// 手动「立即同步」。失败不抛错、不弹窗：结果折进返回的状态里，界面只多一行状态
/// （PLAN-MOBILE §5.5 / §6 M11b）。凭据没配是唯一抛错的路径 —— 那句必须是可执行的话，
/// 而按钮在未配置时本就禁用，走到这里说明状态页过期了，重讲一遍那句话。
#[tauri::command]
pub async fn sync_now(app: AppHandle) -> Result<SyncStatus, Error> {
    let status = tauri::async_runtime::spawn_blocking(move || -> Result<SyncStatus, Error> {
        let state = app.state::<AppState>();
        let conn = state.lock();
        let outcome = relay::run(&conn, &state.device_id, &SystemNow);
        if outcome.skipped {
            return Err(Error::msg(
                "先在备份页填好坚果云邮箱与应用密码（手机同步与备份共用这一组凭据）",
            ));
        }
        // relay::run 已把结果记进 sync.state；重读一遍返回，界面与开机路径看到同一份。
        view(&conn, &state.device_id)
    })
    .await
    .map_err(|e| Error::msg(format!("同步任务没跑起来：{e}")))??;
    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::repos::settings;
    use crate::time::FixedNow;
    use serde_json::json;

    fn conn() -> Connection {
        let mut c = db::open_in_memory().unwrap();
        db::migrations::migrate(&mut c).unwrap();
        c
    }

    /// 计数只能来自 `merge::pending`：写脏两行、清一行，视图就得跟着说 1。
    /// 这一条同时钉住「不在命令层另写 SQL」—— view() 与 relay 读同一份状态。
    #[test]
    fn the_pending_number_always_comes_from_merge() {
        use crate::model::TodoDraft;
        use crate::repos::todos;

        let c = conn();
        let now = FixedNow(1_700_000_000_000);
        let a = todos::create(&c, &now, "pc-a", TodoDraft { title: "一".into(), ..Default::default() }).unwrap();
        todos::create(&c, &now, "pc-a", TodoDraft { title: "二".into(), ..Default::default() }).unwrap();

        let st = view(&c, "pc-a").unwrap();
        assert_eq!((st.pending, st.last_sync_at, st.my_device.as_str()), (2, None, "pc-a"));

        // 模拟「这一行已经上过云」：清脏走 merge 的唯一入口
        merge::clear_dirty(&c, 1_700_000_000_000).unwrap();
        todos::set_done(&c, &now, "pc-a", &a.stamp.id, true).unwrap();
        assert_eq!(view(&c, "pc-a").unwrap().pending, 1);
    }

    /// sync.state 里的 listing 会原样走到界面（设备 id，不是文件名，更不是邮箱）。
    #[test]
    fn stored_listing_survives_into_the_view() {
        let c = conn();
        settings::set_json(
            &c,
            settings::KEY_SYNC_STATE,
            &json!({"lastSyncAt": 123, "lastError": null, "lastErrorAt": null, "peers": ["phone-a", "pc-b"]}),
            123,
            "pc-a",
        )
        .unwrap();
        let st = view(&c, "pc-a").unwrap();
        assert_eq!(st.last_sync_at, Some(123));
        assert_eq!(st.peers, vec!["phone-a".to_owned(), "pc-b".to_owned()]);
        assert!(st.last_error.is_none());
    }
}
