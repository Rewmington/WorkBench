//! 博客的**线上**清单。M10b 起它是主源：站点由 GitHub Actions 每天 cron 重建，
//! 本地 checkout 不会同步 —— 读本地那份等于拿旧数据冒充「你现在所有的项目」（PLAN §6.8）。
//!
//! 站点冷启实测首包 ~8 s，所以这条路径永远在 `spawn_blocking` 里跑、有总预算、
//! 七个清单并发抓、整轮全失败时进 60 秒抑制窗口（别每进一次页面重撞一次冷启）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::Duration;

use serde_json::Value;
use ureq::Agent;

use super::{sig, Snapshot, Status};
use crate::linking::blog_local::normalize_kind;

pub const BASE: &str = "https://wmddd.online/content/";

/// 单次请求的总预算。站点冷启实测 ~8 s，12 s 留一点余量又不至于挂着不放。
pub const BUDGET: Duration = Duration::from_secs(12);

/// 整轮线上全失败之后，多久之内不再试。
const SUPPRESS: i64 = 60_000;
/// 下次允许联网的时刻（ms）。0 = 从没失败过。
static NEXT_TRY_AT: AtomicI64 = AtomicI64::new(0);

fn agent() -> Agent {
    // 三条 https 路共用一份底座：TLS provider 配错时 ureq 是 panic 而不是返回错误，
    // 而 panic 在 release 里等于整个应用消失（见 `crate::net`）。
    crate::net::agent(
        BUDGET,
        Duration::from_secs(3),
        "Anvil/0.1 (本机工作台，读本站公开的清单)",
    )
}

/// 抑制窗口：整轮全失败时记下"下次再试"的时刻。
pub fn suppressed_until(ms: i64) -> bool {
    ms < NEXT_TRY_AT.load(Ordering::Relaxed)
}

pub fn note_round_failed(ms: i64) {
    NEXT_TRY_AT.store(ms + SUPPRESS, Ordering::Relaxed);
}

fn clear_suppression() {
    NEXT_TRY_AT.store(0, Ordering::Relaxed);
}

/// 拉一个清单并归一化成与本地读取同样的形状 —— 两边同形状，`sig` 才可直接比。
pub fn fetch_normalized_with(agent: &Agent, kind: &str) -> Result<String, String> {
    let url = format!("{BASE}{kind}.json");
    let body = agent
        .get(&url)
        .call()
        .map_err(|e| format!("取不到 {url}：{e}"))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("读 {url} 失败：{e}"))?;

    let value: Value =
        serde_json::from_str(&body).map_err(|e| format!("{kind} 线上内容不是合法 JSON：{e}"))?;
    Ok(normalize_kind(kind, &value))
}

