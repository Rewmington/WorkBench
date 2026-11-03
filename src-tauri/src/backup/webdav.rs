//! 坚果云 WebDAV。传输层沿用你在药箱里已经跑通的那套（Basic + MKCOL/PROPFIND/PUT/GET），
//! 但**目录必须是独立的**：药箱 PROPFIND 时按 `device-` 前缀无差别挑文件并合并，
//! 工作台一旦往 `/dav/medicine-sync/` 写，就会被当成一台设备，把待办数据混进用药记录。
//! 这条边界在 `new`、`url`、`list` 三处各钉了一次钉子，不靠自觉。

use std::str::FromStr;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ureq::http::{self, Method};
use ureq::{Agent, Body, RequestExt};

use super::propfind::{self, Entry};
use crate::error::{Error, Result};

pub const DEFAULT_BASE: &str = "https://dav.jianguoyun.com/dav/workbench-backup/";

/// PROPFIND 的清单只有文件名/大小/时间；4 MiB 已经够列几十万份。超过这个数不是备份多，
/// 是对端根本不是我们以为的那个目录。
const MAX_XML_BYTES: u64 = 4 * 1024 * 1024;

/// 一份加密备份最多下载多少。还原上限是解压后 512 MiB，信封/ gzip 不会比它大太多；
/// 这里给 1 GiB 余量。这不是「防攻击者」的精确数，是「别让一次手滑的超大文件把内存吃掉」的数。
const MAX_BACKUP_BYTES: u64 = 1024 * 1024 * 1024;

/// 站点侧的目录名。改这个之前先想清楚上面那段话。
pub const REQUIRED_SEGMENT: &str = "/workbench-backup/";

/// ureq 3 的 Agent 只暴露标准动词，MKCOL/PROPFIND 得自己拼 `http::Request`。
fn verb(name: &str) -> Method {
    Method::from_bytes(name.as_bytes()).expect("内置的 WebDAV 动词常量")
}

fn agent() -> Agent {
    // TLS 与 PROPFIND 的门票都在 `crate::net::agent` 里配：三处 https 共用一份，
    // 少一处就会在发请求前 panic（见那个模块的注释）。
    crate::net::agent(
        Duration::from_secs(60),
        Duration::from_secs(5),
        "Anvil/0.1 (personal workbench backup)",
    )
}

/// 「请求没发出去」与「服务器回了非 2xx」必须分开：后者对 MKCOL/DELETE
/// 是正常结局（405 已存在、404 已删除），不能当错误往上抛。
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
    pub base: String,
    user: String,
    pass: String,
}

impl Remote {
    /// `base` 必须以 `/workbench-backup/` 结尾。写错目录不是小事，所以直接拒绝而不是警告。
    pub fn new(base: &str, user: &str, pass: &str) -> Result<Self> {
        let trimmed = base.trim();
        if !trimmed.ends_with(REQUIRED_SEGMENT) {
            return Err(Error::msg(format!(
                "备份目录必须是 {REQUIRED_SEGMENT} 结尾的独立目录，不能与药箱共用"
            )));
        }
        if trimmed.contains("medicine-sync") {
            return Err(Error::msg(
                "那是药箱的同步目录，工作台必须写进自己的 workbench-backup",
            ));
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
    pub fn from_env(user: &str, pass: &str) -> Result<Self> {
        let base = match std::env::var("ANVIL_WEBDAV_BASE") {
            Ok(v) if cfg!(debug_assertions) && !v.is_empty() => v,
            _ => DEFAULT_BASE.to_owned(),
        };
        Self::new(&base, user, pass)
    }

    fn url(&self, name: Option<&str>) -> std::result::Result<String, Error> {
        match name {
            None => Ok(self.base.clone()),
            Some(n) if n.starts_with("device-") => Err(Error::msg(
                "绝不写 device-* 命名：那是药箱的同步文件名，会被它当成一台设备",
            )),
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
    /// 405（已存在）与 409（父级不存在，说明路径填错了）都在这儿翻成中文。
    pub fn ensure_dir(&self) -> Result<()> {
        let sent = self.send(verb("MKCOL"), None, &[], None);
        match sent.code() {
            Some(405) | Some(201) | Some(204) => Ok(()),
            Some(409) => Err(Error::msg("上层目录不存在：检查坚果云里 workbench-backup 的完整路径")),
            _ => sent.into_result(self).map(|_| ()),
        }
    }

    pub fn list(&self) -> Result<Vec<Entry>> {
        let body = r#"<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:displayname/><D:getcontentlength/><D:resourcetype/></D:prop></D:propfind>"#;
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
        // 万一远端真混进了 device-* 文件，也绝不返回给上层
        entries.retain(|e| !e.name.starts_with("device-"));
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
        crate::net::read_capped(resp.body_mut().as_reader(), MAX_BACKUP_BYTES)
    }

    /// 幂等删除：404（本来就不在）与 405（不是可删的资源）都算删干净了。
    pub fn delete(&self, name: &str) -> Result<()> {
        let sent = self.send(Method::DELETE, Some(name), &[], None);
        if matches!(sent.code(), Some(404) | Some(405)) {
            return Ok(());
        }
        sent.into_result(self).map(|_| ())
    }

    /// 401/403 的文案沿用你在药箱里已经调好的那句 —— 它准确指出了最常见的两个错因。
    fn map_http(&self, e: ureq::Error) -> Error {
        match e {
            ureq::Error::StatusCode(401) | ureq::Error::StatusCode(403) => Error::msg(
                "坚果云拒绝了凭据：检查账号是否填的是注册邮箱、密码是否用「第三方应用管理」里的应用密码（不是登录密码）",
            ),
            ureq::Error::StatusCode(code) => {
                Error::msg(format!("坚果云返回了 {code}"))
            }
            other => Error::msg(format!("坚果云请求失败：{other}")),
        }
    }
}
