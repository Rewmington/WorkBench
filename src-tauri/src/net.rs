//! 出网请求的公共配置。
//!
//! 这里只放一件必须三处一致的事：**TLS provider**。ureq 3.5 的默认值是 `Rustls`，
//! 而本 crate 是按 `native-tls`（Windows 的 SChannel）编译的。两边对不上时 ureq 不是
//! 返回错误，而是在任何 I/O 发生之前直接 `panic!` —— release 里 panic 就是整个应用消失。
//!
//! 三条 https 路（备份 / 博客线上比对 / 读药箱云端）都从这里取同一份配置。少写一处，
//! 就是一个只在真机上、只在按下那个按钮时才炸的闪退：桩测试走的是 127.0.0.1 的 http，
//! 压根到不了这段代码。

use std::io::Read;
use std::time::Duration;

use ureq::tls::{RootCerts, TlsConfig, TlsProvider};
use ureq::Agent;

use crate::error::{Error, Result};

/// Windows 证书库那套。不引 rustls，也就不用自己带一份根证书。
///
/// `root_certs(PlatformVerifier)` 不是可选项：ureq 的默认值是 `WebPki`（自带那份 Mozilla 根集），
/// 配上 Schannel 后端后会 `disable_built_in_roots(true)` 并要求链上**逐字节等于** bundle 里的某个根 ——
/// 凡是链末端是"交叉签过的根"的站（Cloudflare/GTS 就是）全部握手失败，而 Windows 自己是信的。
/// 实测过的一次误诊就是把这句话当成了"这台机器的根存储缺 GTS"。
pub fn tls() -> TlsConfig {
    TlsConfig::builder()
        .provider(TlsProvider::NativeTls)
        .root_certs(RootCerts::PlatformVerifier)
        .build()
}

/// 三条路共用的 agent 底座：TLS 配好、连接超时收紧，其余由调用方加。
pub fn agent(global: Duration, connect: Duration, user_agent: &str) -> Agent {
    Agent::config_builder()
        .tls_config(tls())
        .timeout_global(Some(global))
        .timeout_connect(Some(connect))
        .user_agent(user_agent)
        // WebDAV 的动词不在 HTTP 标准方法表里，没这个开关 PROPFIND 会被直接拒掉请求行。
        .allow_non_standard_methods(true)
        .build()
        .into()
}

/// 外部响应最多读多少：**边读边夹**，而不是先 `read_to_end` 再把长度判一遍。
/// 后者在内存爆掉之后才说「超过 32 MiB」，那句话已经没有意义了。多取一个字节
/// 就为了知道「越界了」；合法的小响应完全不受影响。
pub(crate) fn read_capped(mut reader: impl Read, max: u64) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    reader
        .by_ref()
        .take(max + 1)
        .read_to_end(&mut out)
        .map_err(|e| Error::msg(format!("读响应失败：{e}")))?;
    if out.len() as u64 > max {
        return Err(Error::msg(format!(
            "响应超过 {max} 字节上限，已停止读取（对端多半不是我们以为的那个服务）"
        )));
    }
    Ok(out)
}

/// 文本响应的有上限读法。WebDAV 的 XML、博客 JSON 都走这里；非 UTF-8 不 panic，
/// 折成 lossy 文本后交给各层的「不是合法 XML/JSON」判据去说人话。
pub(crate) fn read_capped_text(reader: impl Read, max: u64) -> Result<String> {
    let bytes = read_capped(reader, max)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 回归：`https://` 不许把进程 panic 掉。
    ///
    /// 打一个必然连不上的本地端口就够了 —— provider 判错发生在任何 I/O 之前，
    /// 所以「拿到 Err 而不是 panic」即证明这条链走对了，且不碰任何外部服务、不碰任何人凭据。
    #[test]
    fn https_fails_as_an_error_not_as_a_panic() {
        let a = agent(Duration::from_secs(2), Duration::from_millis(300), "anvil-test");
        let ran = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            a.get("https://127.0.0.1:1/").call()
        }));
        assert!(!ran.is_err(), "https 请求 panic 了：那就是闪退的原文");
        assert!(ran.unwrap().is_err(), "连不上应该是 Err");
    }

    #[test]
    fn a_capped_reader_stops_before_the_whole_bomb_is_in_memory() {
        let big = vec![0u8; 4096];
        let ok = read_capped(big.as_slice(), 4096).unwrap();
        assert_eq!(ok.len(), 4096, "刚好到上限要放行");
        let err = read_capped(big.as_slice(), 4095).unwrap_err().to_string();
        assert!(err.contains("上限"), "{err}");
    }
}
