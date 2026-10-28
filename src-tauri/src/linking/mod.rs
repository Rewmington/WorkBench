//! 外部联动（只读）。
//!
//! 三条共同点写在类型上：外部数据只进 `linked_snapshots` 缓存表，永不混进 `todos`
//! 等主表；读不到就 `missing`/`error` 加一行说明，界面折叠成虚线条；
//! 任何一条都不弹框、不伪装「已同步」（PLAN §2 原则 5、§6.1）。

pub mod blog_local;
pub mod blog_online;
pub mod medicine_copy;
pub mod medicine_math;
pub mod paths;
pub mod sig;

use serde::Serialize;

/// 快照状态。`stale` 只用于「本地缓存落后于线上」这一种情况。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Missing,
    Stale,
    Error,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Missing => "missing",
            Self::Stale => "stale",
            Self::Error => "error",
        }
    }

    pub fn parse(raw: &str) -> Self {
        match raw {
            "ok" => Self::Ok,
            "missing" => Self::Missing,
            "stale" => Self::Stale,
            _ => Self::Error,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub source: String,
    pub kind: String,
    pub fetched_at: i64,
    pub sig: String,
    pub status: Status,
    pub detail: String,
    pub payload_json: String,
}

/// 归一化后的载荷：`{items, count, meta}`。
///
/// 博客那边数组/对象混着来（`projects.json` 是数组，`github-repos.json` 是
/// `{fetchedAt, repos:[…]}`），不先归一化的话每个消费方都要再判一次形状。
pub fn normalized(items: serde_json::Value, meta: serde_json::Map<String, serde_json::Value>) -> String {
    let count = match &items {
        serde_json::Value::Array(list) => list.len(),
        serde_json::Value::Null => 0,
        other => usize::from(!other.is_null()),
    };
    serde_json::json!({ "items": items, "count": count, "meta": serde_json::Value::Object(meta) })
        .to_string()
}

pub fn empty_payload() -> String {
    normalized(serde_json::Value::Array(Vec::new()), serde_json::Map::new())
}
