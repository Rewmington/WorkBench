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

/// PROPFIND 清单的上限；同步目录只有几份 `device-*.enc`，4 MiB 已经宽到不像个人用量。
const MAX_XML_BYTES: u64 = 4 * 1024 * 1024;

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

/// 这一轮要读的对端信封，与被窗口裁掉的那些。
///
/// 分成两份返回是为了让"裁掉了谁"能被断言 —— 静默丢掉别人已发布的改动是这个仓库最不能
/// 接受的事（`run_with` 拿后者去写日志，界面那头的点名是下一步）。
/// 排序键是 `getlastmodified`，不是文件名：坚果云按名字排，而每次还原都会换一个
/// 新的 `pc-<uuid>`，旧信封又没有删除路径 —— 按名字取窗口等于让排在后面的设备常年落选。
/// 不知道时间的排最后：那是"读不出这个属性"，不是"这份很旧"。
fn peer_window<'a>(entries: &'a [Entry], device: &str) -> (Vec<&'a Entry>, Vec<&'a Entry>) {
    let mut theirs: Vec<&Entry> = entries
        .iter()
        .filter(|e| !is_our_file(&e.name, device))
        .collect();
    theirs.sort_by_key(|e| std::cmp::Reverse(e.modified));
    let chosen = theirs.iter().take(MAX_PEERS).copied().collect();
    let dropped = theirs.iter().skip(MAX_PEERS).copied().collect();
    (chosen, dropped)
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
        let body = r#"<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:displayname/><D:getcontentlength/><D:getlastmodified/><D:resourcetype/></D:prop></D:propfind>"#;
        let xml = crate::net::read_capped_text(
            self.request(
                verb("PROPFIND"),
                None,
                &[
                    ("Depth", "1"),
                    ("Content-Type", "application/xml; charset=utf-8"),
                ],
                Some(body.as_bytes()),
            )?
            .body_mut()
            .as_reader(),
            MAX_XML_BYTES,
        )
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

    /// 删掉一份对端信封。**只服务「清理读不懂的对端文件」那一个入口**：
    /// 那是别人设备上已经发布出来的数据，跨设备不可逆，所以调用方必须先把它列出来、由人确认，
    /// 不允许任何自动路径调到这里。404 算成功（本来就不在）。
    pub fn delete(&self, name: &str) -> Result<()> {
        let sent = self.send(verb("DELETE"), Some(name), &[], None);
        if matches!(sent.code(), Some(404)) {
            return Ok(());
        }
        sent.into_result(self).map(|_| ())
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
    /// 远端设备 listing（文件名去掉 `device-` 前缀与 `.enc` 后缀；不含本机）。
    pub peers: Vec<String>,
    /// `peers` 里这次**读不懂**的那几个（旧版本写的、被改坏的、传输截断的）。
    /// 它们仍算在线，只是这一轮没并进来它们的改动 —— 界面要能区分这两件事。
    pub unreadable: Vec<String>,
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
///
/// 但**凭据管理器读不出来不是没配**：那种记一条失败（`failed`）。折成 skipped 的代价是
/// 界面从此只说「先去填一个本来就填对了的密码」，而按钮按 `configured` 禁用之后连试都试不了
/// —— P2-7 与 `backup::run` 同一个处置，两端都不许把故障冒充成缺席。
pub fn run(conn: &Connection, device: &str, now: &dyn Now) -> Outcome {
    let ms = now.ms();
    let (email, password) = match webdav_credentials(conn) {
        Ok(Some(pair)) => pair,
        Ok(None) => {
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
                unreadable: Vec::new(),
                sealed_at: None,
            };
        }
        Err(e) => return failed(conn, device, ms, e.to_string()),
    };
    match Remote::from_env(&email, &password) {
        Ok(remote) => run_with(
            conn,
            device,
            &format!("{email}:{password}"),
            &remote,
            now,
        ),
        Err(e) => failed(conn, device, ms, e.to_string()),
    }
}

/// 一次根本没跑起来的同步（凭据用不了、坚果云地址配错）。
///
/// 必须记进 sync.state 才算说过话：`sync_now` 的注释写着「run 已把结果记进状态」，
/// 而这两条早退原先只把 Outcome 还给调用方 —— 手动点那次界面上看得到，
/// 开机与定时那次什么都不留，故障就成了「今天没同步，也没人解释」。
fn failed(conn: &Connection, device: &str, ms: i64, error: String) -> Outcome {
    let out = Outcome {
        at: ms,
        ok: false,
        skipped: false,
        error: Some(error),
        applied: 0,
        kept: 0,
        uploaded: false,
        put_skipped: false,
        // 计数照走 merge::pending：没跑起来不代表待上传数变了（§5.5 那半句「N 条待上传」仍可验证）。
        pending_after: merge::pending(conn).unwrap_or(0),
        peers: Vec::new(),
        unreadable: Vec::new(),
        sealed_at: None,
    };
    record(conn, device, &out);
    out
}

/// 备份那组凭据（邮箱 + `Anvil/webdav` 那条应用密码）。走 `backup::credentials()`，
/// 不自创第二份构造 —— 它同时是「绝不读药箱那把」的钉子：那边是 `medicine-webdav` 账号。
/// `Err` = 管理器用不了，`Ok(None)` = 真没配 —— 这一区分不许在这一层折掉。
fn webdav_credentials(conn: &Connection) -> Result<Option<(String, String)>> {
    crate::backup::credentials(conn)
}

/// 本机有没有任何可同步的行。PC 只要本地有数据就发布全量快照：
/// 只拉取过的机器 lastSyncAt 可能晚于课表更新时间，按时间判断会永远漏发。
fn has_local_rows(conn: &Connection) -> Result<bool> {
    let any: i64 = conn.query_row(
        "SELECT (EXISTS(SELECT 1 FROM todos) OR EXISTS(SELECT 1 FROM classes))",
        [],
        |r| r.get(0),
    )?;
    Ok(any != 0)
}

/// 一次「清理读不懂的对端文件」的结果。删成什么样就报成什么样：
/// 一份没删成不该把已经删掉的隐掉，也不该因为一份失败就停住剩下的。
#[derive(Debug, Default, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cleaned {
    pub deleted: Vec<String>,
    pub failed: Vec<FailedPeer>,
    /** 验过之后**故意没删**的那些，连同理由（`cleanable_now`）。界面要把它和"删失败"分开说 */
    pub skipped: Vec<SkippedPeer>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedPeer {
    pub name: String,
    pub why: String,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FailedPeer {
    pub name: String,
    pub why: String,
}

/// 删之前重新验一遍「这一份到底为什么读不懂」，验不过就不删并说明理由。
///
/// 名单是**上一轮**的结论，而删除跨设备不可逆，所以这里不看结论、看此刻的事实：
///
/// - **解不开**：很可能是另一组凭据写的（换过应用密码的那台设备）。那正是它自己唯一的
///   云端历史，"销毁别人的数据"不在这个按钮的授权范围里 —— 用户点的是"清理垃圾"，
///   不是"删掉那台设备"。
/// - **现在读得懂了**：上一轮的结论过期了（比如对方重发了一份），更不该删。
/// - **读都读不到**：网络/404，不动。
///
/// 只有「能解开、内容却读不懂」才是可证明的垃圾：对我们它就是一堆认不出的字节。
fn cleanable_now(remote: &Remote, secret: &str, name: &str) -> std::result::Result<(), String> {
    let blob = remote.get(name).map_err(|e| format!("读不到这份文件，所以不动它：{e}"))?;
    match crypto::open(secret, &blob) {
        Err(e) => Err(format!("这台机器解不开它（可能是别的凭据或账号写的），不删：{e}")),
        Ok(plain) => match Snapshot::from_bytes(&plain) {
            // 解得开、内容读不懂 = 可证明的垃圾，允许删
            Err(_) => Ok(()),
            // 现在连内容都读得懂了 = 上一轮的结论过期了（对方可能重发过一份）
            Ok(_) => Err("这一份现在读得懂了，名单是上一轮的结论，不删".to_owned()),
        },
    }
}

/// 哪些信封允许删 —— 「清理读不懂的对端文件」的判据。
///
/// 抽成纯函数是因为这一步删的是**别人发布的数据**，跨设备不可逆，每条拒绝理由
/// 都得能在单测里钉住，而不是等对着桩跑的 e2e 去撞。
/// 只认这两个来源的交集：`unreadable` 是这一轮本机真读不懂的那些 id（写在 `sync.state` 里，
/// 不是"看起来不像合法文件"这种猜测），`listing` 是远端此刻真有这些文件。
/// 自己的那份永远不删 —— 那是本机唯一的云端历史。
fn clean_targets(device: &str, listing: &[Entry], unreadable: &[String]) -> Vec<String> {
    unreadable
        .iter()
        .filter(|id| id.as_str() != device)
        .map(|id| file_name(id))
        .filter(|name| listing.iter().any(|e| &e.name == name))
        .collect()
}

/// 对着真远端跑一遍清理。凭据与 `remote` 由调用方给，与 `run_with` 同一形状 ——
/// 这样它能对着本地桩跑完，不必碰 Windows 凭据管理器。
pub fn clean_unreadable(
    conn: &Connection,
    device: &str,
    secret: &str,
    remote: &Remote,
    now: &dyn Now,
) -> Result<Cleaned> {
    let state = stored_state(conn)?;
    let unreadable: Vec<String> = state
        .get("unreadablePeers")
        .and_then(serde_json::Value::as_array)
        .map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_owned)).collect())
        .unwrap_or_default();
    let listing = remote.list()?;
    let targets = clean_targets(device, &listing, &unreadable);

    let mut out = Cleaned::default();
    for name in targets {
        if let Err(why) = cleanable_now(remote, secret, &name) {
            log::info!("清理时跳过 {name}：{why}");
            out.skipped.push(SkippedPeer { name, why });
            continue;
        }
        match remote.delete(&name) {
            Ok(()) => {
                log::warn!("按用户确认删掉了对端信封 {name}（能解开，内容读不懂）");
                out.deleted.push(name);
            }
            Err(e) => {
                log::warn!("删不掉 {name}：{e}");
                out.failed.push(FailedPeer { name, why: e.to_string() });
            }
        }
    }

    if !out.deleted.is_empty() {
        // 文件已经没了，名单里还挂着它们就一直说「这几台读不懂」—— 那是过期结论，不是事实。
        // 只摘掉删成的那几台；下一轮同步会按实际 listing 重写整份名单。
        let gone: Vec<String> = out.deleted.iter().map(|n| peer_id(n)).collect();
        let mut next = state;
        if let Some(arr) = next
            .get_mut("unreadablePeers")
            .and_then(serde_json::Value::as_array_mut)
        {
            arr.retain(|v| !gone.iter().any(|g| v.as_str() == Some(g.as_str())));
        }
        if let Err(e) = settings::set_json(conn, settings::KEY_SYNC_STATE, &next, now.ms(), device) {
            log::warn!("清理后同步状态没能写回设置：{e}");
        }
    }
    Ok(out)
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
        // 先把自己摘掉，窗口才真的是「8 个对端」。从前是取 `MAX_PEERS + 1` 条再在循环里
        // `continue` 掉自己 —— 自己那份落在窗口里时，它占掉的是一个真对端的位子。
        // 窗口装不下所有对端时，"谁落在外面"从前只由 listing 顺序决定（坚果云按名字排，
        // `device-pc-*` 恒在 `device-phone-*` 前）。而还原一次就换一个新的 `pc-<uuid>`，
        // 旧信封在生产代码里没有删除路径 —— 攒够 8 份，排在后面的那台会被静默裁掉。
        // 现在按"最近写过"取，不知道时间的排最后（那是"不知道"，不是"很旧"）。
        let (theirs, dropped) = peer_window(&entries, device);
        // 排序靠的是 `getlastmodified`，而这一列**从没在真坚果云上实测过**（本地桩发的是 GMT 那一种）。
        // 服务器要是不给，`peer_window` 会安静地退回按 listing 顺序 —— 那正是这条改动想消灭的旧行为，
        // 所以至少要说响一次：不然"按最新挑"这件事只在测试里成立。
        if !theirs.is_empty() && theirs.iter().all(|e| e.modified.is_none()) {
            log::warn!(
                "这一轮所有对端信封都没带 getlastmodified（或格式读不懂），窗口退回按 listing 顺序取前 {MAX_PEERS} 份"
            );
        }
        // 合并前先看本机有没有本地数据：合并进来的对端行会让「空库」看起来有数据，不能算。
        let local_rows = has_local_rows(conn)?;
        let mut applied = 0usize;
        let mut kept = 0usize;
        let mut peers = Vec::new();
        // 一个读不懂的对端文件不该把整轮同步带走：跳过它、把其余的并进来、自己那份照发。
        // 但也不能装作什么都没发生 —— 那台设备的新改动这次确实没并进来，所以单列一条
        // 「谁读不懂」交给界面（`unreadable`），而不是折进 lastError：那不算失败。
        let mut unreadable: Vec<String> = Vec::new();
        if !dropped.is_empty() {
            let names: Vec<&str> = dropped.iter().map(|e| e.name.as_str()).collect();
            log::warn!(
                "这一轮只读到最近写的 {} 份信封，没读到：{}（对端设备太多，或还原换 id 后旧信封没清）",
                MAX_PEERS,
                names.join("、")
            );
        }
        for entry in theirs {
            let name = entry.name.clone();
            let id = peer_id(&name);
            // listing 里有没有它，取决于远端有没有这个文件，跟读不读得懂无关
            peers.push(id.clone());
            let merged = (|| -> Result<()> {
                let blob = remote.get(&name)?;
                let plain = crypto::open(secret, &blob)?;
                let snap = Snapshot::from_bytes(&plain)?;
                let report = merge::apply(conn, &snap, merge::PC_ACCEPTS)?;
                applied += report.applied;
                kept += report.kept;
                Ok(())
            })();
            if let Err(e) = merged {
                log::warn!("跳过读不懂的对端文件 {name}：{e}");
                unreadable.push(id);
            }
        }

        // PC 只要本地有数据就发布全量快照；只有真正的空库才跳过。
        // 课表没有 dirty 列，只拉取过的机器 lastSyncAt 又可能晚于课表更新时间，
        // 任何基于「有没有变化」的优化都会漏发（2026-10-31 真机复现），所以不做。
        let pending = merge::pending(conn)?;
        let should_publish = pending > 0 || local_rows;
        let mut uploaded = false;
        let mut sealed_at = None;
        if should_publish {
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
            unreadable,
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
            unreadable: Vec::new(),
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

/// 把这次结果并进 settings 的那份状态。成功刷新 `lastSyncAt`（界面那句时间的来源）；
/// 失败刷新 `lastError/lastErrorAt`，**不抹掉上一次的成功时间** ——
/// 界面上「只在失败比成功新时才显示那行」沿用备份页 `staleFailure` 的同一判据。
/// 没配凭据的 skipped 走不到这里（`run` 直接返回），所以不会产生失败记录；
/// 凭据用不了那条走 `failed`，会记 —— 它不是「还没配」，是配了却读不出来。
fn record(conn: &Connection, device: &str, out: &Outcome) {
    let prev = stored_state(conn).unwrap_or_else(|_| json!({}));
    let prev_ms = |key: &str| prev.get(key).and_then(serde_json::Value::as_i64);
    let peers: Vec<serde_json::Value> = out.peers.iter().map(|p| json!(p)).collect();
    let unreadable: Vec<serde_json::Value> = out.unreadable.iter().map(|p| json!(p)).collect();
    let prev_peers = prev.get("peers").cloned().unwrap_or_else(|| json!([]));
    let prev_unreadable = prev
        .get("unreadablePeers")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let next = json!({
        "lastSyncAt": if out.ok { Some(out.at) } else { prev_ms("lastSyncAt") },
        "lastError": if out.ok { None } else { out.error.as_deref() },
        "lastErrorAt": if out.ok { prev_ms("lastErrorAt") } else { Some(out.at) },
        "peers": if out.ok { serde_json::Value::Array(peers) } else { prev_peers },
        // 「在线但读不懂」是成功轮里也要留的一句话，不是失败
        "unreadablePeers": if out.ok { serde_json::Value::Array(unreadable) } else { prev_unreadable },
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

    fn peer(name: &str, modified: Option<i64>) -> Entry {
        Entry { name: name.to_owned(), size: 10, is_dir: false, modified }
    }

    fn names(list: &[&Entry]) -> Vec<String> {
        list.iter().map(|e| e.name.clone()).collect()
    }

    /// 窗口只有 8 个位子，从前"谁落选"由坚果云的名字顺序决定 —— 而名字顺序是
    /// `device-pc-*` 恒在 `device-phone-*` 前面，跟"谁最近写过"毫无关系。
    #[test]
    fn the_window_takes_the_newest_envelopes_not_the_first_names() {
        let mut entries: Vec<Entry> = (0..8)
            .map(|i| peer(&format!("device-pc-stale{i}.enc"), Some(1_000)))
            .collect();
        entries.push(peer("device-phone-joined-later.enc", Some(2_000)));

        let (chosen, dropped) = peer_window(&entries, "pc-me");
        assert!(
            names(&chosen).contains(&"device-phone-joined-later.enc".to_owned()),
            "刚写过的那台被 8 份旧的挤在窗口外：{:?}",
            names(&chosen)
        );
        assert_eq!(names(&dropped), vec!["device-pc-stale7.enc"], "落选的应该是最旧那份");
    }

    /// 自己那份既不被读回来，也不该占掉一个对端的位子（从前靠 `MAX_PEERS + 1` 让位，
    /// 而那个位子只在"自己恰好排在窗口里"时才起作用）。
    #[test]
    fn our_own_envelope_does_not_take_a_peer_slot() {
        let mut entries: Vec<Entry> = (0..8)
            .map(|i| peer(&format!("device-phone-{i}.enc"), Some(1_000 + i as i64)))
            .collect();
        entries.push(peer("device-pc-me.enc", Some(9_000)));

        let (chosen, dropped) = peer_window(&entries, "pc-me");
        assert_eq!(chosen.len(), MAX_PEERS, "{:?}", names(&chosen));
        assert!(dropped.is_empty(), "8 个对端加自己正好装得下：{:?}", names(&dropped));
        assert!(
            !names(&chosen).contains(&"device-pc-me.enc".to_owned()),
            "{:?}",
            names(&chosen)
        );
    }

    /// 「服务器没给这个属性」与「这份很旧」是两件事：不知道时间的排最后，
    /// 但全都不知道时保持 listing 原样，不因为解析不出日期就裁人。
    #[test]
    fn unknown_mtime_sorts_last_instead_of_looking_oldest() {
        let entries = vec![
            peer("device-phone-a.enc", None),
            peer("device-phone-b.enc", Some(500)),
            peer("device-phone-c.enc", None),
        ];
        let (chosen, dropped) = peer_window(&entries, "pc-me");
        assert_eq!(names(&chosen), vec!["device-phone-b.enc", "device-phone-a.enc", "device-phone-c.enc"]);
        assert!(dropped.is_empty());

        let all_unknown: Vec<Entry> = (0..MAX_PEERS + 2)
            .map(|i| peer(&format!("device-phone-{i}.enc"), None))
            .collect();
        let (chosen, dropped) = peer_window(&all_unknown, "pc-me");
        assert_eq!(chosen.len(), MAX_PEERS);
        assert_eq!(names(&dropped).len(), 2, "装不下时仍按 listing 顺序裁：{:?}", names(&dropped));
    }

    #[test]
    fn a_small_fleet_loses_nobody() {
        let entries: Vec<Entry> = (0..3)
            .map(|i| peer(&format!("device-phone-{i}.enc"), Some(1_000)))
            .collect();
        let (chosen, dropped) = peer_window(&entries, "pc-me");
        assert_eq!(chosen.len(), 3);
        assert!(dropped.is_empty());
    }

    /// 「清理读不懂的对端文件」删的是别人发布的数据，所以判据要能在单测里钉住：
    /// 交集（这一轮真读不懂 ∩ 远端此刻真有）、不含自己那份、别人已经清掉的不再动。
    #[test]
    fn cleanup_targets_are_the_intersection_of_unreadable_and_present() {
        let listing: Vec<Entry> = ["device-phone-a.enc", "device-phone-b.enc", "device-pc-old.enc"]
            .iter()
            .map(|n| peer(n, Some(1)))
            .collect();

        let got = clean_targets("pc-me", &listing, &["phone-a".to_owned()]);
        assert_eq!(got, vec!["device-phone-a.enc".to_owned()]);

        let nothing = clean_targets("pc-me", &listing, &[]);
        assert!(nothing.is_empty(), "没读不懂的时不许删任何东西");

        // "看起来不像合法信封"不是删除理由：listing 里读得懂的一概不碰
        let readable = clean_targets(
            "pc-me",
            &listing,
            &["pc-old".to_owned(), "phone-a".to_owned()],
        );
        assert_eq!(readable.len(), 2, "{readable:?}");

        // 远端已经没有的那份（比如那台设备自己清过）不该报失败，也不该去删
        let gone = clean_targets("pc-me", &listing, &["phone-missing".to_owned()]);
        assert!(gone.is_empty(), "{gone:?}");

        // 自己的那份即便出现在名单里也永远不删
        let mine = clean_targets("pc-me", &listing, &["pc-me".to_owned()]);
        assert!(mine.is_empty(), "本机的云端历史不能由这个入口删：{mine:?}");
    }

    #[test]
    fn a_stray_id_still_only_produces_a_device_file_name() {
        let listing = vec![peer("device-phone-a.enc", Some(1))];
        // id 是从 `device-<id>.enc` 反解出来的，重新拼名字必须拼得回去
        assert_eq!(file_name("phone-a"), "device-phone-a.enc");
        assert_eq!(
            clean_targets("pc-me", &listing, &["phone-a".to_owned()]),
            vec!["device-phone-a.enc".to_owned()]
        );
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

    /// 同一条 `run` 的另一头：**读不出凭据不是没配凭据**。用 `DROP TABLE settings` 造真故障
    /// （不用碰凭据管理器，所以这条能在任何机器上跑）。它必须落成一条失败而不是 skipped ——
    /// 折成 skipped 的话，界面就只会催用户去重填一个本来就填对了的密码（P2-7）。
    #[test]
    fn an_unreadable_credential_is_a_failure_not_a_skip() {
        let c = conn();
        c.execute("DROP TABLE settings", []).unwrap();
        let out = run(&c, "pc-a", &FixedNow(1_700_000_000_000));
        assert!(!out.ok);
        assert!(!out.skipped, "读库故障不许冒充「还没配」");
        let msg = out.error.unwrap();
        assert!(!msg.contains("还没配好"), "故障不能借用没配那句措辞：{msg:?}");
        assert!(!msg.is_empty() && !msg.contains('\n'), "状态必须是一行话：{msg:?}");
    }

}