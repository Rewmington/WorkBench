//! 博客的**线上**清单。M10b 起它是主源：站点由 GitHub Actions 每天 cron 重建，
//! 本地 checkout 不会同步 —— 读本地那份等于拿旧数据冒充「你现在所有的项目」（PLAN §6.8）。
//!
//! 站点冷启实测首包 ~8 s，所以这条路径永远在 `spawn_blocking` 里跑、有总预算、
//! 七个清单串行复用同一条连接（并发会被排队成握手超时，见 `fetch_all`）、
//! 整轮全失败时进 60 秒抑制窗口（别每进一次页面重撞一次冷启）。

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant};

use serde_json::Value;
use ureq::Agent;

use super::{sig, Snapshot, Status};
use crate::linking::blog_local::normalize_kind;

pub const BASE: &str = "https://wmddd.online/content/";

/// 线上那份清单的地址。文件名一律由调用方从 `blog_local::kinds()` 那张表给，**不在这里按 kind 现拼**：
/// `repos` 这一类的文件真名是 `github-repos.json`，拼成 `repos.json` 会落进站点的 SPA 回退，
/// 拿到的是一整页 404 HTML —— 于是仓库永远「线上没够到」，项目列表退回那份不会同步的旧 checkout。
fn content_url(file: &str) -> String {
    format!("{BASE}{file}")
}

/// 单次请求的总预算。站点在 Cloudflare 后面、内容是 GitHub Pages，冷启实测首包 ~8 s，
/// 而串行之后一轮的天花板由 `ROUND_BUDGET` 兜住，所以这里给足余量而不是卡到刚好。
pub const BUDGET: Duration = Duration::from_secs(20);

/// 一整轮（七个清单）的总预算。串行跑是为了复用同一条 TLS 连接（见 `fetch_all`），
/// 单个 20 s 的前提下最坏是 140 s —— 用完这一条就把剩下几类直接标成「没轮到」，
/// 好让界面早点退回本机仓库那份并说明原因，而不是让人对着转圈的横幅干等。
const ROUND_BUDGET: Duration = Duration::from_secs(45);

/// 整轮线上全失败之后，多久之内不再试。
const SUPPRESS: i64 = 60_000;

/// 抑制窗口的状态。**故意不做成全局 static**：那样并行跑的测试会互相清掉，
/// 而且"谁在被抑制"这件事会从函数签名上消失。命令层自己持有一份 `BLOG`。
#[derive(Debug, Default)]
pub struct Suppression(AtomicI64);

impl Suppression {
    pub const fn new() -> Self {
        Self(AtomicI64::new(0))
    }

    /// 现在该不该跳过联网。0 = 从没失败过。
    pub fn suppressed(&self, ms: i64) -> bool {
        ms < self.0.load(Ordering::Relaxed)
    }

    fn note_failed(&self, ms: i64) {
        self.0.store(ms + SUPPRESS, Ordering::Relaxed)
    }

    fn cleared(&self) {
        self.0.store(0, Ordering::Relaxed);
    }
}

/// 博客线上源的那一份。
pub static BLOG: Suppression = Suppression::new();

fn agent() -> Agent {
    // 三条 https 路共用一份底座：TLS provider 配错时 ureq 是 panic 而不是返回错误，
    // 而 panic 在 release 里等于整个应用消失（见 `crate::net`）。
    //
    // 连接超时给到 8 s：这个站实测过一次 `timeout: connect` —— 冷 DNS + Cloudflare + GitHub Pages
    // 的握手能排到 3 s 开外，而连接超时是**每次握手**的预算，收紧它只会把清单打成降级。
    crate::net::agent(
        BUDGET,
        Duration::from_secs(8),
        "Anvil/0.1 (本机工作台，读本站公开的清单)",
    )
}

/// 七个公开清单的上限。站点真实内容远小于这个数；超了不是「博客变大了」，是地址/
/// 对端已经不是我们以为的那个内容目录，不能让一次自动刷新把整个工作台拖去 OOM。
const MAX_JSON_BYTES: u64 = 8 * 1024 * 1024;

/// 拉一个清单并归一化成与本地读取同样的形状 —— 两边同形状，`sig` 才可直接比。
pub fn fetch_normalized_with(agent: &Agent, kind: &str, file: &str) -> Result<String, String> {
    let url = content_url(file);
    let mut resp = agent.get(&url).call().map_err(|e| format!("取不到 {url}：{e}"))?;
    let body = crate::net::read_capped_text(resp.body_mut().as_reader(), MAX_JSON_BYTES)
        .map_err(|e| format!("读 {url} 失败：{e}"))?;

    let value: Value =
        serde_json::from_str(&body).map_err(|e| format!("{kind} 线上内容不是合法 JSON：{e}"))?;
    Ok(normalize_kind(kind, &value))
}

