//! 本机加密的小件秘密（速达的 `secret_note`）。
//!
//! 与 M4 备份信封是两把不同的钥匙：备份密钥从坚果云应用密码派生（换机能重算），
//! 这把是本机随机生成的（换机解不开就是解不开）。混用会导致「还原了备份却以为
//! 备注也该能看」这种谎 —— 所以 key_id 直接写进密文前缀，跨机时明说解不开。

use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit},
    Aes256Gcm, Nonce,
};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::win32::creds;

const PREFIX: &str = "v1";

type AesNonce = Nonce<<Aes256Gcm as AeadCore>::NonceSize>;

fn key_id(key: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(key);
    let digest = h.finalize();
    digest[..4].iter().map(|b| format!("{b:02x}")).collect()
}

fn random_bytes() -> [u8; 12] {
    use rand::RngExt;
    rand::rng().random()
}

/// aes-gcm 0.11 的 `Nonce::generate()` 需要 aead/getrandom，我们没开；
/// 自己喂 12 字节随机数，少一条依赖路径，也让 nonce 的来源在代码里看得见。
fn nonce_from(bytes: impl AsRef<[u8]>) -> Result<AesNonce> {
    AesNonce::try_from(bytes.as_ref()).map_err(|_| Error::msg("nonce 长度不对"))
}

fn cipher() -> Result<Aes256Gcm> {
    let key = creds::local_key()?;
    Aes256Gcm::new_from_slice(&key).map_err(|_| Error::msg("本机密钥长度不对"))
}

/// `v1:<key_id>:<base64(nonce | ciphertext | tag)>`
pub fn encrypt(plain: &str) -> Result<String> {
    let key = creds::local_key()?;
    let c = cipher()?;
    let nonce = nonce_from(random_bytes())?;
    let ct = c
        .encrypt(&nonce, plain.as_bytes())
        .map_err(|_| Error::msg("加密失败"))?;

    let mut blob = Vec::with_capacity(nonce.len() + ct.len());
    blob.extend_from_slice(nonce.as_slice());
    blob.extend_from_slice(&ct);
    Ok(format!("{PREFIX}:{}:{}", key_id(&key), B64.encode(blob)))
}

pub fn decrypt(text: &str) -> Result<String> {
    let mut parts = text.splitn(3, ':');
    let (Some(prefix), Some(id), Some(body)) = (parts.next(), parts.next(), parts.next()) else {
        return Err(Error::msg("密文格式不认识，可能是旧版本留下的"));
    };
    if prefix != PREFIX {
        return Err(Error::msg("密文版本不支持"));
    }

    let key = creds::local_key()?;
    if id != key_id(&key) {
        // 诚实优先：不显示乱码，也不显示空，直接说明为什么看不了
        return Err(Error::msg(
            "这条备注是在另一台设备上加密的，当前设备解不开。要留下文字请先把它复制出来。",
        ));
    }

    let blob = B64
        .decode(body)
        .map_err(|_| Error::msg("密文不是合法 base64"))?;
    if blob.len() <= 12 {
        return Err(Error::msg("密文被截断了"));
    }
    let (nonce_bytes, ct) = blob.split_at(12);
    let nonce = nonce_from(nonce_bytes)?;
    let plain = cipher()?
        .decrypt(&nonce, ct)
        .map_err(|_| Error::msg("解密失败：本机密钥变了，或密文被改过"))?;
    String::from_utf8(plain).map_err(|_| Error::msg("解密结果不是文本"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 依赖 Windows 凭据管理器里的本机密钥；这台机器上凭据管理器可用（M0 已验证），
    /// 所以真跑，不做 mock —— mock 掉就等于没测。
    #[test]
    fn round_trip_and_tamper_detection() {
        let secret = "路由器后台：admin / 某密码";
        let cipher = encrypt(secret).unwrap();
        assert!(cipher.starts_with("v1:"));
        assert!(!cipher.contains("admin"), "密文里不该出现明文");
        assert_eq!(decrypt(&cipher).unwrap(), secret);

        // 改一个字符 → 认证标签必须拒掉，不能悄悄解出错的东西
        let mut tampered: Vec<u8> = cipher.into_bytes();
        let last = tampered.len() - 2;
        tampered[last] = if tampered[last] == b'A' { b'B' } else { b'A' };
        assert!(decrypt(&String::from_utf8(tampered).unwrap()).is_err());
    }

    #[test]
    fn a_foreign_key_id_is_reported_as_undecryptable() {
        let cipher = encrypt("x").unwrap();
        let foreign = cipher.replacen(&format!("{PREFIX}:"), "v1:00000000:", 1);
        let err = decrypt(&foreign).unwrap_err();
        assert!(
            err.to_string().contains("另一台设备"),
            "要说明为什么看不了：{err}"
        );
    }

    #[test]
    fn two_encryptions_of_the_same_text_differ() {
        // nonce 每次都换：否则「同一条备注」在库里长得一样，能被看出相等关系
        assert_ne!(encrypt("同一段").unwrap(), encrypt("同一段").unwrap());
    }
}
