//! 手机端同步的 PC 侧中转客户端：坚果云 `/dav/workbench-sync/`（PLAN-MOBILE §3.2、§3.6、§4）。
//!
//! 一次同步的形状（§3.2，照 `backup/webdav.rs` 这条仓库里唯一跑通过的 WebDAV 路写）：
//! `MKCOL`（首次，405 视为已存在）→ `PROPFIND` Depth:1 列目录 →
//! `GET` 别人那些 `device-*.enc`（**不含自己那份**）→ `merge::apply` 并进本地 →
//! 只有 `pending()>0` 才 `seal_with` + `PUT` 自己那份 → 成功后
//! `clear_dirty(那份快照自己的 sealed_at)`。
//!
//! 目录钉子写在类型层（§3.6 那张表），不靠自觉：
//! - `new` 只接受以 `/workbench-sync/` 结尾的 base，拒绝 `medicine-sync` 与 `workbench-backup`；
//! - `url()` 拒绝任何不以 `device-` 开头的文件名 —— 备份件（`anvil-*.abackup`）不该
//!   出现在这个目录里，真出现了也不该被我们读写；
//! - 独立目录的理由见 `backup/webdav.rs` 头部那段反例注释：药箱按 `device-` 前缀无差别
//!   挑文件合并，混进去就会被当成一台设备。
//!
//! 凭据复用备份那一组（§4）：邮箱来自 settings 的 `webdav.email`，应用密码来自
//! Windows 凭据管理器的 `Anvil/webdav`，KDF secret 是 `"{email}:{password}"` ——
//! 与 `backup::credentials()` 同一个构造，不新增第三把密码，**绝不读药箱存在它自己
//! 凭据管理器里的那把**。信封也复用 `backup::crypto`，但成本走 `seal_with(..., SYNC_M_KIB, ...)`
//! 那档（§4 实测补记：手机每次打开 app 要派生两次，64 MiB 单次就 632 ms）。

use std::io::Read;
use std::str::FromStr;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rusqlite::Connection;
use serde_json::json;
use ureq::http::{self, Method};
use ureq::{Agent, Body, RequestExt};

use crate::backup::propfind::{self, Entry};
use crate::backup::crypto;
use crate::error::{Error, Result};
use crate::repos::settings;
use crate::time::Now;

use super::merge;
use super::snapshot::{self, Snapshot};

pub const DEFAULT_BASE: &str = "https://dav.jianguoyun.com/dav/workbench-sync/";

/// 站点侧的目录名。改这个之前先想清楚 §3.6 那张表。
pub const REQUIRED_SEGMENT: &str = "/workbench-sync/";

/// 每设备一个文件的命名（§3.2）：`device-<device_id>.enc` = 备份信封包着 gzip 快照。
pub const FILE_PREFIX: &str = "device-";
pub const FILE_SUFFIX: &str = ".enc";

/// 一次最多读几份对端文件。真实使用是 1–2 台手机 + 1 台 PC；上限存在的理由是
/// 别对着一个陌生目录无界地拉文件（与 `dav_readonly::MAX_DOCS` 同款防护）。
pub const MAX_PEERS: usize = 8;

/// 单份对端文件的体积上限。个人量级的 gzip 快照是几十 KB，超这数说明地址填到了别处。
const MAX_BYTES: u64 = 32 * 1024 * 1024;

/// 本机要写的那一个文件名。
pub fn file_name(device: &str) -> String {
    format!("{FILE_PREFIX}{device}{FILE_SUFFIX}")
}

fn is_our_file(name: &str, device: &str) -> bool {
    name == file_name(device)
}

/// 把备份的 base 折成同步的 base：同一账号、父目录下的**兄弟目录**。
///
/// `from_env` 与 e2e 都从这里取值，所以「桩指哪儿、同步就跟到哪儿」只有一处定义。
/// 只换最后一段目录名；输入不合规时原样返回，由 `Remote::new` 的钉子去拒绝，不在这里猜。
pub fn sync_base_from_backup_base(backup_base: &str) -> String {
    let trimmed = backup_base.trim().trim_end_matches('/');
    match trimmed.rsplit_once('/') {
        Some((parent, _)) => format!("{parent}/workbench-sync/"),
        None => trimmed.to_owned(),
    }
}

