//! 备份信封：`ANVILBKP | ver | kdf | m_kib | t | p | salt[16] | nonce[12] | 密文`
//!
//! 为什么不是「直接 AES(应用密码的 SHA-256)」：坚果云上的那个文件是可以被任何人
//! 下载下来离线攻击的，而应用密码是人手打的低熵字符串。所以用 Argon2id 撑成本。
//!
//! AAD 取「nonce 之前的全部头部字节」：这样 kdf 参数或 salt 被人换掉时
//! 认证标签直接不通过，而不是悄悄用一把错的钥匙解出乱码。

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use rand::RngExt;

use crate::error::{Error, Result};

const MAGIC: &[u8; 8] = b"ANVILBKP";
const VERSION: u8 = 1;
const KDF_ARGON2ID: u8 = 1;

/// 64 MiB / 3 轮 / 1 并行度：日备一次，~120 ms 的派生成本无所谓。
pub const M_KIB: u32 = 65_536;
pub const T_COST: u32 = 3;
pub const P_COST: u32 = 1;

const SALT_LEN: usize = 16;
const NONCE_LEN: usize = 12;
const HEADER_LEN: usize = MAGIC.len() + 1 + 1 + 4 + 4 + 1 + SALT_LEN + NONCE_LEN;

/// 头部里的 kdf 参数是文件带来的，不可信。不夹住的话，一个被改过的
/// m_cost 字节就能让「还原」在派生密钥时申请十几 GB 内存。
const MAX_M_KIB: u32 = 1_048_576; // 1 GiB
const MAX_T_COST: u32 = 16;
const MAX_P_COST: u32 = 8;

pub struct Envelope {
    pub kdf: u8,
    pub m_kib: u32,
    pub t_cost: u32,
    pub p_cost: u32,
    pub salt: [u8; SALT_LEN],
    pub nonce: [u8; NONCE_LEN],
}

fn derive(secret: &str, salt: &[u8], m_kib: u32, t: u32, p: u32) -> Result<[u8; 32]> {
    let params = Params::new(m_kib, t, p, Some(32))
        .map_err(|e| Error::msg(format!("KDF 参数不合法：{e}")))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon
        .hash_password_into(secret.as_bytes(), salt, &mut key)
        .map_err(|e| Error::msg(format!("密钥派生失败：{e}")))?;
    Ok(key)
}

/// `secret` 是 `email:应用密码` —— 邮箱也进 KDF，
/// 这样同一个应用密码在别的账号下解不开我们的快照。
pub fn seal(secret: &str, plaintext: &[u8]) -> Result<Vec<u8>> {
    let salt: [u8; SALT_LEN] = rand::rng().random();
    let nonce: [u8; NONCE_LEN] = rand::rng().random();
    let key = derive(secret, &salt, M_KIB, T_COST, P_COST)?;

    let mut out = Vec::with_capacity(HEADER_LEN + plaintext.len());
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.push(KDF_ARGON2ID);
    out.extend_from_slice(&M_KIB.to_be_bytes());
    out.extend_from_slice(&T_COST.to_be_bytes());
    out.push(P_COST as u8);
    out.extend_from_slice(&salt);
    let aad_len = out.len();
    out.extend_from_slice(&nonce);

    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| Error::msg("密钥长度不对"))?;
    let ct = cipher
        .encrypt(
            &Nonce::from(nonce),
            Payload {
                msg: plaintext,
                aad: &out[..aad_len],
            },
        )
        .map_err(|_| Error::msg("加密失败"))?;
    out.extend_from_slice(&ct);
    Ok(out)
}

pub fn open(secret: &str, blob: &[u8]) -> Result<Vec<u8>> {
    if blob.len() < HEADER_LEN + 16 {
        return Err(Error::msg("快照文件太短，不像 Anvil 的备份"));
    }
    if &blob[..MAGIC.len()] != MAGIC {
        return Err(Error::msg("这不是 Anvil 的备份文件（魔数不匹配）"));
    }
    if blob[MAGIC.len()] != VERSION {
        return Err(Error::msg("备份版本不支持"));
    }
    let env = parse_header(blob)?;
    if env.kdf != KDF_ARGON2ID {
        return Err(Error::msg("不支持的密钥派生方式"));
    }

    let key = derive(secret, &env.salt, env.m_kib, env.t_cost, env.p_cost)?;
    let aad_len = HEADER_LEN - NONCE_LEN;
    let ct = &blob[HEADER_LEN..];
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|_| Error::msg("密钥长度不对"))?;
    cipher
        .decrypt(
            &Nonce::from(env.nonce),
            Payload {
                msg: ct,
                aad: &blob[..aad_len],
            },
        )
        .map_err(|_| Error::msg("解密失败：应用密码不对，或这个快照来自另一台机器/被改过"))
}

