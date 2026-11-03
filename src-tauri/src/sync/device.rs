use std::path::Path;

use crate::error::Result;
use uuid::Uuid;

/// `created_by` / `updated_by` 用的本机设备 id。
///
/// 存文件而不是存库：备份里含库，整文件恢复之后如果复活了旧机器的 id，
/// 「最后写入设备」在两台机器上就会说同一个名字 —— 那是谎话（§2 原则 5）。
/// 所以 `backup::restore()` 换完库要重新写这个文件。
pub fn load_or_create(path: &Path) -> Result<String> {
    match std::fs::read_to_string(path) {
        Ok(raw) => {
            let trimmed = raw.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_owned());
            }
            create(path)
        }
        // 文件不存在或内容是空白才轮到生成。权限、目录、损坏设备这些读取故障不能
        // 顺手折成"没设过"：换一把 id 会让本机历史每一行的 `updated_by` 变成别人写的，
        // 平票裁决和待上传计数全部失去依据。宁可拒绝启动，也不制造一个不可解释的新身份。
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => create(path),
        Err(e) => Err(crate::error::Error::Io(e)),
    }
}

fn create(path: &Path) -> Result<String> {
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

    /// 「读不到这个文件」有好几种：真没有、是目录、权限不让读。后面两种绝不能折成
    /// "没设过"再生成新身份，否则一次瞬时环境故障就会把本机历史全部变成别人写的。
    #[test]
    fn a_device_id_that_cannot_be_read_is_not_treated_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("device.id");
        std::fs::create_dir(&p).unwrap();

        let err = load_or_create(&p).unwrap_err().to_string();
        assert!(err.contains("读写文件出错"), "{err}");
        assert!(p.is_dir(), "失败时不许顺手把那个占位对象换成一把新 id");
    }
}
