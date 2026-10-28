use serde::{Deserialize, Serialize};

use crate::sync::lww::Stamp;

/// 待办分区。`inbox` 是「未整理捕获」，§8.8.4 的第 5 条规则数的就是它。
pub mod zones {
    pub const INBOX: &str = "inbox";
    pub const TODAY: &str = "today";
    pub const LATER: &str = "later";
    pub const SOMEDAY: &str = "someday";
}

/// 0 最急。状态不只靠颜色（§2 原则 3）：优先级同时改文字与图标，不只改色点。
pub mod priority {
    pub const URGENT: i64 = 0;
    pub const HIGH: i64 = 1;
    pub const NORMAL: i64 = 2;
    pub const LOW: i64 = 3;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Todo {
    #[serde(flatten)]
    pub stamp: Stamp,

    pub title: String,
    pub note: String,
    /// 取消勾选时**不清空**，只递增 rev（PLAN §5.1）—— 清空会让历史统计少一格。
    pub done_at: Option<i64>,
    /// done_at 对应的本地自然日 yyyymmdd，统计按它分组（见 migrations 注释）
    pub done_day: Option<i64>,
    pub due_at: Option<i64>,
    pub remind_at: Option<i64>,
    pub priority: i64,
    pub pinned: bool,
    pub parent_id: Option<String>,
    pub sort_key: f64,
    pub zone: String,
    pub created_day: Option<i64>,
    pub created_hour: Option<i64>,
    /// 列表接口顺带填上，省掉前端 N+1 次查询
    pub tags: Vec<String>,
}

impl Todo {
    pub fn is_done(&self) -> bool {
        self.done_at.is_some()
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TodoDraft {
    pub title: String,
    #[serde(default)]
    pub note: String,
    #[serde(default)]
    pub zone: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub due_at: Option<i64>,
    #[serde(default)]
    pub remind_at: Option<i64>,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// 局部更新。`Option<Option<T>>` 是有意的：外层 None = 不改这个字段，
/// 内层 None = 把它清空（比如去掉截止时间）。少一层就没法表达「清空截止」。
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TodoPatch {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub note: Option<Option<String>>,
    #[serde(default)]
    pub zone: Option<String>,
    #[serde(default)]
    pub priority: Option<i64>,
    #[serde(default)]
    pub due_at: Option<Option<i64>>,
    #[serde(default)]
    pub remind_at: Option<Option<i64>>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub parent_id: Option<Option<String>>,
}
