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
    /// 药箱在坚果云上的同步目录。`cloud_ready` 是「配了那第二条应用密码」，
    /// 不是「云端一定连得上」—— 那要发一次请求才知道，而首屏不发。
    pub medicine_dav: String,
    pub medicine_cloud_ready: bool,
    pub online_check_enabled: bool,
    /// 三个「不改时用它」的值。界面上那个「恢复默认」按钮必须知道默认是什么，
    /// 而默认值只该有一份 —— 以前 TS 里手抄了一份坚果云地址，改 Rust 不会通知它。
    pub blog_repo_default: String,
    pub medicine_db_default: String,
    pub medicine_dav_default: String,
}

/// 本机博客仓库的默认位置。这是**这台机器**的路径，不是产品常量 ——
/// 目录被移动时不用改代码，在设置页里改掉即可（改了也不会写坏对方仓库，我们只读）。
const DEFAULT_BLOG_REPO: &str = r"D:\ADevelop\Repositories\Mine\Rewmington-Blog-main";

pub fn blog_repo_default() -> PathBuf {
    PathBuf::from(DEFAULT_BLOG_REPO)
}

pub fn blog_repo_dir(conn: &Connection) -> Result<PathBuf> {
    Ok(match settings::get_json(conn, settings::KEY_BLOG_REPO_DIR)? {
        Some(serde_json::Value::String(s)) if !s.trim().is_empty() => PathBuf::from(s),
        _ => blog_repo_default(),
    })
}

/// 药箱的库在 Roaming 下（它自己选的位置）。我们只读、且先复制再读，
/// 所以它选哪儿都不影响我们 —— 但**不跟着它去 Roaming 放我们自己的库**（PLAN §5.0）。
pub fn medicine_db_default() -> Result<PathBuf> {
    let base = dirs::config_dir().ok_or_else(|| crate::error::Error::msg("找不到 AppData\\Roaming"))?;
    Ok(base
        .join("dev.mine")
        .join("medicine_tracker")
        .join("medicine_tracker.db"))
}

pub fn medicine_db_path(conn: &Connection) -> Result<PathBuf> {
    if let Some(serde_json::Value::String(s)) =
        settings::get_json(conn, settings::KEY_MEDICINE_DB)?
    {
        if !s.trim().is_empty() {
            return Ok(PathBuf::from(s));
        }
    }
    medicine_db_default()
}

pub fn medicine_dav_default() -> String {
    crate::linking::dav_readonly::DEFAULT_BASE.to_owned()
}

/// 药箱在坚果云上的同步目录。默认值与本地桩都走 `dav_readonly`，
/// 免得两处各写一份 URL、改的时候只改到一半。
pub fn medicine_dav_dir(conn: &Connection) -> Result<String> {
    Ok(match settings::get_json(conn, settings::KEY_MEDICINE_DAV_DIR)? {
        Some(serde_json::Value::String(s)) if !s.trim().is_empty() => s.trim().to_owned(),
        _ => medicine_dav_default(),
    })
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
        medicine_dav: medicine_dav_dir(conn)?,
        medicine_cloud_ready: crate::win32::creds::medicine_configured(),
        online_check_enabled: online.as_bool().unwrap_or(true),
        blog_repo_default: blog_repo_default().display().to_string(),
        medicine_db_default: medicine_db_default()?.display().to_string(),
        medicine_dav_default: medicine_dav_default(),
    })
}

/// 供设置页「改路径」用：把用户填的字符串变成绝对路径，空串表示恢复默认。
///
/// 相对路径按当前工作目录展开 —— 函数名以前叫「规范化」但并没有真的变绝对，
/// 填个 `foo\bar` 会存成相对路径，之后 `is_dir()` 拿它对着进程 CWD 判断，
/// 于是出现「路径看着没错，就是永远说目录不可用」。
pub fn normalize_dir(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim().trim_matches('"');
    if trimmed.is_empty() {
        return None;
    }
    let p = Path::new(trimmed);
    Some(if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_default().join(p)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_paths_get_expanded_instead_of_stored_as_is() {
        // 空串 = 恢复默认，交给调用方删行
        assert!(normalize_dir("").is_none());
        assert!(normalize_dir("   ").is_none());
        // 带引号：从资源管理器「复制文件地址」粘出来就是这个样子
        let quoted = normalize_dir(r#"  "D:\somewhere\blog"  "#).unwrap();
        assert_eq!(quoted, Path::new(r"D:\somewhere\blog"));

        let rel = normalize_dir(r"repo\blog").unwrap();
        assert!(rel.is_absolute(), "相对路径必须展开成绝对：{rel:?}");
        assert!(rel.ends_with(r"repo\blog"));
    }
}
