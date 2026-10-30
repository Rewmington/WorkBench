use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::Error;
use crate::linking::{
    blog_local, blog_online, dav_readonly, medicine_cloud, medicine_copy, medicine_math, paths,
    Snapshot, Status,
};
use crate::model::Todo;
use crate::repos::{linked, settings, todos};
use crate::state::AppState;
use crate::time::{Now, SystemNow};

const LINK_BLOG: &str = "link://blog";
const LINK_MEDICINE: &str = "link://medicine";

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkedState {
    pub paths: paths::SourcePaths,
    pub blog: Vec<linked::SnapshotView>,
    pub medicine: Option<linked::SnapshotView>,
}

/// 首屏只读缓存，绝不在这条路径上碰磁盘外部文件或网络 ——
/// 外部源再慢再缺，界面第一帧都得画出来（PLAN §6.1「拿不到时」那一列）。
#[tauri::command]
pub fn linked_state(state: State<'_, AppState>) -> Result<LinkedState, Error> {
    let now = SystemNow;
    let conn = state.lock();
    let resolved = paths::resolve(&conn, &now, &state.device_id)?;
    Ok(LinkedState {
        paths: resolved,
        blog: linked::list_for(&conn, blog_local::SOURCE)?,
        medicine: linked::get_view(&conn, "medicine", "today")?,
    })
}

#[tauri::command]
pub async fn linked_refresh_blog(app: AppHandle) -> Result<Vec<linked::SnapshotView>, Error> {
    let handle = app.clone();
    let views = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<linked::SnapshotView>, Error> {
        let state = handle.state::<AppState>();
        let now = SystemNow;
        let repo = {
            let conn = state.lock();
            paths::blog_repo_dir(&conn)?
        };

        let snaps = blog_local::read_repo(&repo, &now);
        let conn = state.lock();
        for snap in &snaps {
            linked::upsert(&conn, snap)?;
        }
        linked::list_for(&conn, blog_local::SOURCE)
    })
    .await
    .map_err(|e| Error::msg(format!("联动读取任务没跑起来：{e}")))??;

    let _ = app.emit(LINK_BLOG, &views);
    Ok(views)
}

/// 线上比对：只把状态改成 stale 并写一句人话，不改缓存内容、不伪装同步。
#[tauri::command]
pub async fn linked_check_online(app: AppHandle) -> Result<Vec<linked::SnapshotView>, Error> {
    let handle = app.clone();
    let views = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<linked::SnapshotView>, Error> {
        let state = handle.state::<AppState>();
        let now = SystemNow;
        {
            let conn = state.lock();
            let enabled = settings::get_json(&conn, settings::KEY_BLOG_ONLINE_CHECK)?
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            if !enabled {
                return linked::list_for(&conn, blog_local::SOURCE);
            }
        }

        for (kind, _) in blog_local::kinds() {
            let local = {
                let conn = state.lock();
                linked::get(&conn, blog_local::SOURCE, kind)?
            };
            let Some(local) = local else { continue };
            // 本地本来就缺文件的类别，线上有没有都不该报「落后」，那是另一回事
            if !matches!(local.status, Status::Ok) {
                continue;
            }

            let (status, detail) = match blog_online::fetch_normalized(kind) {
                Ok(remote) => blog_online::compare(&local, &remote),
                Err(e) => {
                    // 网络不可达不是错误状态：缓存还在用，只是这次没比对上
                    log::debug!("线上比对 {kind} 失败：{e}");
                    continue;
                }
            };

            let conn = state.lock();
            linked::set_status(&conn, blog_local::SOURCE, kind, status, &detail, now.ms())?;
        }

        let conn = state.lock();
        linked::list_for(&conn, blog_local::SOURCE)
    })
    .await
    .map_err(|e| Error::msg(format!("比对任务没跑起来：{e}")))??;

    let _ = app.emit(LINK_BLOG, &views);
    Ok(views)
}