fn parse_header(blob: &[u8]) -> Result<Envelope> {
    // 头部布局：magic(8) ver(1) kdf(1) m(4) t(4) p(1) salt(16) nonce(12)
    let kdf = blob[MAGIC.len() + 1];
    let mut i = MAGIC.len() + 2;

    let m_kib = u32::from_be_bytes(blob[i..i + 4].try_into().unwrap());
    i += 4;
    let t_cost = u32::from_be_bytes(blob[i..i + 4].try_into().unwrap());
    i += 4;
    let p_cost = blob[i] as u32;
    i += 1;

    if m_kib == 0
        || m_kib > MAX_M_KIB
        || t_cost == 0
        || t_cost > MAX_T_COST
        || p_cost == 0
        || p_cost > MAX_P_COST
    {
        return Err(Error::msg(format!(
            "备份头部的 KDF 参数超出可接受范围（m={m_kib} KiB / t={t_cost} / p={p_cost}），拒绝派生"
        )));
    }

    let mut salt = [0u8; SALT_LEN];
    salt.copy_from_slice(&blob[i..i + SALT_LEN]);
    i += SALT_LEN;

    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&blob[i..i + NONCE_LEN]);

    Ok(Envelope {
        kdf,
        m_kib,
        t_cost,
        p_cost,
        salt,
        nonce,
    })
}

/// 只读头部（列远端清单时判断能不能解、要不要跳过）。
pub fn describe(blob: &[u8]) -> Option<String> {
    if blob.len() < HEADER_LEN || &blob[..MAGIC.len()] != MAGIC {
        return None;
    }
    let env = parse_header(blob).ok()?;
    Some(format!("v{} kdf={} salt={}", blob[MAGIC.len()], env.kdf, B64.encode(env.salt)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "me@example.com:app-pass-123";

    #[test]
    fn round_trip_recovers_the_plaintext() {
        let plain = b"SQLite format 3\0 fake database bytes".to_vec();
        let sealed = seal(SECRET, &plain).unwrap();
        assert_ne!(&sealed[HEADER_LEN..], &plain[..]);
        assert_eq!(open(SECRET, &sealed).unwrap(), plain);
    }

    #[test]
    fn two_seals_of_the_same_bytes_differ() {
        let a = seal(SECRET, b"x").unwrap();
        let b = seal(SECRET, b"x").unwrap();
        assert_ne!(a, b, "salt 与 nonce 每次都换，否则能看出两份备份内容相同");
    }

    #[test]
    fn wrong_password_is_rejected_not_garbled() {
        let sealed = seal(SECRET, b"payload").unwrap();
        let err = open("me@example.com:wrong-pass", &sealed).unwrap_err();
        assert!(err.to_string().contains("解密失败"), "{err}");
    }

    #[test]
    fn tampering_with_kdf_params_or_salt_is_detected() {
        let mut sealed = seal(SECRET, b"payload").unwrap();
        // 改 m_cost 的最低位（64 MiB → 64 MiB+1 KiB，仍在可接受范围内）：
        // AAD 覆盖 nonce 之前的头部，所以认证标签直接不通过
        let m_low = MAGIC.len() + 2 + 3;
        sealed[m_low] ^= 0x01;
        assert!(open(SECRET, &sealed).is_err(), "kdf 参数被换掉必须检测出来");

        let mut sealed2 = seal(SECRET, b"payload").unwrap();
        let salt_at = MAGIC.len() + 2 + 4 + 4 + 1;
        sealed2[salt_at] ^= 0xff;
        assert!(open(SECRET, &sealed2).is_err());
    }

    #[test]
    fn absurd_kdf_cost_is_refused_before_deriving() {
        // 头部是文件带来的：一个被改过的 m_cost 高位不能让我们申请十几 GB 内存
        let mut sealed = seal(SECRET, b"payload").unwrap();
        sealed[MAGIC.len() + 2] = 0x01;
        let err = open(SECRET, &sealed).unwrap_err();
        assert!(err.to_string().contains("超出可接受范围"), "{err}");
        assert!(describe(&sealed).is_none(), "读不出合理参数的文件不该报出头部");
    }

    #[test]
    fn truncated_or_foreign_files_are_refused_cleanly() {
        assert!(open(SECRET, b"short").is_err());
        let mut sealed = seal(SECRET, b"payload").unwrap();
        sealed[..MAGIC.len()].copy_from_slice(b"NOTANVIL");
        let err = open(SECRET, &sealed).unwrap_err();
        assert!(err.to_string().contains("不是 Anvil"), "{err}");
    }

    #[test]
    fn describe_reads_the_header_without_a_password() {
        let sealed = seal(SECRET, b"payload").unwrap();
        let text = describe(&sealed).unwrap();
        assert!(text.contains("kdf=1"), "{text}");
        assert!(describe(b"garbage").is_none());
    }
}