/// ureq 3 的 Agent 只暴露标准动词，MKCOL/PROPFIND 得自己拼 `http::Request`。
fn verb(name: &str) -> Method {
    Method::from_bytes(name.as_bytes()).expect("内置的 WebDAV 动词常量")
}

fn agent() -> Agent {
    // TLS 与 PROPFIND 的门票都在 `crate::net::agent` 里配：三处 https 共用一份，
    // 少一处就会在发请求前 panic（见那个模块与 `backup/webdav.rs` 的同款注释）。
    crate::net::agent(
        Duration::from_secs(60),
        Duration::from_secs(5),
        "Anvil/0.1 (personal workbench sync relay)",
    )
}

/// 「请求没发出去」与「服务器回了非 2xx」必须分开：后者对 MKCOL 是正常结局
/// （405 已存在），不能当错误往上抛。照 `backup/webdav.rs` 的既有做法。
enum Sent {
    Build(Error),
    Http(ureq::Error),
    Ok(http::Response<Body>),
}

impl Sent {
    fn code(self: &Sent) -> Option<u16> {
        match self {
            Sent::Ok(resp) => Some(resp.status().as_u16()),
            Sent::Build(_) => None,
            // ureq::Error 是 non-exhaustive，其余变体都是「没拿到状态码」
            Sent::Http(ureq::Error::StatusCode(c)) => Some(*c),
            Sent::Http(_) => None,
        }
    }