fn medicine_payload(day_start_ms: i64, now_minutes: i32, meds: &[medicine_math::Medicine], doses: &[medicine_math::Dose]) -> String {
    let mut items = Vec::new();
    let (mut taken, mut owed, mut low) = (0usize, 0usize, 0usize);

    for med in meds {
        let mine: Vec<medicine_math::Dose> = doses.iter().filter(|d| d.medicine_id == med.id).cloned().collect();
        let plan = medicine_math::today_plan(med, &mine, day_start_ms, now_minutes);
        taken += plan.taken;
        owed += plan.owed;
        if med.is_low() || med.is_out() {
            low += 1;
        }

        let mut meta = Map::new();
        meta.insert("schedule".into(), json!(plan.slots));
        meta.insert("taken".into(), json!(plan.taken));
        meta.insert("owed".into(), json!(plan.owed));
        meta.insert("pending".into(), json!(plan.pending));
        meta.insert("skipped".into(), json!(plan.skipped));

        items.push(json!({
            "id": med.id,
            "name": med.name,
            "unit": med.unit,
            "remaining": med.remaining,
            "boxes": if med.per_box > 0.0 { (med.remaining / med.per_box * 10.0).round() / 10.0 } else { 0.0 },
            "dailyUse": med.daily_use(),
            "daysLeft": med.days_left().map(|d| (d * 10.0).round() / 10.0),
            "lowDays": med.low_days,
            "isLow": med.is_low(),
            "isOut": med.is_out(),
            "slots": plan.slots,
            "taken": plan.taken,
            "owed": plan.owed,
            "pending": plan.pending,
            "skipped": plan.skipped,
        }));
    }

    let mut meta = Map::new();
    meta.insert("todayTaken".into(), json!(taken));
    meta.insert("todayOwed".into(), json!(owed));
    meta.insert("lowCount".into(), json!(low));
    meta.insert("nowMinutes".into(), json!(now_minutes));

    crate::linking::normalized(Value::Array(items), meta)
}

#[tauri::command]
pub async fn linked_refresh_medicine(app: AppHandle) -> Result<Option<linked::SnapshotView>, Error> {
    let handle = app.clone();
    let view = tauri::async_runtime::spawn_blocking(move || -> Result<Option<linked::SnapshotView>, Error> {
        let state = handle.state::<AppState>();
        let now = SystemNow;

        let snap = medicine_snapshot(&state, &now);

        let conn = state.lock();
        linked::upsert(&conn, &snap)?;
        linked::get_view(&conn, "medicine", "today")
    })
    .await
    .map_err(|e| Error::msg(format!("药箱读取任务没跑起来：{e}")))??;

    let _ = app.emit(LINK_MEDICINE, &view);
    Ok(view)
}

/// 本机文件 + （配了第二条应用密码时）坚果云快照，交给 `medicine_copy::refresh` 判谁更新。
///
/// 云端的失败永远不往上抛：这个部件的底线是"药箱被卸载 / 断网 / 凭据被拒，
/// 工作台照常可用"（PLAN §6.1、§6.2）。
fn medicine_snapshot(state: &AppState, now: &SystemNow) -> Snapshot {
    let (source, dav_dir) = {
        let conn = state.lock();
        (
            paths::medicine_db_path(&conn).unwrap_or_default(),
            paths::medicine_dav_dir(&conn).unwrap_or_default(),
        )
    };

    let cloud = match cloud_read(state, &dav_dir) {
        Some(read) => read,
        None => medicine_copy::CloudRead::Skipped,
    };

    medicine_copy::refresh(&source, cloud, &state.paths.tmp, now, |meds, doses, day_start, minutes| {
        medicine_payload(day_start, minutes, meds, doses)
    })
}

/// `None` = 没配第二条凭据，压根不启用云端（detail 里也不该出现"云端"字样）。
fn cloud_read(state: &AppState, dav_dir: &str) -> Option<medicine_copy::CloudRead> {
    if !crate::win32::creds::medicine_configured() {
        return None;
    }
    let read = match fetch_cloud(state, dav_dir) {
        Ok(got) => medicine_copy::CloudRead::Fresh(got.today, got.device, got.published_at),
        Err(e) => medicine_copy::CloudRead::Failed(e.to_string()),
    };
    Some(read)
}

fn fetch_cloud(state: &AppState, dav_dir: &str) -> Result<medicine_cloud::Fetched, Error> {
    let user = medicine_email(state)?;
    let pass = crate::win32::creds::medicine_password()?;
    medicine_cloud::fetch(&dav_readonly::ReadRemote::new(dav_dir, &user, &pass)?)
}

/// 云端用注册邮箱做 Basic 用户名。药箱把邮箱存在它自己的 shared_preferences 里，
/// 我们不去读那个文件（PLAN §6.2），所以要求在这里单独填一次。
fn medicine_email(state: &AppState) -> Result<String, Error> {
    let conn = state.lock();
    settings::get_json(&conn, settings::KEY_MEDICINE_EMAIL)?
        .and_then(|v| v.as_str().map(str::to_owned))
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| Error::msg("还没填药箱那边用的坚果云邮箱"))
}

