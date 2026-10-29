//! 出网请求的公共配置。
//!
//! 这里只放一件必须三处一致的事：**TLS provider**。ureq 3.5 的默认值是 `Rustls`，
//! 而本 crate 是按 `native-tls`（Windows 的 SChannel）编译的。两边对不上时 ureq 不是
//! 返回错误，而是在任何 I/O 发生之前直接 `panic!` —— release 里 panic 就是整个应用消失。
//!
//! 三条 https 路（备份 / 博客线上比对 / 读药箱云端）都从这里取同一份配置。少写一处，
//! 就是一个只在真机上、只在按下那个按钮时才炸的闪退：桩测试走的是 127.0.0.1 的 http，
//! 压根到不了这段代码。

use std::time::Duration;

use ureq::tls::{TlsConfig, TlsProvider};
use ureq::Agent;

/// Windows 证书库那套。不引 rustls，也就不用自己带一份根证书。
pub fn tls() -> TlsConfig {
    TlsConfig::builder()
        .provider(TlsProvider::NativeTls)
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
}