    fn into_result(self, remote: &Remote) -> Result<http::Response<Body>> {
        match self {
            Sent::Ok(resp) => Ok(resp),
            Sent::Build(e) => Err(e),
            Sent::Http(e) => Err(remote.map_http(e)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Remote {
    base: String,
    user: String,
    pass: String,
}

impl Remote {
    /// `base` 必须以 `/workbench-sync/` 结尾。§3.6：写错目录不是小事，直接拒绝而不是警告。
    pub fn new(base: &str, user: &str, pass: &str) -> Result<Self> {
        let trimmed = base.trim();
        if !trimmed.ends_with(REQUIRED_SEGMENT) {
            return Err(Error::msg(format!(
                "同步目录必须是 {REQUIRED_SEGMENT} 结尾的独立目录，不能与备份或药箱共用"
            )));
        }
        if trimmed.contains("medicine-sync") {
            return Err(Error::msg("那是药箱的同步目录，工作台的手机同步必须写进自己的 workbench-sync"));
        }
        if trimmed.contains("workbench-backup") {
            return Err(Error::msg("那是每日备份的目录，手机同步的 device-*.enc 会把备份客户端的列目录搅浑"));
        }
        if user.trim().is_empty() || pass.trim().is_empty() {
            return Err(Error::msg("还没在设置里填坚果云邮箱与应用密码"));
        }
        Ok(Self {
            base: trimmed.to_owned(),
            user: user.trim().to_owned(),
            pass: pass.to_owned(),
        })
    }

    /// 本地桩（`tools/webdav-stub.mjs`）靠这个变量接管；release 构建永远走生产地址。
    ///
    /// 读的是备份那条同一个 `ANVIL_WEBDAV_BASE`（它是备份目录的 base），同步目录取它的
    /// 兄弟 —— 见 `sync_base_from_backup_base`。没设就用生产地址。
    pub fn from_env(user: &str, pass: &str) -> Result<Self> {
        let base = match std::env::var("ANVIL_WEBDAV_BASE") {
            Ok(v) if cfg!(debug_assertions) && !v.trim().is_empty() => {
                sync_base_from_backup_base(&v)
            }
            _ => DEFAULT_BASE.to_owned(),
        };
        Self::new(&base, user, pass)
    }

    fn url(&self, name: Option<&str>) -> Result<String> {
        match name {
            None => Ok(self.base.clone()),
            Some(n) if !n.starts_with(FILE_PREFIX) => Err(Error::msg(format!(
                "同步目录里只认 {FILE_PREFIX}*{FILE_SUFFIX} 这一种文件名，「{n}」不是"
            ))),
            Some(n) => Ok(format!("{}{}", self.base, propfind::percent_encode(n))),
        }
    }

    fn auth(&self) -> String {
        format!("Basic {}", B64.encode(format!("{}:{}", self.user, self.pass)))
    }

    fn send(
        &self,
        method: Method,
        name: Option<&str>,
        headers: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> Sent {
        let uri = match self.url(name) {
            Ok(v) => v,
            Err(e) => return Sent::Build(e),
        };
        let mut b = http::Request::builder()
            .method(method)
            .uri(uri)
            .header("Authorization", self.auth());
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        // 无 body 的动词也传空 Vec：MKCOL 与 PROPFIND 需要明确的 Content-Length。
        let req = match b.body(body.unwrap_or_default().to_vec()) {
            Ok(r) => r,
            Err(e) => return Sent::Build(Error::msg(format!("请求没拼起来：{e}"))),
        };
        match req.with_agent(agent()).run() {
            Ok(resp) => Sent::Ok(resp),
            Err(e) => Sent::Http(e),
        }
    }

    fn request(
        &self,
        method: Method,
        name: Option<&str>,
        headers: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> Result<http::Response<Body>> {
        self.send(method, name, headers, body).into_result(self)
    }

    /// 坚果云不允许在根下直接建文件，必须先有集合。
    /// 405（已存在）与 409（父级不存在，多半是路径填错了）都在这儿翻成中文。
    pub fn ensure_dir(&self) -> Result<()> {
        let sent = self.send(verb("MKCOL"), None, &[], None);
        match sent.code() {
            Some(405) | Some(201) | Some(204) => Ok(()),
            Some(409) => Err(Error::msg("上层目录不存在：检查坚果云里 workbench-sync 的完整路径")),
            _ => sent.into_result(self).map(|_| ()),
        }
    }

    /// 列目录，只返回 `device-*.enc` —— 备份件、目录、以及任何别的名字都不该被上层看见。
    pub fn list(&self) -> Result<Vec<Entry>> {
        let body = r#"<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:displayname/><D:getcontentlength/><D:resourcetype/></D:prop></D:propfind>"#;
        let xml = self
            .request(
                verb("PROPFIND"),
                None,
                &[
                    ("Depth", "1"),
                    ("Content-Type", "application/xml; charset=utf-8"),
                ],
                Some(body.as_bytes()),
            )?
            .body_mut()
            .read_to_string()
            .map_err(|e| Error::msg(format!("读目录列表失败：{e}")))?;

        // href 是站点绝对路径，比较时只取 base 的 path 部分 ——
        // 本地桩的 base 带 127.0.0.1 前缀，整串比一定对不上。
        let collection = http::Uri::from_str(&self.base)
            .map(|u| u.path().to_owned())
            .unwrap_or_else(|_| self.base.clone());
        let mut entries = propfind::parse_multistatus(&xml, &collection)?;
        entries.retain(|e| {
            !e.is_dir && e.name.starts_with(FILE_PREFIX) && e.name.ends_with(FILE_SUFFIX)
        });
        Ok(entries)
    }

    pub fn put(&self, name: &str, bytes: &[u8]) -> Result<()> {
        let status = self
            .request(
                Method::PUT,
                Some(name),
                &[("Content-Type", "application/octet-stream")],
                Some(bytes),
            )?
            .status()
            .as_u16();
        if matches!(status, 200 | 201 | 204) {
            Ok(())
        } else {
            Err(Error::msg(format!("上传返回了 {status}")))
        }
    }

    pub fn get(&self, name: &str) -> Result<Vec<u8>> {
        let mut resp = self.request(Method::GET, Some(name), &[], None)?;
        let mut buf = Vec::new();
        resp.body_mut()
            .as_reader()
            .take(MAX_BYTES + 1)
            .read_to_end(&mut buf)
            .map_err(|e| Error::msg(format!("下载 {name} 失败：{e}")))?;
        if buf.len() as u64 > MAX_BYTES {
            return Err(Error::msg(format!("{name} 体积异常（超过 32 MiB），已停止读取")));
        }
        Ok(buf)
    }

    /// 401/403 的文案沿用备份那条已经调好的句子 —— 它准确指出了最常见的两个错因。
    /// 坚果云另有两个已知坑（§7 第 2 条：PROPFIND 403、微信二次验证）都落在这句话里；
    /// 应对方式见 `super::schedule`：**每次开机只试一次，不静默重试到死**。
    fn map_http(&self, e: ureq::Error) -> Error {
        match e {
            ureq::Error::StatusCode(401) | ureq::Error::StatusCode(403) => Error::msg(
                "坚果云拒绝了凭据：检查账号是否填的是注册邮箱、密码是否用「第三方应用管理」里的应用密码（不是登录密码）",
            ),
            ureq::Error::StatusCode(404) => {
                Error::msg("同步目录还不存在：它可能从没同步过，或完整路径填错了")
            }
            ureq::Error::StatusCode(code) => Error::msg(format!("坚果云返回了 {code}")),
            other => Error::msg(format!("坚果云请求失败：{other}")),
        }
    }
}

/// 一次同步的结果。与 `backup::Outcome` 同款约定：**失败折成字段，不折成 panic**，
/// 界面只读这里（`ok=false` + 一句人话），不弹窗（PLAN-MOBILE §6 M11b 验收）。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    /// 这次同步的开始时刻（毫秒）。成功时它就是界面那句「上次同步于」的来源。
    pub at: i64,
    pub ok: bool,
    /// 凭据没配 = 可执行的一句话，**不是失败记录**：不写 sync.state，不进任何流水。
    pub skipped: bool,
    pub error: Option<String>,
    /// 对端那份并进本地的行数（含墓碑）。
    pub applied: usize,
    /// 本地版本更新或打平、原样保留的行数。
    pub kept: usize,
    /// 这次有没有 PUT 自己那份。
    pub uploaded: bool,
    /// `pending()==0` 的省配额跳过（§3.4；M11b 验收点名要测试钉住的那条）。
    pub put_skipped: bool,
    /// 同步之后的待上传数，只来自 `merge::pending`。
    pub pending_after: i64,
    /// 远端别人的设备 id（文件名去掉 `device-` 前缀与 `.enc` 后缀；不含本机）。
    pub peers: Vec<String>,
    /// 发出去那份快照自己的时间戳；`clear_dirty` 用的就是它，不是墙钟（§3.3 第 2 条）。
    pub sealed_at: Option<i64>,
}

/// 一台机器上「上次同步于 X」这类状态的唯一存放处（settings 表，见 `KEY_SYNC_STATE`）。
/// `commands::sync` 与 `sync::schedule` 都从这里读，不另立第二份状态。
pub fn stored_state(conn: &Connection) -> Result<serde_json::Value> {
    Ok(settings::get_json(conn, settings::KEY_SYNC_STATE)?.unwrap_or_else(|| json!({})))
}

/// 生产入口：凭据复用备份那一组（`backup::credentials()` 同一个构造）。
/// 没配齐就跳过，且**不写成失败记录**（§5.5：那是可执行的一句话，由界面直接说「先去填」）。
pub fn run(conn: &Connection, device: &str, now: &dyn Now) -> Outcome {
    let ms = now.ms();
    let Some((email, password)) = webdav_credentials(conn) else {
        return Outcome {
            at: ms,
            ok: false,
            skipped: true,
            error: Some("还没配好坚果云凭据，同步先不跑".to_owned()),
            applied: 0,
            kept: 0,
            uploaded: false,
            put_skipped: false,
            pending_after: merge::pending(conn).unwrap_or(0),
            peers: Vec::new(),
            sealed_at: None,
        };
    };
    match Remote::from_env(&email, &password) {
        Ok(remote) => run_with(
            conn,
            device,
            &format!("{email}:{password}"),
            &remote,
            now,
        ),
        Err(e) => Outcome {
            at: ms,
            ok: false,
            skipped: false,
            error: Some(e.to_string()),
            applied: 0,
            kept: 0,
            uploaded: false,
            put_skipped: false,
            pending_after: merge::pending(conn).unwrap_or(0),
            peers: Vec::new(),
            sealed_at: None,
        },
    }
}

/// 备份那组凭据（邮箱 + `Anvil/webdav` 那条应用密码）。走 `backup::credentials()`，
/// 不自创第二份构造 —— 它同时是「绝不读药箱那把」的钉子：那边是 `medicine-webdav` 账号。
fn webdav_credentials(conn: &Connection) -> Option<(String, String)> {
    crate::backup::credentials(conn)
}

/// 一次完整同步的纯逻辑：远端与密钥由调用方给，所以 `tests/sync_relay_e2e.rs`
/// 能对着本地桩跑通整条链，不必碰 Windows 凭据管理器（PLAN §6.3「不经 Tauri、直接可测」）。
pub fn run_with(
    conn: &Connection,
    device: &str,
    secret: &str,
    remote: &Remote,
    now: &dyn Now,
) -> Outcome {
    let start = now.ms();
    let attempt = || -> Result<Outcome> {
        remote.ensure_dir()?;

        // GET 的是**别人**那些 device-*.enc：自己那份不读回来合并（自己的行本来就在库里）。
        let entries = remote.list()?;
        let mut applied = 0usize;
        let mut kept = 0usize;
        let mut peers = Vec::new();
        for entry in entries.iter().take(MAX_PEERS + 1) {
            if is_our_file(&entry.name, device) {
                continue;
            }
            let name = entry.name.clone();
            peers.push(peer_id(&name));
            let blob = remote.get(&name).map_err(|e| wrap_peer(&name, e))?;
            let plain = crypto::open(secret, &blob).map_err(|e| wrap_peer(&name, e))?;
            let snap = Snapshot::from_bytes(&plain).map_err(|e| wrap_peer(&name, e))?;
            let report = merge::apply(conn, &snap, merge::PC_ACCEPTS).map_err(|e| wrap_peer(&name, e))?;
            applied += report.applied;
            kept += report.kept;
        }
        if peers.len() > MAX_PEERS {
            peers.truncate(MAX_PEERS);
        }

        // 「本地没有写入就跳过 PUT」的省配额判断（§3.4）。计数只有 merge::pending 一个来源。
        let pending = merge::pending(conn)?;
        let mut uploaded = false;
        let mut sealed_at = None;
        if pending > 0 {
            let stamp = now.ms();
            let snap = snapshot::dump(conn, snapshot::PC_TABLES, device, stamp)?;
            let plain = snap.to_bytes()?;
            // 同步那档成本（16 MiB），不是日备那把 64 MiB —— §4 实测补记，手机开这份要用。
            let blob = crypto::seal_with(secret, &plain, crypto::SYNC_M_KIB, crypto::SYNC_T_COST, crypto::SYNC_P_COST)?;
            remote.put(&file_name(device), &blob)?;
            // 清的是**那份快照自己的** sealed_at：序列化之后、PUT 响应回来之前新写的行
            // 必须还留在 outbox 里（§3.3 第 2 条），所以这里绝不能用 now。
            merge::clear_dirty(conn, snap.sealed_at)?;
            uploaded = true;
            sealed_at = Some(snap.sealed_at);
        }

        Ok(Outcome {
            at: start,
            ok: true,
            skipped: false,
            error: None,
            applied,
            kept,
            uploaded,
            put_skipped: !uploaded,
            pending_after: merge::pending(conn)?,
            peers,
            sealed_at,
        })
    };

    let outcome = match attempt() {
        Ok(done) => done,
        Err(e) => Outcome {
            at: start,
            ok: false,
            skipped: false,
            error: Some(e.to_string()),
            applied: 0,
            kept: 0,
            uploaded: false,
            put_skipped: false,
            // 计数照走 merge::pending：远端断了不代表待上传数变了（§5.5 那半句「N 条待上传」仍可验证）。
            pending_after: merge::pending(conn).unwrap_or(0),
            peers: Vec::new(),
            sealed_at: None,
        },
    };
    record(conn, device, &outcome);
    outcome
}

/// `device-<id>.enc` → `<id>`。钉子保证了前缀后缀，这里只做剥壳。
fn peer_id(name: &str) -> String {
    name.trim_start_matches(FILE_PREFIX)
        .trim_end_matches(FILE_SUFFIX)
        .to_owned()
}

/// 对端文件的失败要点名是哪一台机器：「读远端失败」和「读远端 device-phone-a.enc 失败：…」
/// 是两种可执行程度完全不同的句子（§7 第 2 条：撞上了就说人话）。
fn wrap_peer(name: &str, e: Error) -> Error {
    Error::msg(format!("读远端 {name} 失败：{e}"))
}

/// 把这次结果并进 settings 的那份状态。成功刷新 `lastSyncAt`（界面那句时间的来源）；
/// 失败刷新 `lastError/lastErrorAt`，**不抹掉上一次的成功时间** ——
/// 界面上「只在失败比成功新时才显示那行」沿用备份页 `staleFailure` 的同一判据。
/// 没配凭据的 skipped 走不到这里（`run` 直接返回），所以不会产生失败记录。
fn record(conn: &Connection, device: &str, out: &Outcome) {
    let prev = stored_state(conn).unwrap_or_else(|_| json!({}));
    let prev_ms = |key: &str| prev.get(key).and_then(serde_json::Value::as_i64);
    let peers: Vec<serde_json::Value> = out.peers.iter().map(|p| json!(p)).collect();
    let prev_peers = prev.get("peers").cloned().unwrap_or_else(|| json!([]));
    let next = json!({
        "lastSyncAt": if out.ok { Some(out.at) } else { prev_ms("lastSyncAt") },
        "lastError": if out.ok { None } else { out.error.as_deref() },
        "lastErrorAt": if out.ok { prev_ms("lastErrorAt") } else { Some(out.at) },
        "peers": if out.ok { serde_json::Value::Array(peers) } else { prev_peers },
    });
    // 记不进 settings 不值得把成功的同步说成失败：那份数据已经传上去了。
    if let Err(e) = settings::set_json(conn, settings::KEY_SYNC_STATE, &next, out.at, device) {
        log::warn!("同步状态没能写进设置：{e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::time::FixedNow;

    fn conn() -> Connection {
        let mut c = db::open_in_memory().unwrap();
        db::migrations::migrate(&mut c).unwrap();
        c
    }

    /// §3.6 钉子之一：`Remote::new` 只认 `/workbench-sync/`，别的目录一个都不进。
    #[test]
    fn only_the_sync_directory_is_accepted() {
        let ok = Remote::new(
            "https://dav.jianguoyun.com/dav/workbench-sync/",
            "me@example.com",
            "app-pass",
        );
        assert!(ok.is_ok(), "{ok:?}");

        for bad in [
            // 药箱的同步目录
            "https://dav.jianguoyun.com/dav/medicine-sync/",
            // 每日备份的目录
            "https://dav.jianguoyun.com/dav/workbench-backup/",
            // 根目录 / 缺尾斜杠 / 空串
            "https://dav.jianguoyun.com/dav/",
            "https://dav.jianguoyun.com/dav/workbench-sync",
            "  ",
        ] {
            assert!(
                Remote::new(bad, "me@example.com", "app-pass").is_err(),
                "{bad} 不该被接受"
            );
        }
        // 凭据缺任一条都不构造得出 Remote
        assert!(Remote::new(DEFAULT_BASE, "", "p").is_err());
        assert!(Remote::new(DEFAULT_BASE, "me@x.com", " ").is_err());
    }

    /// 钉子之二：文件名不叫 `device-*` 就连请求都不该拼出来（备份件混进来也不碰）。
    #[test]
    fn names_that_are_not_device_files_are_refused_before_the_socket() {
        // base 指这台机器上必然没人监听的端口：拒绝如果发生在拼 URL 之前，
        // 报错文案就来自钉子本身，而不是连接失败。
        let r = Remote::new("http://127.0.0.1:1/dav/workbench-sync/", "me@x.com", "p").unwrap();
        for name in ["anvil-20261030T000000Z-00000000.abackup", "notes.txt", ""] {
            let err = r.get(name).unwrap_err().to_string();
            assert!(err.contains("device-"), "{name} 应被文件名钉子拒绝，实际：{err}");
            let err = r.put(name, b"x").unwrap_err().to_string();
            assert!(err.contains("device-"), "{name} 应被文件名钉子拒绝，实际：{err}");
        }
    }

    /// 桩与生产都靠这一个函数把备份 base 折成同步 base；钉子保证了折错就构造失败。
    #[test]
    fn the_backup_base_folds_to_its_sibling_sync_directory() {
        assert_eq!(
            sync_base_from_backup_base("https://dav.jianguoyun.com/dav/workbench-backup/"),
            "https://dav.jianguoyun.com/dav/workbench-sync/"
        );
        assert_eq!(
            sync_base_from_backup_base("http://127.0.0.1:8791/dav/workbench-backup"),
            "http://127.0.0.1:8791/dav/workbench-sync/"
        );
        // 折出来的 base 必须能过 new 自己的钉子
        let folded = sync_base_from_backup_base(DEFAULT_BASE);
        assert!(Remote::new(&folded, "me@x.com", "p").is_ok());
    }

    #[test]
    fn our_own_file_name_is_the_device_prefixed_envelope() {
        assert_eq!(file_name("pc-0196"), "device-pc-0196.enc");
        assert!(is_our_file("device-pc-0196.enc", "pc-0196"));
        assert!(!is_our_file("device-phone-a.enc", "pc-0196"));
        assert_eq!(peer_id("device-phone-a.enc"), "phone-a");
    }

    /// 断网（这里用「连一个没人监听的端口」模拟）的结局必须是 `ok=false` + 一句人话。
    /// 不许 panic —— Cargo.toml 里不写 panic=abort 的那段注释就是为这一刻。
    #[test]
    fn offline_leaves_a_sentence_and_keeps_the_pending_count() {
        use crate::model::TodoDraft;
        use crate::repos::todos;

        let c = conn();
        todos::create(
            &c,
            &FixedNow(1_700_000_000_000),
            "pc-a",
            TodoDraft {
                title: "断网也该记得我还没传".into(),
                ..Default::default()
            },
        )
        .unwrap();

        let r = Remote::new("http://127.0.0.1:1/dav/workbench-sync/", "me@x.com", "p").unwrap();
        let out = run_with(&c, "pc-a", "me@x.com:p", &r, &FixedNow(1_700_000_000_000));
        assert!(!out.ok);
        assert!(!out.skipped);
        let msg = out.error.clone().unwrap_or_default();
        assert!(!msg.is_empty() && !msg.contains('\n'), "状态必须是一行话：{msg:?}");
        assert_eq!(out.pending_after, 1, "远端断了不改本机欠账");

        // 失败也要落进 sync.state：错误单独记着，而「上次同步于」没有成功可写就保持空
        let st = stored_state(&c).unwrap();
        assert!(st.get("lastSyncAt").and_then(serde_json::Value::as_i64).is_none());
        let saved = st.get("lastError").and_then(|v| v.as_str()).unwrap_or_default().to_owned();
        assert_eq!(saved, msg, "state 里那句与 Outcome 里必须是同一句人话");
    }

    /// skipped（凭据没配）不写失败记录 —— 界面那句是可执行的话，不是流水。
    /// 这里不碰真凭据管理器：直接调 run 但库里没填邮箱，credentials() 一定拿不到。
    #[test]
    fn unconfigured_is_skipped_and_records_nothing() {
        let c = conn();
        let out = run(&c, "pc-a", &FixedNow(1_700_000_000_000));
        assert!(!out.ok);
        assert!(out.skipped);
        assert!(out.error.unwrap().contains("还没配好"));
        assert_eq!(stored_state(&c).unwrap(), json!({}), "跳过不该留下任何状态");
    }
}
