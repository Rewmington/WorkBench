use std::path::Path;

use sha2::{Digest, Sha256};
use tauri::State;

use crate::error::Error;
use crate::repos::resources::{self, Resource, ResourceDraft};
use crate::state::AppState;
use crate::time::{Now, SystemNow};

/// 图标文件名按目标路径哈希：同一个 exe 被拖进来两次不会长出两份 PNG。
fn icon_file_name(target: &str) -> String {
    let mut h = Sha256::new();
    h.update(target.as_bytes());
    let digest = h.finalize();
    format!(
        "{}.png",
        &digest
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()[..16]
    )
}

fn detect_kind(target: &str) -> &'static str {
    let lower = target.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
    {
        return "web";
    }
    // 用 lower 判后缀：Windows 上 .EXE 大写很常见，漏判会让它变成「文件」而不是「应用」
    match Path::new(&lower).extension().and_then(|e| e.to_str()) {
        Some(ext) if ext == "exe" || ext == "lnk" => "app",
        _ => "file",
    }
}

fn default_name(target: &str) -> String {
    Path::new(target)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(target)
        .to_owned()
}

/// 拖进来一个 exe / lnk / 文件：提图标、落 PNG、建条目。
/// 图标提不到不拦创建 —— 磁贴会退化成首字母，功能不缺（§2 原则 2）。
#[tauri::command]
pub fn resource_add_from_path(
    state: State<'_, AppState>,
    path: String,
    zone_id: Option<String>,
) -> Result<Resource, Error> {
    let path = path.trim().trim_matches('"').to_owned();
    if !Path::new(&path).exists() {
        return Err(Error::msg("文件不存在，可能被移动或删除了"));
    }

    // 提不到图标不拦创建：磁贴退化成首字母，功能不缺（§2 原则 2）
    let icon_path = match crate::win32::icons::extract_png(&path) {
        Ok(bytes) => {
            let file = state.paths.icons.join(icon_file_name(&path));
            match std::fs::write(&file, bytes) {
                Ok(()) => Some(file.display().to_string()),
                Err(e) => {
                    log::warn!("图标缓存写入失败：{e}");
                    None
                }
            }
        }
        Err(e) => {
            log::debug!("没有提取到图标（{e}），磁贴用首字母");
            None
        }
    };

    let now = SystemNow;
    let device = state.device_id.to_string();
    let draft = ResourceDraft {
        kind: detect_kind(&path).to_owned(),
        name: default_name(&path),
        target: path,
        args: String::new(),
        icon_path,
        zone_id,
        note: String::new(),
        secret: None,
    };
    let conn = state.lock();
    resources::create(&conn, &now, &device, draft)
}

#[tauri::command]
pub fn resource_add_web(
    state: State<'_, AppState>,
    name: String,
    url: String,
    zone_id: Option<String>,
) -> Result<Resource, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let draft = ResourceDraft {
        kind: "web".to_owned(),
        name,
        target: url,
        args: String::new(),
        icon_path: None,
        zone_id,
        note: String::new(),
        secret: None,
    };
    let conn = state.lock();
    resources::create(&conn, &now, &device, draft)
}

/// 原生文件对话框选目标。**在 Rust 侧弹**：前端要用对话框就得再装一个 JS 插件包，
/// 而这里选完紧接着就要提图标入库，少一次往返也少一处能出错的地方。
#[tauri::command]
pub fn resource_pick(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    zone_id: Option<String>,
) -> Result<Option<Resource>, Error> {
    use tauri_plugin_dialog::DialogExt;

    let picked = app
        .dialog()
        .file()
        .set_title("选择要钉到速达的程序或文件")
        .blocking_pick_file();
    let Some(path) = picked else {
        return Ok(None);
    };
    let as_str = path
        .as_path()
        .map(|p| p.display().to_string())
        .ok_or_else(|| Error::msg("选中的路径读不出来（可能是网络位置或临时项）"))?;
    drop(app);

    let path = as_str;
    let lower = path.to_ascii_lowercase();
    let kind = if lower.ends_with(".exe") || lower.ends_with(".lnk") {
        "app"
    } else {
        "file"
    };
    let icon_path = match crate::win32::icons::extract_png(&path) {
        Ok(bytes) => {
            let file = state.paths.icons.join(icon_file_name(&path));
            std::fs::write(&file, bytes)
                .ok()
                .map(|()| file.display().to_string())
        }
        Err(_) => None,
    };

    let now = SystemNow;
    let device = state.device_id.to_string();
    let draft = ResourceDraft {
        kind: kind.to_owned(),
        name: default_name(&path),
        target: path,
        args: String::new(),
        icon_path,
        zone_id,
        note: String::new(),
        secret: None,
    };
    let conn = state.lock();
    Ok(Some(resources::create(&conn, &now, &device, draft)?))
}

