//! 外部联动（只读）。
//!
//! 三条共同点写在类型上：外部数据只进 `linked_snapshots` 缓存表，永不混进 `todos`
//! 等主表；读不到就 `missing`/`error` 加一行说明，界面折叠成虚线条；
//! 任何一条都不弹框、不伪装「已同步」（PLAN §2 原则 5、§6.1）。

pub mod blog_local;
pub mod blog_online;
pub mod dav_readonly;
pub mod medicine_cloud;
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

/// 一份「今天的药箱数据」。本机库和云端快照都折算成它，于是比较与渲染只有一条路径。
#[derive(Debug, Clone, Default)]
pub struct Today {
    pub meds: Vec<medicine_math::Medicine>,
    pub doses: Vec<medicine_math::Dose>,
    /// 这份数据里最新一次行改动，**含墓碑**（删除也是一次更新）。
    /// 跨来源比谁更新只用这一个数，口径与药箱自己的 LWW 一致。
    pub fresh_ms: i64,
}

/// 两个来源各有一份今天的药箱数据时信谁。
///
/// 相等时选**本机**：那是用户眼前的 app 刚写的，拿一份云端快照盖掉它需要更强的理由，
/// 而不是一个并列。反过来云端更新（手机上刚打过卡、PC 端药箱还没开）才用它。
pub fn cloud_is_fresher(cloud_ms: i64, local_ms: i64) -> bool {
    cloud_ms > local_ms
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

#[cfg(test)]
mod tests {
    use super::cloud_is_fresher;

    #[test]
    fn ties_go_to_the_local_copy_not_the_cloud() {
        assert!(cloud_is_fresher(200, 100), "云端更新时用云端");
        assert!(!cloud_is_fresher(100, 100), "相等时用本机 —— 那是眼前的 app 刚写的");
        assert!(!cloud_is_fresher(50, 100));
    }
}