/// 抓所有类别，**串行**。返回 `kind → 结果`；每一条都带自己的原因，调用方据此退回本机仓库。
///
/// 并发是错的：真机实测同一个 agent 开七条线程各自握手，`timeout_connect` 3 s 下有五条
/// 报 `timeout: connect` 退回本机仓库；换成串行复用同一条连接，整轮实测 4–37 s 且七个清单
/// 全通（慢的那几轮是站点侧握手本身就慢，`cargo test --lib linking::blog_online -- --ignored`
/// 会把每一类的条数和整轮耗时打出来）。这个站的内容在 GitHub Pages 上，
/// 同一时刻来的多条新连接会被排队 —— 并发只换来更多的握手超时，没换来速度。
///
/// 偶发的单次握手失败（串行连跑多轮撞见一次，报 `timeout: connect`，而 IPv4/IPv6 手动都秒连）
/// 不足以把那一类降级成旧数据，所以没够到的再补一趟。
pub fn fetch_all(kinds: &[(&str, &str)]) -> HashMap<String, Result<String, String>> {
    let a = agent();
    let start = Instant::now();
    let mut out: HashMap<String, Result<String, String>> = kinds
        .iter()
        .map(|(kind, file)| ((*kind).to_owned(), attempt(&a, &start, kind, file)))
        .collect();

    // 第二趟只补第一趟没够到的那几条。
    let missed: Vec<(&str, &str)> = kinds
        .iter()
        .filter(|(k, _)| matches!(out.get(*k), Some(Err(_))))
        .map(|(k, f)| (*k, *f))
        .collect();
    for (kind, file) in missed {
        // 预算没了就停：留着第一趟那个真原因，别拿「没轮到」盖掉「线上没够到：timeout: connect」。
        if start.elapsed() > ROUND_BUDGET {
            break;
        }
        out.insert(
            kind.to_owned(),
            fetch_normalized_with(&a, kind, file),
        );
    }
    out
}

