use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::error::Result;
use crate::repos::settings;
use crate::time::Now;

/// 外部源的路径解析与展示值。设置页要**原样显示绝对路径**（§2 原则 4），
/// 所以这里返回的一律是绝对路径字符串，不给「已配置」这种含糊话。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourcePaths {
    pub blog_repo: String,
    pub blog_repo_exists: bool,
    pub medicine_db: String,
    pub medicine_db_exists: bool,
    pub online_check_enabled: bool,
}

/// 本机博客仓库的默认位置。这是**这台机器**的路径，不是产品常量 ——
/// 目录被移动时不用改代码，在设置页里改掉即可（改了也不会写坏对方仓库，我们只读）。
const DEFAULT_BLOG_REPO: &str = r"D:\ADevelop\Repositories\Mine\Rewmington-Blog-main";

pub fn blog_repo_dir(conn: &Connection) -> Result<PathBuf> {
    Ok(match settings::get_json(conn, settings::KEY_BLOG_REPO_DIR)? {
        Some(serde_json::Value::String(s)) if !s.trim().is_empty() => PathBuf::from(s),
        _ => PathBuf::from(DEFAULT_BLOG_REPO),
    })
}

pub fn medicine_db_path(conn: &Connection) -> Result<PathBuf> {
    if let Some(serde_json::Value::String(s)) =
        settings::get_json(conn, settings::KEY_MEDICINE_DB)?
    {
        if !s.trim().is_empty() {
            return Ok(PathBuf::from(s));
        }
    }
    // 药箱的库在 Roaming 下（它自己选的位置）。我们只读、且先复制再读，
    // 所以它选哪儿都不影响我们 —— 但**不跟着它去 Roaming 放我们自己的库**（PLAN §5.0）。
    let base = dirs::config_dir().ok_or_else(|| crate::error::Error::msg("找不到 AppData\\Roaming"))?;
    Ok(base
        .join("dev.mine")
        .join("medicine_tracker")
        .join("medicine_tracker.db"))
}

pub fn resolve(conn: &Connection, now: &dyn Now, device: &str) -> Result<SourcePaths> {
    let blog = blog_repo_dir(conn)?;
    let med = medicine_db_path(conn)?;
    let online = settings::get_or_default(
        conn,
        settings::KEY_BLOG_ONLINE_CHECK,
        &serde_json::json!(true),
        now.ms(),
        device,
    )?;

    Ok(SourcePaths {
        blog_repo: blog.display().to_string(),
        blog_repo_exists: blog.is_dir(),
        medicine_db: med.display().to_string(),
        medicine_db_exists: med.is_file(),
        online_check_enabled: online.as_bool().unwrap_or(true),
    })
}

/// 供设置页「改路径」用：把用户填的字符串变成绝对路径，空串表示恢复默认。
pub fn normalize_dir(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim().trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }
    Some(Path::new(trimmed).to_path_buf())
}