#[tauri::command]
pub fn resources_list(state: State<'_, AppState>) -> Result<Vec<Resource>, Error> {
    let conn = state.lock();
    resources::list(&conn)
}

/// 启动。返回 "started" 或 "raised"，前端据此决定要不要给一句「已经帮你切过去了」。
#[tauri::command]
pub fn resource_launch(state: State<'_, AppState>, id: String) -> Result<String, Error> {
    let (target, args) = {
        let conn = state.lock();
        let row = resources::get(&conn, &id)?.ok_or_else(|| Error::msg("速达条目不存在"))?;
        (row.target, row.args)
    }; // 先放锁：ShellExecute 可能几百毫秒，不该占着数据库

    let outcome = crate::win32::launch::launch_or_raise(&target, &args)?;
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    resources::record_launch(&conn, &now, &device, &id)?;

    Ok(match outcome {
        crate::win32::launch::LaunchOutcome::Raised => "raised".into(),
        crate::win32::launch::LaunchOutcome::Started(_) => "started".into(),
    })
}

#[tauri::command]
pub fn resource_rename(state: State<'_, AppState>, id: String, name: String) -> Result<(), Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    resources::rename(&conn, &now, &device, &id, &name)
}

#[tauri::command]
pub fn resource_move(
    state: State<'_, AppState>,
    id: String,
    zone_id: Option<String>,
) -> Result<(), Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    resources::move_to_zone(&conn, &now, &device, &id, zone_id)
}

#[tauri::command]
pub fn resource_delete(state: State<'_, AppState>, id: String) -> Result<(), Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    resources::soft_delete(&conn, &now, &device, &id)
}

#[tauri::command]
pub fn resource_set_secret(
    state: State<'_, AppState>,
    id: String,
    text: String,
) -> Result<(), Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    if text.trim().is_empty() {
        return resources::set_secret(&conn, &id, "", now.ms(), &device);
    }
    resources::set_secret(&conn, &id, &text, now.ms(), &device)
}

/// 只有点开某一条才解密这一条的备注。
#[tauri::command]
pub fn resource_reveal_secret(
    state: State<'_, AppState>,
    id: String,
) -> Result<Option<String>, Error> {
    let conn = state.lock();
    resources::reveal_secret(&conn, &id)
}

#[tauri::command]
pub fn zones_list(state: State<'_, AppState>) -> Result<Vec<resources::Zone>, Error> {
    let conn = state.lock();
    resources::list_zones(&conn)
}

#[tauri::command]
pub fn zone_add(state: State<'_, AppState>, name: String) -> Result<resources::Zone, Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    resources::add_zone(&conn, &now, &device, &name)
}

#[tauri::command]
pub fn zone_delete(state: State<'_, AppState>, id: String) -> Result<(), Error> {
    let now = SystemNow;
    let device = state.device_id.to_string();
    let conn = state.lock();
    resources::delete_zone(&conn, &now, &device, &id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_are_split_by_target_shape() {
        assert_eq!(detect_kind("https://wmddd.online"), "web");
        assert_eq!(detect_kind("C:\\Tools\\anything.EXE"), "app");
        assert_eq!(detect_kind(r"C:\Users\hp\Desktop\笔记.lnk"), "app");
        assert_eq!(detect_kind(r"C:\data\report.pdf"), "file");
    }

    #[test]
    fn display_name_drops_the_extension_and_keeps_the_stem() {
        assert_eq!(default_name(r"C:\a\b\notepad.exe"), "notepad");
        assert_eq!(default_name(r"C:\a\快捷方式.lnk"), "快捷方式");
        // web 条目走 resource_add_web，名字由用户填，不经过这里
    }

    #[test]
    fn icon_names_are_stable_and_short() {
        let a = icon_file_name(r"C:\a\notepad.exe");
        let b = icon_file_name(r"C:\a\notepad.exe");
        assert_eq!(a, b);
        assert_ne!(a, icon_file_name(r"C:\b\notepad.exe"));
        assert_eq!(a.len(), "0000000000000000.png".len());
        assert!(a.ends_with(".png"));
    }
}
