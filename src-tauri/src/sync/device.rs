use std::path::Path;

use crate::error::Result;
use uuid::Uuid;

/// `created_by` / `updated_by` 用的本机设备 id。
///
/// 存文件而不是存库：备份里含库，整文件恢复之后如果复活了旧机器的 id，
/// 「最后写入设备」在两台机器上就会说同一个名字 —— 那是谎话（§2 原则 5）。
/// 所以 `backup::restore()` 换完库要重新写这个文件。
pub fn load_or_create(path: &Path) -> Result<String> {
    if let Ok(raw) = std::fs::read_to_string(path) {
        let trimmed = raw.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_owned());
        }
    }
    // UUIDv7 时间有序，前 12 位十六进制就能看出先后；加 pc- 前缀是让详情面板里肉眼可认。
    let id = format!("pc-{}", Uuid::now_v7().simple());
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, &id)?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_read_returns_the_same_id() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("device.id");
        let a = load_or_create(&p).unwrap();
        let b = load_or_create(&p).unwrap();
        assert_eq!(a, b);
        assert!(a.starts_with("pc-"));
    }

    #[test]
    fn empty_file_is_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("device.id");
        std::fs::write(&p, "  \n").unwrap();
        let id = load_or_create(&p).unwrap();
        assert!(id.starts_with("pc-"));
    }
}
