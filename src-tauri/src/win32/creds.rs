//! Windows 凭据管理器。
//!
//! 两条边界写在这里：
//! 1. 坚果云应用密码由用户在设置里填一次，之后只进凭据管理器，**不落 SQLite、不进备份**
//!    （PLAN §6.2）。药箱把密码明文放在 shared_preferences.json —— 那个缺点不抄，
//!    也**不去读它的凭据文件**。
//! 2. `resources.secret_cipher` 用的是另一把本机随机密钥，与 WebDAV 密码无关：
//!    换机恢复后解不开就该明说解不开，而不是显示乱码冒充成功。

use keyring::Entry;

use crate::error::{Error, Result};

/// 凭据管理器的服务名。所有 Anvil 凭据都在这个命名空间下，不与任何程序共用。
const SERVICE: &str = "Anvil";
const ACCOUNT_WEBDAV: &str = "webdav";
const ACCOUNT_LOCAL_KEY: &str = "local-key";

fn entry(account: &str) -> Result<Entry> {
    Entry::new(SERVICE, account).map_err(|e| Error::msg(format!("凭据管理器不可用：{e}")))
}

pub fn set_webdav_password(password: &str) -> Result<()> {
    entry(ACCOUNT_WEBDAV)?
        .set_password(password)
        .map_err(|e| Error::msg(format!("写入凭据管理器失败：{e}")))
}

pub fn webdav_password() -> Result<String> {
    entry(ACCOUNT_WEBDAV)?
        .get_password()
        .map_err(|_| Error::msg("还没在设置里填坚果云应用密码"))
}

pub fn clear_webdav_password() {
    // 删除失败不值得报错：没有凭据就是没配置，调用方按未配置处理即可
    let _ = entry(ACCOUNT_WEBDAV).map(|e| e.delete_credential());
}

pub fn webdav_configured() -> bool {
    matches!(entry(ACCOUNT_WEBDAV).map(|e| e.get_password()), Ok(Ok(_)))
}

/// 本机加密密钥（32 字节，base64）。第一次用的时候随机生成。
pub fn local_key() -> Result<Vec<u8>> {
    let e = entry(ACCOUNT_LOCAL_KEY)?;
    if let Ok(existing) = e.get_password() {
        if let Ok(bytes) = base64_decode(&existing) {
            if bytes.len() == 32 {
                return Ok(bytes);
            }
        }
    }
    let key: [u8; 32] = {
        use rand::RngExt;
        rand::rng().random()
    };
    e.set_password(&base64_encode(&key))
        .map_err(|err| Error::msg(format!("生成本机密钥失败：{err}")))?;
    Ok(key.to_vec())
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn base64_decode(text: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(text)
        .map_err(|e| Error::msg(format!("凭据内容不是合法 base64：{e}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 这台机器上凭据管理器是可用的（cmdkey 在，用户会话在），所以真跑一遍。
    /// 用一次性账号名，测完删掉，不在别人机器上留下垃圾。
    #[test]
    fn local_key_is_stable_across_calls() {
        let key = local_key().expect("本机密钥");
        assert_eq!(key.len(), 32);
        let again = local_key().unwrap();
        assert_eq!(key, again, "第二次必须拿回同一把钥匙，否则密文永远解不开");
    }
}
