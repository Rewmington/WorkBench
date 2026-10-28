use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("数据库错误：{0}")]
    Db(#[from] rusqlite::Error),

    #[error("读写文件出错：{0}")]
    Io(#[from] std::io::Error),

    #[error("迁移 {name} 未通过：{source}")]
    Migration {
        name: &'static str,
        source: rusqlite::Error,
    },

    #[error("{0}")]
    Message(String),

    #[error("窗口/菜单：{0}")]
    Tauri(#[from] tauri::Error),

    #[error("网络错误：{0}")]
    Net(String),

    #[error("解密失败：快照可能来自另一台设备，或应用密码不对")]
    Decrypt,
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<String> for Error {
    fn from(value: String) -> Self {
        Self::Message(value)
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        Self::Message(value.to_owned())
    }
}

/// 前端拿到的是中文句子本身 —— 命令的 Err 分支直接进 UI 文案，不再二次映射。
impl serde::Serialize for Error {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl Error {
    pub fn msg(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }
}