/// 邮箱进 settings（它不是秘密，且要解释凭据条目属于谁），应用密码只进凭据管理器。
#[tauri::command]
pub fn linked_set_medicine_password(
    state: State<'_, AppState>,
    email: String,
    password: String,
) -> Result<bool, Error> {
    let email = email.trim().to_owned();
    if email.is_empty() {
        return Err(Error::msg("邮箱不能为空"));
    }
    // 框里空着 = 沿用凭据管理器里那条（换页回来密码框本来就不回显）
    if let Some(p) = crate::win32::creds::keep_or_set(&password, crate::win32::creds::medicine_configured())? {
        crate::win32::creds::set_medicine_password(&p)?;
    }
    let now = SystemNow;
    let conn = state.lock();
    settings::set_json(
        &conn,
        settings::KEY_MEDICINE_EMAIL,
        &json!(email),
        now.ms(),
        &state.device_id,
    )?;
    Ok(true)
}

#[tauri::command]
pub fn linked_clear_medicine_password(state: State<'_, AppState>) -> Result<bool, Error> {
    crate::win32::creds::clear_medicine_password();
    let now = SystemNow;
    let conn = state.lock();
    settings::set_json(
        &conn,
        settings::KEY_MEDICINE_EMAIL,
        &Value::Null,
        now.ms(),
        &state.device_id,
    )?;
    Ok(false)
}

/// 改药箱的同步目录地址。空串恢复默认。填错当场拒，不等下次刷新才发现。
#[tauri::command]
pub fn linked_set_medicine_dir(state: State<'_, AppState>, dir: String) -> Result<String, Error> {
    let now = SystemNow;
    let conn = state.lock();
    let value = if dir.trim().is_empty() {
        dav_readonly::DEFAULT_BASE.to_owned()
    } else {
        dav_readonly::ReadRemote::check_dir(&dir)?;
        dir.trim().trim_matches('"').to_owned()
    };
    settings::set_json(
        &conn,
        settings::KEY_MEDICINE_DAV_DIR,
        &json!(value),
        now.ms(),
        &state.device_id,
    )?;
    Ok(value)
}

/// 改药箱库的位置。空串恢复默认（`%APPDATA%\dev.mine\medicine_tracker\medicine_tracker.db`）。
///
/// 与博客那条一样只做展开与去引号，**不要求文件此刻存在**：药箱可能压根没装在这台机器上，
/// 那正是云端那条路的存在理由（§6.2），在这里拦一道就把用户的正当配置挡死了。
#[tauri::command]
pub fn linked_set_medicine_db(
    state: State<'_, AppState>,
    path: String,
) -> Result<paths::SourcePaths, Error> {
    let now = SystemNow;
    let conn = state.lock();
    match paths::normalize_dir(&path) {
        Some(dir) => settings::set_json(
            &conn,
            settings::KEY_MEDICINE_DB,
            &json!(dir.display().to_string()),
            now.ms(),
            &state.device_id,
        )?,
        None => {
            conn.execute(
                "DELETE FROM settings WHERE key = ?1",
                rusqlite::params![settings::KEY_MEDICINE_DB],
            )?;
        }
    }
    paths::resolve(&conn, &now, &state.device_id)
}

/// 「补买 X」是用户点出来的，不是系统偷偷建的（PLAN §6.1：副本读失败不生成任何待办）。
#[tauri::command]
pub fn linked_restock_todo(
    app: AppHandle,
    name: String,
    days_left: Option<f64>,
) -> Result<Todo, Error> {
    let state = app.state::<AppState>();
    let now = SystemNow;
    let device = state.device_id.to_string();
    let suffix = match days_left {
        Some(d) if d <= 0.0 => "已用完".to_owned(),
        Some(d) => format!("剩余 {} 天", (d * 10.0).round() / 10.0),
        None => "库存待确认".to_owned(),
    };
    let mut conn = state.lock();
    let tx = conn.transaction()?;
    let todo = todos::create(
        &tx,
        &now,
        &device,
        crate::model::TodoDraft {
            title: format!("补买 {name}（{suffix}）"),
            zone: Some(crate::model::zones::TODAY.to_owned()),
            priority: Some(crate::model::priority::HIGH),
            tags: vec!["跑腿".to_owned()],
            ..Default::default()
        },
    )?;
    tx.commit()?;
    Ok(todo)
}

#[tauri::command]
pub fn linked_set_blog_dir(state: State<'_, AppState>, path: String) -> Result<paths::SourcePaths, Error> {
    let now = SystemNow;
    let conn = state.lock();
    match paths::normalize_dir(&path) {
        Some(dir) => settings::set_json(
            &conn,
            settings::KEY_BLOG_REPO_DIR,
            &json!(dir.display().to_string()),
            now.ms(),
            &state.device_id,
        )?,
        None => {
            // 空串 = 恢复默认，删掉这一行比写一个空字符串更干净
            conn.execute(
                "DELETE FROM settings WHERE key = ?1",
                rusqlite::params![settings::KEY_BLOG_REPO_DIR],
            )?;
        }
    }
    paths::resolve(&conn, &now, &state.device_id)
}
