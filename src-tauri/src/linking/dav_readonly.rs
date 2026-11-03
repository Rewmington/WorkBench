//! 只读 WebDAV 客户端。
//!
//! 与 `backup::webdav::Remote` 是**两个类型**，这个是故意做窄的：只有 PROPFIND 与 GET，
//! 压根没有 PUT / MKCOL / DELETE 方法。药箱目录「绝不写入」这条边界因此落在编译期 ——
//! 不靠运行时判断，也不靠注释提醒后来人。（备份那边确实要写自己的目录，所以它用三处运行时钉子。）

use std::io::Read;
use std::str::FromStr;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use ureq::http::{self, Method};
use ureq::{Agent, Body, RequestExt};

use crate::backup::propfind::{self, Entry};
use crate::error::{Error, Result};

/// 药箱自己的同步目录名。我们读它，永远不写它。
pub const MEDICINE_DIR: &str = "medicine-sync";
pub const DEFAULT_BASE: &str = "https://dav.jianguoyun.com/dav/medicine-sync/";

/// 药箱 app 只发坚果云；这里不把它当成安全边界（凭据本来就是账号级的），
/// 但地址一旦能填任意主机，这条只读路就会把应用密码送到那台机器上。
/// 白名单只放行真服务；debug 构放本地桩，让 e2e 能跑。
const MEDICINE_HOST: &str = "dav.jianguoyun.com";
const LOOPBACKS: [&str; 3] = ["127.0.0.1", "localhost", "::1"];

/// 一次最多读几份设备快照。真实使用是 2–3 台，给到 8 已经宽。
/// 上限存在的理由是：这是健康数据，别对着一个陌生目录无界地拉文件。
pub const MAX_DOCS: usize = 8;

/// 单份快照的体积上限。药箱的全量快照是几百行 JSON，超这数说明地址填到了别处。
const MAX_BYTES: u64 = 8 * 1024 * 1024;

/// PROPFIND 清单的上限。`list_devices` 只认 `device-*.json`，4 MiB 的 XML 已经能列很久。
const MAX_XML_BYTES: u64 = 4 * 1024 * 1024;

fn agent() -> Agent {
    // 与备份那条共用一份底座（TLS provider 配错会在发请求前 panic）。
    crate::net::agent(
        Duration::from_secs(30),
        Duration::from_secs(5),
        "Anvil/0.1 (read-only medicine link)",
    )
}

/// 一份云端文档：文件名 + 字节。
pub struct Doc {
    pub name: String,
    pub bytes: Vec<u8>,
}

/// 药箱按这个前缀挑文件合并，所以别的都不是快照 —— 跟着它的口径走，不自作主张。
fn keep_device(name: &str) -> bool {
    name.starts_with("device-") && name.ends_with(".json")
}

pub struct ReadRemote {
    base: String,
    user: String,
    pass: String,
}

/// 主机之后、查询串之前的**路径分段**（顺手做百分号解码，不让 `%77orkbench` 这种写法多一个洞）。
/// 不引 `url` crate：这里只要分段，为一条校验加依赖不值得。
fn path_segments(base: &str) -> Vec<String> {
    let after_host = base
        .split_once("://")
        .map_or(base, |(_, rest)| rest)
        .split(['?', '#'])
        .next()
        .unwrap_or_default();
    let path = after_host.split_once('/').map_or("", |(_, rest)| rest);
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(crate::backup::propfind::percent_decode)
        .collect()
}

/// 把 `scheme://authority/path` 拆成 scheme + authority；不处理反斜杠之类的花样，
/// 因为这类地址本来就不是合法 URL，直接拒掉比猜更安全。
fn scheme_and_authority(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim();
    let (scheme, rest) = trimmed.split_once("://")?;
    let authority = rest
        .split(['/', '?', '#'])
        .next()
        .filter(|a| !a.is_empty())?
        .to_owned();
    Some((scheme.to_ascii_lowercase(), authority))
}

