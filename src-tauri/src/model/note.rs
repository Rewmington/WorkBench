use serde::{Deserialize, Serialize};

use crate::sync::lww::Stamp;

/// 一篇笔记。**正文 Markdown 就是真相源**（PLAN §4），预览是它的派生物，
/// 所以这里不存任何 HTML —— 存了两份就迟早会对不上。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    #[serde(flatten)]
    pub stamp: Stamp,
    pub title: String,
    pub body_md: String,
    pub folder_id: Option<String>,
    pub pinned: bool,
    /// 回收站用的时间戳，与 `deleted_at`（同步墓碑）是两件事：
    /// 进回收站的笔记还要能被还原，而墓碑意味着这条记录本身要被传播删除。
    pub trashed_at: Option<i64>,
    pub created_day: Option<i64>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NoteDraft {
    pub title: String,
    #[serde(default)]
    pub body_md: String,
    #[serde(default)]
    pub folder_id: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotePatch {
    #[serde(default)]
    pub title: Option<String>,
    /// 双层 Option：外层 None = 不改这个字段，内层 None = 清空它（移到无文件夹那一层）。
    #[serde(default)]
    pub body_md: Option<String>,
    #[serde(default)]
    pub folder_id: Option<Option<String>>,
    #[serde(default)]
    pub pinned: Option<bool>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteFolder {
    #[serde(flatten)]
    pub stamp: Stamp,
    pub name: String,
    pub parent_id: Option<String>,
    pub sort: f64,
    /// 直属笔记数。列表要显示计数，让前端自己数等于逼它把全部笔记拉一遍。
    pub note_count: i64,
}
