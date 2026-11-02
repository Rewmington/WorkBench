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
const ACCOUNT_MEDICINE: &str = "medicine-webdav";
const ACCOUNT_LOCAL_KEY: &str = "local-key";

fn entry(account: &str) -> Result<Entry> {
    Entry::new(SERVICE, account).map_err(|e| Error::msg(format!("凭据管理器不可用：{e}")))
}

fn put(account: &str, password: &str) -> Result<()> {
    entry(account)?
        .set_password(password)
        .map_err(|e| Error::msg(format!("写入凭据管理器失败：{e}")))
}

/// 取一条凭据。**「管理器里没有这一条」和「管理器用不了」是两件事**：
/// 只有前者才该说"还没填"。原来任何错误都被折成同一句话，于是服务被锁、
/// 访问被拒这类真故障会让人去重填一个本来就填对了的密码。
fn take(account: &str, unset: &str) -> Result<String> {
    match entry(account)?.get_password() {
        Ok(secret) => Ok(secret),
        Err(keyring::Error::NoEntry) => Err(Error::msg(unset.to_owned())),
        Err(e) => Err(Error::msg(format!("读凭据管理器里 {account} 那条失败了：{e}"))),
    }
}

fn drop_credential(account: &str) {
    // 删除失败不值得报错：没有凭据就是没配置，调用方按未配置处理即可
    let _ = entry(account).map(|e| e.delete_credential());
}

/// 「确实没有这一条」与「管理器读不出来」分开：只有前者才是 `Ok(false)`。
/// 上面 `take()` 那条理由在这里同样成立，只是这里要的是布尔而不是秘密。
fn password_present(account: &str) -> Result<bool> {
    match entry(account)?.get_password() {
        Ok(_) => Ok(true),
        Err(keyring::Error::NoEntry) => Ok(false),
        Err(e) => Err(Error::msg(format!("读凭据管理器里 {account} 那条失败了：{e}"))),
    }
}

/// 给「框里空着算不算沿用」这类**决策**用的布尔版：折成 `false` 的代价只是让他重打
/// 一遍密码，不会写坏任何东西，所以可以折 —— 但必须留一行日志，因为故障本身是真的。
/// 「要不要跑备份/同步」那种停摆级判断**不许**用这个，走 `password_present`。
fn present_or_false(account: &str) -> bool {
    match password_present(account) {
        Ok(yes) => yes,
        Err(e) => {
            log::warn!("{e}");
            false
        }
    }
}

pub fn webdav_password_present() -> Result<bool> {
    password_present(ACCOUNT_WEBDAV)
}

/// 药箱那条应用密码在不在。**「没有这一条」与「管理器读不出来」分开**，理由同 `webdav_password_present`：
/// 药箱卡片拿它决定「要不要显示云端那一行」，折成 false 就等于把一次真故障说成「你还没配」。
pub fn medicine_password_present() -> Result<bool> {
    password_present(ACCOUNT_MEDICINE)
}

/// 界面上那个密码框**留空**是什么意思：沿用凭据管理器里已有那条，不是"清空"。
///
/// 清空走各页自己的「清除」按钮 —— 那是显式动作。这条规则买的是"换页回来
/// 密码框空着（它本来就不该回显），不必为了改个邮箱重打一遍应用密码"。
/// 返回 `None` = 什么都不用写。
pub fn keep_or_set(incoming: &str, already_stored: bool) -> Result<Option<String>> {
    if !incoming.trim().is_empty() {
        return Ok(Some(incoming.to_owned()))
    }
    if already_stored {
        return Ok(None)
    }
    Err(Error::msg("应用密码不能为空"))
}