/// 一次尝试。整轮预算用完了就不要再撞，把「没轮到」写进原因，界面上说的是实话而不是「线上没够到」。
fn attempt(agent: &Agent, start: &Instant, kind: &str, file: &str) -> Result<String, String> {
    if start.elapsed() > ROUND_BUDGET {
        return Err(format!("整轮预算用完，没轮到 {file}"));
    }
    fetch_normalized_with(agent, kind, file)
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
pub fn merge(
    local: Vec<Snapshot>,
    online: &HashMap<String, Result<String, String>>,
    ms: i64,
    gap: &Suppression,
) -> Vec<Snapshot> {
    if online.values().any(|r| r.is_ok()) {
        gap.cleared();
    } else if !online.is_empty() {
        gap.note_failed(ms);
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
        let gap = Suppression::new();
        let mut map = HashMap::new();
        map.insert("projects".to_owned(), Ok(ONLINE.to_owned()));
        let out = merge(
            vec![local("projects", LOCAL, Status::Ok)],
            &map,
            1_793_000_000_000,
            &gap,
        );
        assert_eq!(out[0].payload_json, ONLINE, "显示的内容必须是线上那份");
        assert_eq!(out[0].sig, sig::content_sig(ONLINE), "签名要跟着内容换，否则下次比对是假的");
        assert_eq!(out[0].detail, "线上");
        assert_eq!(out[0].status, Status::Ok);
    }

    /// 这条就是 M10b 存在的理由：本地 checkout 是旧的，线上一有新内容就必须盖掉它。
    #[test]
    fn a_stale_checkout_never_outvives_the_live_list() {
        let gap = Suppression::new();
        let dir = tempfile::tempdir().unwrap();
        let content = dir.path().join("src").join("content");
        std::fs::create_dir_all(&content).unwrap();
        std::fs::write(content.join("projects.json"), r#"[{"name":"旧项目"}]"#).unwrap();

        let snaps = blog_local::read_repo(dir.path(), &FixedNow(1));
        let mut map = HashMap::new();
        map.insert("projects".to_owned(), Ok(ONLINE.to_owned()));
        let out = merge(snaps, &map, 2, &gap);
        let projects = out.iter().find(|s| s.kind == "projects").unwrap();
        let v: Value = serde_json::from_str(&projects.payload_json).unwrap();
        assert_eq!(v["count"], 3, "界面上必须是线上那 3 条，不是本地那 1 条");
    }

    #[test]
    fn an_unreachable_site_falls_back_and_names_the_reason() {
        let gap = Suppression::new();
        let mut map = HashMap::new();
        map.insert("projects".to_owned(), Err("dns error: 没有达到要求的名称服务器".to_owned()));
        let out = merge(vec![local("projects", LOCAL, Status::Ok)], &map, 7, &gap);
        assert_eq!(out[0].payload_json, LOCAL, "够不到线上时退回本机那份");
        assert!(out[0].detail.starts_with("本机仓库 · 线上没够到"), "{}", out[0].detail);
        assert!(out[0].detail.contains("dns error"), "原因要能看见：{}", out[0].detail);
        assert_eq!(out[0].fetched_at, 7);
    }

    /// 本地也没有 + 线上也没有 ≠ "0 条"。那是契约问题，得说缺什么（PLAN §6.8 末）。
    #[test]
    fn missing_on_both_sides_says_both_are_missing() {
        let gap = Suppression::new();
        let mut map = HashMap::new();
        map.insert("notes".to_owned(), Err("404".to_owned()));
        let mut snap = local("notes", &empty_payload(), Status::Missing);
        snap.detail = "notes 清单不在仓库里".to_owned();
        let out = merge(vec![snap], &map, 7, &gap);
        assert_eq!(out[0].status, Status::Missing);
        assert!(out[0].detail.contains("本机仓库没有"), "{}", out[0].detail);
        assert!(out[0].detail.contains("线上也没够到"), "{}", out[0].detail);
    }

    #[test]
    fn a_suppressed_round_falls_back_without_lying_about_the_source() {
        let gap = Suppression::new();
        let out = merge(vec![local("projects", LOCAL, Status::Ok)], &HashMap::new(), 9, &gap);
        assert_eq!(out[0].payload_json, LOCAL);
        assert_eq!(out[0].detail, "本机仓库 · 这次没联网", "没去试和试了失败是两件事");
    }

    #[test]
    fn the_failure_window_suppresses_and_then_opens_again() {
        let gap = Suppression::new();
        let t = 500_000;
        let mut all_bad = HashMap::new();
        all_bad.insert("projects".to_owned(), Err("timeout".to_owned()));
        merge(vec![local("projects", LOCAL, Status::Ok)], &all_bad, t, &gap);
        assert!(gap.suppressed(t + 1_000), "整轮全失败之后马上再撞一次是没必要的");
        assert!(!gap.suppressed(t + SUPPRESS + 1), "窗口过了就该再试");

        let mut one_ok = HashMap::new();
        one_ok.insert("projects".to_owned(), Ok(ONLINE.to_owned()));
        merge(vec![local("projects", LOCAL, Status::Ok)], &one_ok, t, &gap);
        assert!(!gap.suppressed(t + 1), "抓通一次就该立刻解除抑制");
    }

    /// 线上文件名必须与本机那张表**同一个来源**。这条测试存在的全部理由：
    /// `repos` 的本机文件是 `github-repos.json`，早先线上按 kind 拼成 `repos.json`，
    /// 站点回退成一页 404 HTML —— 仓库这一类从此只能退回旧 checkout，而横幅会把它
    /// 说成「线上没够到」，看着像网络问题，其实是拼错了名字。
    #[test]
    fn every_online_url_comes_from_the_mapped_file_not_the_kind() {
        for (kind, file) in blog_local::kinds() {
            assert_eq!(
                content_url(file),
                format!("{BASE}{file}"),
                "{kind} 的地址必须落在 {file} 上，不是 {kind}.json"
            );
        }
        assert_eq!(
            content_url("github-repos.json"),
            "https://wmddd.online/content/github-repos.json",
            "仓库这一类问的是 github-repos.json"
        );
        assert!(
            !content_url("github-repos.json").ends_with("/repos.json"),
            "哪天有人改回按 kind 拼，这条会拦住"
        );
    }

    /// 真联网的一条：对着你自己的站跑一整轮，证明「线上那份才是最新的」这条链是真的通。
    /// 七个清单全问一遍 —— 早先只问 projects，正好放过了拼错名字的那一类。
    /// 顺带卡住串行这件事：整轮实测 ~5 s，超过 `ROUND_BUDGET` 就是退回并发的征兆。
    /// 默认忽略（CI 与别人的机器不该依赖外网），跑法：
    ///   cargo test --lib linking::blog_online -- --ignored
    #[test]
    #[ignore = "需要联网抓 wmddd.online"]
    fn the_live_site_answers_with_a_normalized_list() {
        let start = Instant::now();
        let online = fetch_all(blog_local::kinds());
        let took = start.elapsed();
        assert_eq!(online.len(), blog_local::kinds().len(), "每个 kind 都该有一条结果");
        for (kind, file) in blog_local::kinds() {
            let payload = online
                .get(*kind)
                .unwrap_or_else(|| panic!("{kind} 这一类压根没被问过"))
                .clone()
                .unwrap_or_else(|e| panic!("线上抓 {kind}（{file}）失败：{e}"));
            let v: Value = serde_json::from_str(&payload).unwrap();
            println!("线上 {kind}（{file}）：{} 条", v["count"].as_u64().unwrap_or(0));
            assert!(v["items"].is_array(), "归一化后必须是 items/count/meta 那个形状");
        }
        assert!(
            took < ROUND_BUDGET,
            "整轮 {took:?}，逼近 {ROUND_BUDGET:?} 的预算了"
        );
        println!("整轮 {took:?}");
    }
}
