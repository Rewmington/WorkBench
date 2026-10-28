use serde_json::{json, Map, Value};
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::Error;
use crate::linking::{blog_local, blog_online, medicine_copy, medicine_math, paths, Status};
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

        let source = {
            let conn = state.lock();
            paths::medicine_db_path(&conn)?
        };

        let snap = medicine_copy::snapshot_for(&source, &state.paths.tmp, &now, |meds, doses, day_start, now_minutes| {
            medicine_payload(day_start, now_minutes, meds, doses)
        });

        let conn = state.lock();
        linked::upsert(&conn, &snap)?;
        linked::get_view(&conn, "medicine", "today")
    })
    .await
    .map_err(|e| Error::msg(format!("药箱读取任务没跑起来：{e}")))??;

    let _ = app.emit(LINK_MEDICINE, &view);
    Ok(view)
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