/// 并发抓所有类别。返回 `kind → 结果`；抓不到的那条**不在表里**，由调用方退回本机仓库。
pub fn fetch_all(kinds: &[(&str, &str)]) -> HashMap<String, Result<String, String>> {
    let a = agent();
    std::thread::scope(|s| {
        let handles: Vec<_> = kinds
            .iter()
            .map(|(kind, _)| {
                let a = &a;
                let kind = (*kind).to_owned();
                s.spawn(move || {
                    let out = fetch_normalized_with(a, &kind);
                    (kind, out)
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|h| h.join().ok())
            .collect()
    })
}

/// 这一类清单这次用哪份。纯函数，不碰网络也不碰数据库 —— 判的是"该说哪句实话"。
pub fn merge_one(mut snap: Snapshot, online: Option<&Result<String, String>>, ms: i64) -> Snapshot {
    match online {
        Some(Ok(payload)) => {
            snap.payload_json = payload.clone();
            snap.sig = sig::content_sig(payload);
            snap.status = Status::Ok;
            snap.detail = "线上".to_owned();
        }
        // 线上够不到：用本机仓库那份，但必须说清用的是旧东西以及为什么
        Some(Err(e)) if matches!(snap.status, Status::Ok) => {
            snap.detail = format!("本机仓库 · 线上没够到：{e}");
        }
        Some(Err(_)) => {
            // 本地也没有（仓库没这个文件）：两边都没有就直说两边都没有
            snap.detail = format!("本机仓库没有，线上也没够到：{}", first_clause(&snap.detail));
        }
        None => {
            snap.detail = if matches!(snap.status, Status::Ok) {
                "本机仓库 · 这次没联网".to_owned()
            } else {
                "本机仓库与线上都没有这一类清单".to_owned()
            };
        }
    }
    snap.fetched_at = ms;
    snap
}

/// 整轮：把线上结果盖到本机仓库读出来的快照上。
pub fn merge(local: Vec<Snapshot>, online: &HashMap<String, Result<String, String>>, ms: i64) -> Vec<Snapshot> {
    let any_ok = online.values().any(|r| r.is_ok());
    if any_ok {
        clear_suppression();
    } else if !online.is_empty() {
        note_round_failed(ms);
    }
    local
        .into_iter()
        .map(|s| {
            // 抑制窗口内 `fetch_all` 不会跑，online 是空表 —— 那也算"这次没联网"
            let key = s.kind.clone();
            merge_one(s, online.get(&key), ms)
        })
        .collect()
}

/// 界面上那一行不该塞进整段异常文本，取第一句就够。
fn first_clause(text: &str) -> String {
    text.split(['：', '，', '。'])
        .next()
        .unwrap_or(text)
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linking::{blog_local, empty_payload};
    use crate::time::FixedNow;

    fn local(kind: &str, payload: &str, status: Status) -> Snapshot {
        Snapshot {
            source: "blog".into(),
            kind: kind.into(),
            fetched_at: 1,
            sig: sig::content_sig(payload),
            status,
            detail: String::new(),
            payload_json: payload.to_owned(),
        }
    }

    const ONLINE: &str = r#"{"count":3,"items":[{"name":"a"},{"name":"b"},{"name":"c"}],"meta":{}}"#;
    const LOCAL: &str = r#"{"count":1,"items":[{"name":"old"}],"meta":{}}"#;

    #[test]
    fn online_content_wins_and_says_so() {
        let mut map = HashMap::new();
        map.insert("projects".to_owned(), Ok(ONLINE.to_owned()));
        let out = merge(
            vec![local("projects", LOCAL, Status::Ok)],
            &map,
            1_793_000_000_000,
        );
        assert_eq!(out[0].payload_json, ONLINE, "显示的内容必须是线上那份");
        assert_eq!(out[0].sig, sig::content_sig(ONLINE), "签名要跟着内容换，否则下次比对是假的");
        assert_eq!(out[0].detail, "线上");
        assert_eq!(out[0].status, Status::Ok);
    }

    /// 这条就是 M10b 存在的理由：本地 checkout 是旧的，线上一有新内容就必须盖掉它。
    #[test]
    fn a_stale_checkout_never_outvives_the_live_list() {
        let dir = tempfile::tempdir().unwrap();
        let content = dir.path().join("src").join("content");
        std::fs::create_dir_all(&content).unwrap();
        std::fs::write(content.join("projects.json"), r#"[{"name":"旧项目"}]"#).unwrap();

        let snaps = blog_local::read_repo(dir.path(), &FixedNow(1));
        let mut map = HashMap::new();
        map.insert("projects".to_owned(), Ok(ONLINE.to_owned()));
        let out = merge(snaps, &map, 2);
        let projects = out.iter().find(|s| s.kind == "projects").unwrap();
        let v: Value = serde_json::from_str(&projects.payload_json).unwrap();
        assert_eq!(v["count"], 3, "界面上必须是线上那 3 条，不是本地那 1 条");
    }

    #[test]
    fn an_unreachable_site_falls_back_and_names_the_reason() {
        let mut map = HashMap::new();
        map.insert("projects".to_owned(), Err("dns error: 没有达到要求的名称服务器".to_owned()));
        let out = merge(vec![local("projects", LOCAL, Status::Ok)], &map, 7);
        assert_eq!(out[0].payload_json, LOCAL, "够不到线上时退回本机那份");
        assert!(out[0].detail.starts_with("本机仓库 · 线上没够到"), "{}", out[0].detail);
        assert!(out[0].detail.contains("dns error"), "原因要能看见：{}", out[0].detail);
        assert_eq!(out[0].fetched_at, 7);
    }

    /// 本地也没有 + 线上也没有 ≠ "0 条"。那是契约问题，得说缺什么（PLAN §6.8 末）。
    #[test]
    fn missing_on_both_sides_says_both_are_missing() {
        let mut map = HashMap::new();
        map.insert("notes".to_owned(), Err("404".to_owned()));
        let mut snap = local("notes", &empty_payload(), Status::Missing);
        snap.detail = "notes 清单不在仓库里".to_owned();
        let out = merge(vec![snap], &map, 7);
        assert_eq!(out[0].status, Status::Missing);
        assert!(out[0].detail.contains("本机仓库没有"), "{}", out[0].detail);
        assert!(out[0].detail.contains("线上也没够到"), "{}", out[0].detail);
    }

    #[test]
    fn a_suppressed_round_falls_back_without_lying_about_the_source() {
        let out = merge(vec![local("projects", LOCAL, Status::Ok)], &HashMap::new(), 9);
        assert_eq!(out[0].payload_json, LOCAL);
        assert_eq!(out[0].detail, "本机仓库 · 这次没联网", "没去试和试了失败是两件事");
    }

    #[test]
    fn the_failure_window_suppresses_and_then_opens_again() {
        let t = 500_000;
        let mut all_bad = HashMap::new();
        all_bad.insert("projects".to_owned(), Err("timeout".to_owned()));
        merge(vec![local("projects", LOCAL, Status::Ok)], &all_bad, t);
        assert!(suppressed_until(t + 1_000), "整轮全失败之后马上再撞一次是没必要的");
        assert!(!suppressed_until(t + SUPPRESS + 1), "窗口过了就该再试");

        let mut one_ok = HashMap::new();
        one_ok.insert("projects".to_owned(), Ok(ONLINE.to_owned()));
        merge(vec![local("projects", LOCAL, Status::Ok)], &one_ok, t);
        assert!(!suppressed_until(t + 1), "抓通一次就该立刻解除抑制");
    }

    #[test]
    fn the_url_is_the_public_content_dir_plus_the_kind() {
        // 只验拼法，不发请求：真发的那条在下面，要联网才跑
        assert_eq!(format!("{BASE}projects.json"), "https://wmddd.online/content/projects.json");
    }

    /// 真联网的一条：对着你自己的站跑，证明「线上那份才是最新的」这条链是真的通。
    /// 默认忽略（CI 与别人的机器不该依赖外网），跑法：
    ///   ANVIL_LIVE_BLOG=1 cargo test --lib linking::blog_online -- --ignored
    #[test]
    #[ignore = "需要联网抓 wmddd.online"]
    fn the_live_site_answers_with_a_normalized_list() {
        use ureq::Agent;
        let a: Agent = agent();
        let payload = fetch_normalized_with(&a, "projects").expect("线上抓 projects");
        let v: Value = serde_json::from_str(&payload).unwrap();
        println!(
            "线上 projects：{} 条",
            v["count"].as_u64().unwrap_or(0)
        );
        assert!(v["items"].is_array(), "归一化后必须是 items/count/meta 那个形状");
    }
}
