use serde::{Deserialize, Serialize};

use crate::sync::lww::Stamp;

/// 便签：固定槽位 + 纯文本。600 ms 防抖是前端的事，这里只保证「同一个槽位只有一条活的」。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sticky {
    #[serde(flatten)]
    pub stamp: Stamp,
    pub slot: i64,
    pub content: String,
}

/// 倒计时模式（PLAN §5.2）。
pub mod cd_mode {
    pub const DURATION: &str = "duration";
    pub const TIMED: &str = "timed";
    pub const DAILY: &str = "daily";
    pub const INTERVAL: &str = "interval";
}

/// `state` 与 `paused` 分工：state 说这支钟走到哪一步，paused 只说它是不是被冻住。
pub mod cd_state {
    pub const IDLE: &str = "idle";
    pub const RUNNING: &str = "running";
    pub const PAUSED: &str = "paused";
    pub const FINISHED: &str = "finished";
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Countdown {
    #[serde(flatten)]
    pub stamp: Stamp,
    pub label: String,
    pub mode: String,
    /// 模式相关的参数，形状见 `deadline_for`。存 JSON 而不是四列，是因为每种模式
    /// 只用得到其中一两个字段，摊平成列会留一堆对某几种模式没意义的 NULL。
    pub spec: serde_json::Value,
    pub ends_at: Option<i64>,
    pub paused: bool,
    pub state: String,
    /// 只在 paused 时有值：冻住的剩余毫秒。见 0003 的注释。
    pub remaining_ms: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CountdownDraft {
    pub label: String,
    pub mode: String,
    pub spec: serde_json::Value,
}

/// 系统资源部件的一帧。CPU 要两次采样才出得来，所以它是可空的。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SysLoad {
    pub cpu_percent: Option<f64>,
    pub mem_total: u64,
    pub mem_avail: u64,
}