/// `host[:port]`；拒绝 userinfo、拒绝裸 IPv6 这种我们解析不来的写法。
/// debug 本地桩用 `http://127.0.0.1:PORT/...` 或 `http://localhost:PORT/...`。
fn host_of_authority(authority: &str) -> Option<String> {
    if authority.contains('@') {
        return None;
    }
    let host = if let Some(stripped) = authority.strip_prefix('[') {
        let (host, _) = stripped.split_once(']')?;
        host
    } else if let Some((host, _)) = authority.split_once(':') {
        host
    } else {
        authority
    };
    if host.is_empty() {
        return None;
    }
    Some(host.to_ascii_lowercase())
}

fn address_is_allowed(raw: &str, dev: bool) -> bool {
    let Some((scheme, authority)) = scheme_and_authority(raw) else {
        return false;
    };
    let Some(host) = host_of_authority(&authority) else {
        return false;
    };
    let loopback = LOOPBACKS.contains(&host.as_str());
    match scheme.as_str() {
        "https" => host == MEDICINE_HOST || (dev && loopback),
        // 只在 debug 放本地桩走 http；release 里这一支永远关着，
        // 省得一个手滑少写一个 s 就把坚果云应用密码送进明文 HTTP。
        "http" => dev && loopback,
        _ => false,
    }
}

impl ReadRemote {
    /// 只判地址本身，不碰凭据 —— 设置页存目录之前先用它挡一次，
    /// 免得填错的地址要等到下次刷新才暴露。
    pub fn check_dir(base: &str) -> Result<()> {
        let trimmed = base.trim();
        if trimmed.is_empty() {
            return Err(Error::msg("还没填药箱的同步目录地址"));
        }
        if !address_is_allowed(trimmed, cfg!(debug_assertions)) {
            return Err(Error::msg(
                "药箱同步目录必须是 https://dav.jianguoyun.com/...；这条路会带应用密码出网，不能手滑发到 http 或别的主机",
            ));
        }
        // 反向钉子：这个客户端不该去读工作台自己的两个目录。
        // 读不到东西是小事，把两份数据混成一个"药箱进度"是大事。
        //
        // 按**分段相等**判，不按子串：`contains("/workbench-backup/")` 漏得掉两种真填法 ——
        // 末尾不带斜杠的 `…/workbench-backup`、以及大小写不同的 `Workbench-Backup`。
        // （写侧 `backup::webdav::Remote::new` 用的是 `ends_with` 正向白名单，本来就是闭的，
        // 所以这个洞只在读侧。）
        const OFF_LIMITS: [(&str, &str); 2] = [
            ("workbench-backup", "那是工作台自己的备份目录，不是药箱的同步目录"),
            ("workbench-sync", "那是工作台的手机同步目录，不是药箱的同步目录"),
        ];
        for seg in path_segments(trimmed) {
            // §3.6 的第三颗钉子（M11b 补）：手机同步目录也不许读。
            // 那边全是 device-*.enc —— keep_device 虽只认 .json 而拿不到内容，
            // 但"对药箱目录只读"的可辩护性靠的是类型层拒绝，不靠那层后缀过滤侥幸。
            if let Some((_, why)) = OFF_LIMITS
                .iter()
                .find(|(name, _)| seg.eq_ignore_ascii_case(name))
            {
                return Err(Error::msg(*why));
            }
        }
        Ok(())
    }

    pub fn new(base: &str, user: &str, pass: &str) -> Result<Self> {
        Self::check_dir(base)?;
        let trimmed = base.trim();
        if user.trim().is_empty() || pass.trim().is_empty() {
            return Err(Error::msg("还没填药箱的坚果云应用密码"));
        }
        Ok(Self {
            base: if trimmed.ends_with('/') {
                trimmed.to_owned()
            } else {
                format!("{trimmed}/")
            },
            user: user.trim().to_owned(),
            pass: pass.trim().to_owned(),
        })
    }

