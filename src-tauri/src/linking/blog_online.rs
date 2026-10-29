//! 线上比对。**只用来判断本地缓存是否落后**，不是数据源：
//! 站点在 GitHub Pages 上每天 cron 重建，慢的时候首包要 8 秒，
//! 所以这条路径永远在 spawn_blocking 里跑、有总预算、失败进抑制窗口（PLAN §6.1）。

use std::time::Duration;

use serde_json::Value;
use ureq::Agent;

use super::{sig, Snapshot, Status};
use crate::linking::blog_local::normalize_kind;

pub const BASE: &str = "https://wmddd.online/content/";

/// 单次请求的总预算。站点冷启实测 ~8 s，12 s 留一点余量又不至于挂着不放。
pub const BUDGET: Duration = Duration::from_secs(12);

fn agent() -> Agent {
    // 三条 https 路共用一份底座：TLS provider 配错时 ureq 是 panic 而不是返回错误，
    // 而 panic 在 release 里等于整个应用消失（见 `crate::net`）。
    crate::net::agent(BUDGET, Duration::from_secs(3), "Anvil/0.1 (本机工作台，仅比对本地缓存是否落后)")
}

/// 拉一个清单并归一化成与本地读取同样的形状，这样两边 sig 才可直接比。
pub fn fetch_normalized(kind: &str) -> Result<String, String> {
    let url = format!("{BASE}{kind}.json");
    let body = agent()
        .get(&url)
        .call()
        .map_err(|e| format!("取不到 {url}：{e}"))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("读 {url} 失败：{e}"))?;

    let value: Value = serde_json::from_str(&body).map_err(|e| format!("{kind} 线上内容不是合法 JSON：{e}"))?;
    Ok(normalize_kind(kind, &value))
}

/// 本地 vs 线上。签名不同就是本地落后 —— 但**不说「已同步」**：
/// 我们没写任何东西上去，只是发现两边不一样（§2 原则 5）。
pub fn compare(local: &Snapshot, remote_payload: &str) -> (Status, String) {
    let remote_sig = sig::content_sig(remote_payload);
    if remote_sig == local.sig {
        (Status::Ok, String::new())
    } else {
        let local_count = count_of(&local.payload_json);
        let remote_count = count_of(remote_payload);
        (
            Status::Stale,
            format!(
                "线上 {remote_count} 条 / 本地缓存 {local_count} 条 —— 本地仓库还没同步，工作台读的是缓存"
            ),
        )
    }
}

fn count_of(payload: &str) -> usize {
    serde_json::from_str::<Value>(payload)
        .ok()
        .and_then(|v| v.get("count").and_then(Value::as_u64).map(|c| c as usize))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linking::empty_payload;

    fn snap(payload: &str) -> Snapshot {
        Snapshot {
            source: "blog".into(),
            kind: "projects".into(),
            fetched_at: 1,
            sig: sig::content_sig(payload),
            status: Status::Ok,
            detail: String::new(),
            payload_json: payload.to_owned(),
        }
    }

    #[test]
    fn identical_content_is_ok_and_different_content_is_stale() {
        let same = r#"{"count":1,"items":[{"name":"a"}],"meta":{}}"#;
        let (status, detail) = compare(&snap(same), same);
        assert_eq!(status, Status::Ok);
        assert!(detail.is_empty());

        let other = r#"{"count":2,"items":[{"name":"a"},{"name":"b"}],"meta":{}}"#;
        let (status, detail) = compare(&snap(same), other);
        assert_eq!(status, Status::Stale);
        assert!(detail.contains("线上 2 条") && detail.contains("本地缓存 1 条"), "{detail}");
    }

    #[test]
    fn an_empty_cache_does_not_panic_into_a_wrong_verdict() {
        let (status, _) = compare(&snap(&empty_payload()), r#"{"count":1,"items":[{"name":"a"}],"meta":{}}"#);
        assert_eq!(status, Status::Stale);
    }
}