/// 坚果云有**两条**应用密码：`webdav` 给工作台自己的备份目录，
/// `medicine-webdav` 只读药箱的同步目录。
///
/// 分成两条买的是「单独吊销 + 看名字知道谁在用」，**不是权限边界** ——
/// 坚果云的应用密码是账号级的，任何一条都能访问整个账号的文件。
/// 真正挡住"写药箱目录"的是类型：`linking::dav_readonly` 只有 PROPFIND 与 GET。
pub fn set_webdav_password(password: &str) -> Result<()> {
    put(ACCOUNT_WEBDAV, password)
}

pub fn webdav_password() -> Result<String> {
    take(ACCOUNT_WEBDAV, "还没在设置里填坚果云应用密码")
}

pub fn clear_webdav_password() {
    drop_credential(ACCOUNT_WEBDAV);
}

pub fn webdav_configured() -> bool {
    present_or_false(ACCOUNT_WEBDAV)
}

pub fn set_medicine_password(password: &str) -> Result<()> {
    put(ACCOUNT_MEDICINE, password)
}

pub fn medicine_password() -> Result<String> {
    take(ACCOUNT_MEDICINE, "还没填药箱的坚果云应用密码")
}

pub fn clear_medicine_password() {
    drop_credential(ACCOUNT_MEDICINE);
}

pub fn medicine_configured() -> bool {
    present_or_false(ACCOUNT_MEDICINE)
}

/// 本机加密密钥（32 字节，base64）。第一次用的时候随机生成。
///
/// 已经有内容却读不成 32 字节时**不再静默换一把**：换钥会让所有旧密文永久解不开，
/// 而当时界面只会说"设置成功"。宁可报出来让人去看那条 `Anvil / local-key`，
/// 也不能把一次可解释的坏状态换成一批不可解释的乱码。
/// 这里只报不修：删条目是用户在凭据管理器里的显式动作，软件不替他删凭据。
pub fn local_key() -> Result<Vec<u8>> {
    let e = entry(ACCOUNT_LOCAL_KEY)?;
    match e.get_password() {
        Ok(stored) => {
            let bytes = base64_decode(&stored).map_err(|_| {
                Error::msg(
                    "凭据管理器里 Anvil / local-key 那条不是一把合法密钥（可能被别的程序改过）。修法：控制面板 → 凭据管理器 → Windows 凭据 → 删掉 Anvil / local-key。删掉之后那几条存成密文的速达备注会读不回来，其余照常。",
                )
            })?;
            if bytes.len() != 32 {
                return Err(Error::msg(format!(
                    "凭据管理器里 Anvil / local-key 那条是 {} 字节、不是 32。修法同上：凭据管理器 → Windows 凭据 → 删掉 Anvil / local-key；删掉之后那几条存成密文的速达备注会读不回来，其余照常",
                    bytes.len()
                )));
            }
            Ok(bytes)
        }
        // 只有「确实没有这一条」才生成新的
        Err(keyring::Error::NoEntry) => {
            let key: [u8; 32] = {
                use rand::RngExt;
                rand::rng().random()
            };
            e.set_password(&base64_encode(&key))
                .map_err(|err| Error::msg(format!("生成本机密钥失败：{err}")))?;
            Ok(key.to_vec())
        }
        Err(err) => Err(Error::msg(format!("读凭据管理器里的本机密钥失败了：{err}"))),
    }
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

    /// 不碰凭据管理器：这条判的是"框里空着到底算什么"，纯函数就够，
    /// 真去写 webdav 那条会把用户已经配好的密码覆盖掉。
    #[test]
    fn an_empty_password_box_means_keep_the_stored_one() {
        assert_eq!(keep_or_set("app-pass", true).unwrap().as_deref(), Some("app-pass"));
        assert_eq!(keep_or_set("", true).unwrap(), None, "已有一条 + 框里空着 = 沿用");
        assert_eq!(keep_or_set("   ", true).unwrap(), None, "只打了空格也算空着");
        for blank in ["", "   "] {
            let err = keep_or_set(blank, false).unwrap_err().to_string();
            assert!(err.contains("应用密码不能为空"), "第一次配置时空着必须当场拒：{err}");
        }
    }
}