    /// 本地桩（`tools/webdav-stub.mjs`）靠这个变量接管；release 构建永远走生产地址。
    pub fn from_env(user: &str, pass: &str) -> Result<Self> {
        let base = match std::env::var("ANVIL_MEDICINE_DAV_BASE") {
            Ok(v) if cfg!(debug_assertions) && !v.is_empty() => v,
            _ => DEFAULT_BASE.to_owned(),
        };
        Self::new(&base, user, pass)
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    fn url(&self, name: Option<&str>) -> Result<String> {
        Ok(match name {
            None => self.base.clone(),
            Some(n) => format!("{}{}", self.base, propfind::percent_encode(n)),
        })
    }

    fn auth(&self) -> String {
        format!("Basic {}", B64.encode(format!("{}:{}", self.user, self.pass)))
    }

    fn request(
        &self,
        method: Method,
        name: Option<&str>,
        headers: &[(&str, &str)],
        body: Option<&[u8]>,
    ) -> Result<http::Response<Body>> {
        let uri = self.url(name)?;
        let mut b = http::Request::builder()
            .method(method)
            .uri(uri)
            .header("Authorization", self.auth());
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        // PROPFIND 需要明确的 Content-Length，所以无 body 的动词也传空 Vec。
        let req = b
            .body(body.unwrap_or_default().to_vec())
            .map_err(|e| Error::msg(format!("请求没拼起来：{e}")))?;
        req.with_agent(agent())
            .run()
            .map_err(|e| self.map_http(e))
    }

    /// 列目录。只返回 `device-*.json` —— 药箱按这个前缀挑文件，别的都不是快照。
    pub fn list_devices(&self) -> Result<Vec<Entry>> {
        const BODY: &str = r#"<?xml version="1.0" encoding="utf-8"?><D:propfind xmlns:D="DAV:"><D:prop><D:displayname/><D:getcontentlength/><D:resourcetype/></D:prop></D:propfind>"#;
        let xml = crate::net::read_capped_text(
            self.request(
                Method::from_bytes(b"PROPFIND").expect("内置动词"),
                None,
                &[
                    ("Depth", "1"),
                    ("Content-Type", "application/xml; charset=utf-8"),
                ],
                Some(BODY.as_bytes()),
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
        entries.retain(|e| !e.is_dir && keep_device(e.name.as_str()));
        if entries.len() > MAX_DOCS {
            entries.truncate(MAX_DOCS);
        }
        Ok(entries)
    }

    pub fn get(&self, name: &str) -> Result<Doc> {
        let mut resp = self.request(Method::GET, Some(name), &[], None)?;
        let mut buf = Vec::new();
        resp.body_mut()
            .as_reader()
            .take(MAX_BYTES + 1)
            .read_to_end(&mut buf)
            .map_err(|e| Error::msg(format!("下载 {name} 失败：{e}")))?;
        if buf.len() as u64 > MAX_BYTES {
            return Err(Error::msg("云端快照体积异常（超过 8 MiB），已停止读取"));
        }
        Ok(Doc {
            name: name.to_owned(),
            bytes: buf,
        })
    }

    /// 401/403 的文案沿用你在药箱里已经调好的那句 —— 它准确指出了最常见的两个错因。
    fn map_http(&self, e: ureq::Error) -> Error {
        match e {
            ureq::Error::StatusCode(401) | ureq::Error::StatusCode(403) => Error::msg(
                "坚果云拒绝了凭据：药箱那边要填的是「第三方应用管理」里的应用密码（不是登录密码），邮箱是注册邮箱",
            ),
            ureq::Error::StatusCode(404) => {
                Error::msg("药箱的同步目录不存在：它可能从没同步过，或目录名不是 medicine-sync")
            }
            ureq::Error::StatusCode(code) => Error::msg(format!("坚果云返回了 {code}")),
            other => Error::msg(format!("坚果云请求失败：{other}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backup_directory_is_off_limits_to_the_reader() {
        // 与 backup::webdav 的守卫互为反向：两边都不许碰对方的目录
        let err = ReadRemote::check_dir("https://dav.jianguoyun.com/dav/workbench-backup/")
            .unwrap_err()
            .to_string();
        assert!(err.contains("工作台自己的备份目录"), "{err}");
        assert!(ReadRemote::check_dir("https://dav.jianguoyun.com/dav/medicine-sync/").is_ok());
        assert!(ReadRemote::check_dir("  ").is_err());
    }

    /// §3.6 表里「补拒 /workbench-sync/」那一格（M11b）：手机同步目录对药箱读取器也是禁区。
    #[test]
    fn the_workbench_sync_directory_is_off_limits_too() {
        let err = ReadRemote::check_dir("https://dav.jianguoyun.com/dav/workbench-sync/")
            .unwrap_err()
            .to_string();
        assert!(err.contains("手机同步目录"), "{err}");
        // new() 走的是同一条 check_dir：填错当场拒，不等第一次刷新才发现
        assert!(ReadRemote::new("https://dav.jianguoyun.com/dav/workbench-sync/", "me@x.com", "p").is_err());
    }

    /// 子串判据漏得掉的几种真填法，分段之后都得拦住（P1-11 那半条的实际内容）。
    #[test]
    fn off_limits_are_caught_without_a_trailing_slash_or_under_other_casing() {
        for bad in [
            "https://dav.jianguoyun.com/dav/workbench-backup",
            "https://dav.jianguoyun.com/dav/Workbench-Backup/",
            "https://dav.jianguoyun.com/dav//workbench-backup//",
            "https://dav.jianguoyun.com/dav/workbench-sync?a=1",
            "https://dav.jianguoyun.com/dav/%77orkbench-backup/",
        ] {
            assert!(ReadRemote::check_dir(bad).is_err(), "{bad} 该被拦住");
        }
        // 反过来：不是那一段路径的名字不该被误杀，否则药箱换个目录名就用不了了
        assert!(ReadRemote::check_dir("https://dav.jianguoyun.com/dav/workbench-backup-notes/").is_ok());
        assert!(ReadRemote::check_dir("https://dav.jianguoyun.com/dav/medicine-sync/").is_ok());
    }

    /// 药箱这条路会带着坚果云应用密码出网。`medicine-sync` 目录名对了，不代表主机/协议对了：
    /// 手滑少写一个 `s` 就是明文，写错主机就是把凭据送给别人。release 只放行坚果云；
    /// debug 额外放行本地桩，否则 e2e 没有路可走。
    #[test]
    fn the_read_only_medicine_address_is_https_and_jianguoyun_only() {
        assert!(ReadRemote::check_dir("http://dav.jianguoyun.com/dav/medicine-sync/").is_err());
        assert!(ReadRemote::check_dir("https://evil.example/dav/medicine-sync/").is_err());
        assert!(ReadRemote::check_dir("ftp://dav.jianguoyun.com/dav/medicine-sync/").is_err());
        // 本地桩只在 debug 测试进程里被 check_dir 放过；release 口径单独验纯函数。
        assert!(ReadRemote::check_dir("http://127.0.0.1:8791/dav/medicine-sync/").is_ok());
        assert!(!address_is_allowed("http://127.0.0.1:8791/dav/medicine-sync/", false));
        assert!(!address_is_allowed("https://127.0.0.1/dav/medicine-sync/", false));
        assert!(address_is_allowed("https://127.0.0.1/dav/medicine-sync/", true));
        assert!(!address_is_allowed("https://user@evil/dav/medicine-sync/", true));
    }

    #[test]
    fn a_missing_credential_is_not_silently_treated_as_configured() {
        let dir = "https://dav.jianguoyun.com/dav/medicine-sync/";
        assert!(ReadRemote::new(dir, "me@x.com", "").is_err());
        assert!(ReadRemote::new(dir, "", "pw").is_err());
        // 尾斜杠补上：拼 URL 时少一个斜杠就会打到父目录去
        let r = ReadRemote::new("https://dav.jianguoyun.com/dav/medicine-sync", "me@x.com", "pw").unwrap();
        assert_eq!(r.base(), dir);
    }

    #[test]
    fn only_device_snapshots_are_candidates() {
        assert!(keep_device("device-pc-01.json"));
        assert!(!keep_device("anvil-20261029Z-abcd1234.abackup"), "别把备份件当快照");
        assert!(!keep_device("device-notes.txt"), "前缀对但不是 JSON 就不是");
        assert!(!keep_device("notes.json"), "没有前缀就不是 —— 药箱也不认它");
    }
}
