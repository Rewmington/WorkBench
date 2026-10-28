//! 每日快照 → gzip → AES-GCM → 坚果云 `/dav/workbench-backup/`。
//!
//! 单写端（一台电脑）让这件事简单：快照不需要合并、不需要版本号，
//! 恢复就是整文件覆盖（PLAN §10）。备份产物里**不含外部缓存与凭据**：
//! linked_snapshots 里是药箱的健康数据，settings 里有本机绝对路径与邮箱。

pub mod crypto;
pub mod propfind;
pub mod schedule;
pub mod webdav;

use std::io::{Read, Write};
use std::path::Path;

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::state::Paths;
use crate::time::{days, Now};
use crate::win32::creds;
use webdav::Remote;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub at: i64,
    pub ok: bool,
    pub bytes: Option<u64>,
    pub remote_path: Option<String>,
    pub error: Option<String>,
    pub skipped: bool,
}

/// 保留多少份本地暂存副本。远端保留数由设置决定，默认不动远端历史。
pub const KEEP_LOCAL: usize = 3;

/// 从库里导出「可上云的最小快照」：一致快照 → 剥掉不该出门的内容 → gzip。
pub fn export(conn: &Connection, tmp_dir: &Path, ms: i64) -> Result<Vec<u8>> {
    std::fs::create_dir_all(tmp_dir)?;
    let snap = tmp_dir.join(format!("snap-{ms}.db"));
    if snap.exists() {
        std::fs::remove_file(&snap)?;
    }

    // VACUUM INTO 是 SQLite 官方的在线一致快照方式，比复制主文件 + WAL 可靠得多
    conn.execute("VACUUM INTO ?1", params![snap.to_string_lossy().to_string()])?;

    let stripped = Connection::open(&snap)?;
    stripped.execute_batch("DELETE FROM linked_snapshots; DELETE FROM backup_log;")?;
    // 键名用常量，免得哪天改了名字、备份里还带着本机绝对路径和邮箱
    stripped.execute(
        "DELETE FROM settings WHERE key IN (?1, ?2, ?3)",
        params![
            crate::repos::settings::KEY_BLOG_REPO_DIR,
            crate::repos::settings::KEY_MEDICINE_DB,
            crate::repos::settings::KEY_WDAV_EMAIL
        ],
    )?;
    stripped.execute_batch("VACUUM")?;
    drop(stripped);

    let raw = std::fs::read(&snap)?;
    std::fs::remove_file(&snap)?;

    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(9));
    encoder.write_all(&raw)?;
    Ok(encoder.finish()?)
}

pub fn file_name(ms: i64, sealed: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(sealed);
    let digest = h.finalize();
    let tag: String = digest[..4].iter().map(|b| format!("{b:02x}")).collect();
    let stamp = chrono::DateTime::from_timestamp_millis(ms)
        .map(|utc| utc.format("%Y%m%dT%H%M%SZ").to_string())
        .unwrap_or_else(|| format!("ms{ms}"));
    format!("anvil-{stamp}-{tag}.abackup")
}

fn record(conn: &Connection, out: &Outcome, kind: &str) {
    let _ = conn.execute(
        "INSERT INTO backup_log (at, ok, bytes, remote_path, error, kind) VALUES (?1,?2,?3,?4,?5,?6)",
        params![out.at, out.ok as i64, out.bytes.map(|b| b as i64), out.remote_path, out.error, kind],
    );
}

/// 坚果云的注册邮箱。应用密码只进凭据管理器，邮箱不是秘密，留在 settings 里
/// 才能解释凭据条目属于谁（也才让「换机重填」这件事说得清楚）。
pub(crate) fn webdav_email(conn: &Connection) -> Result<String> {
    crate::repos::settings::get_json(conn, crate::repos::settings::KEY_WDAV_EMAIL)?
        .and_then(|v| v.as_str().map(str::to_owned))
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| Error::msg("还没在设置里填坚果云邮箱"))
}

/// 一次完整备份。失败只返回结果并记一行流水，不弹窗（PLAN §5.2 backup_log 的约定）。
/// `kind` 是 `'daily'` 或 `'manual'`，只影响流水里怎么显示，判据都是「今天成功过没有」。
/// 凭据齐不齐是「跳过」而不是「失败」：没填邮箱不该进失败退避，也不该记一条流水。
fn credentials(conn: &Connection) -> Option<(String, String)> {
    let email = webdav_email(conn).ok()?;
    let password = creds::webdav_password().ok()?;
    Some((email, password))
}

/// 备份的纯逻辑：远端与密钥由调用方给，所以 `tests/webdav_e2e.rs` 能对着本地桩跑完整链路，
/// 不必碰 Windows 凭据管理器（PLAN §6.3「不经 Tauri、直接可测」）。
pub fn run_with(
    conn: &Connection,
    tmp_dir: &Path,
    now: &dyn Now,
    kind: &str,
    remote: &Remote,
    secret: &str,
) -> Outcome {
    let ms = now.ms();
    let attempt = || -> Result<(String, u64)> {
        let plain = export(conn, tmp_dir, ms)?;
        let sealed = crypto::seal(secret, &plain)?;
        let name = file_name(ms, &sealed);
        let size = sealed.len() as u64;

        remote.ensure_dir()?;
        remote.put(&name, &sealed)?;
        Ok((name, size))
    };

    let outcome = match attempt() {
        Ok((name, size)) => Outcome {
            at: ms,
            ok: true,
            bytes: Some(size),
            remote_path: Some(name),
            error: None,
            skipped: false,
        },
        Err(e) => Outcome {
            at: ms,
            ok: false,
            bytes: None,
            remote_path: None,
            error: Some(e.to_string()),
            skipped: false,
        },
    };

    record(conn, &outcome, kind);
    prune_local(tmp_dir);
    outcome
}

/// 生产入口：凭据不齐是「跳过」而不是「失败」—— 没填邮箱不该进失败退避，也不该记一条流水。
pub fn run(conn: &Connection, tmp_dir: &Path, now: &dyn Now, kind: &str) -> Outcome {
    let ms = now.ms();
    let Some((email, password)) = credentials(conn) else {
        return Outcome {
            at: ms,
            ok: false,
            bytes: None,
            remote_path: None,
            error: Some("还没配好坚果云凭据，备份先不跑".to_owned()),
            skipped: true,
        };
    };
    match Remote::from_env(&email, &password) {
        Ok(remote) => run_with(
            conn,
            tmp_dir,
            now,
            kind,
            &remote,
            &format!("{email}:{password}"),
        ),
        Err(e) => Outcome {
            at: ms,
            ok: false,
            bytes: None,
            remote_path: None,
            error: Some(e.to_string()),
            skipped: false,
        },
    }
}

/// 本地只留最近几份导出件（正常流程里已经即用即删，这里是兜底）。
fn prune_local(tmp_dir: &Path) {
    let Ok(entries) = std::fs::read_dir(tmp_dir) else {
        return;
    };
    let mut snaps: Vec<_> = entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("snap-"))
        .collect();
    snaps.sort_by_key(|e| std::cmp::Reverse(e.file_name().to_string_lossy().into_owned()));
    for stale in snaps.iter().skip(KEEP_LOCAL) {
        let _ = std::fs::remove_file(stale.path());
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteSnapshot {
    pub name: String,
    pub size: u64,
    pub kind: String,
}

pub fn list(conn: &Connection) -> Result<Vec<RemoteSnapshot>> {
    let Ok(email) = webdav_email(conn) else {
        return Ok(Vec::new());
    };
    let Ok(password) = creds::webdav_password() else {
        return Ok(Vec::new());
    };
    let remote = Remote::from_env(&email, &password)?;
    Ok(remote.list()?
        .into_iter()
        .filter(|e| !e.is_dir && e.name.ends_with(".abackup"))
        .map(|e| RemoteSnapshot {
            name: e.name,
            size: e.size,
            kind: "abackup".to_owned(),
        })
        .collect())
}

/// 换机恢复：填邮箱 + 应用密码，选一份快照整文件覆盖。
/// 覆盖前先把旧库改名留着，任何一步失败都不动原文件。
///
/// 成功之后不写 backup_log —— 那一刻的库已经是别人（旧机器）的库了，
/// 往还原结果里塞一条本机流水是把脏数据写进刚恢复好的档案。
pub fn restore(conn: &mut Connection, paths: &Paths, remote_name: &str, now: &dyn Now) -> Result<Outcome> {
    let ms = now.ms();
    // 凭据必须在换库之前读：换完库连接就是旧机器那份的了
    let outcome = match (webdav_email(conn), creds::webdav_password()) {
        (Ok(email), Ok(password)) => {
            let secret = format!("{email}:{password}");
            match Remote::from_env(&email, &password) {
                Ok(remote) => restore_with(conn, paths, &remote, &secret, remote_name, ms),
                Err(e) => Err(e),
            }
        }
        _ => Err(Error::msg("还没配好坚果云凭据，取不回快照")),
    };

    match outcome {
        Ok(bytes) => Ok(Outcome {
            at: ms,
            ok: true,
            bytes: Some(bytes),
            remote_path: Some(remote_name.to_owned()),
            error: None,
            skipped: false,
        }),
        Err(e) => {
            let out = Outcome {
                at: ms,
                ok: false,
                bytes: None,
                remote_path: None,
                error: Some(e.to_string()),
                skipped: false,
            };
            // 失败路径上连接没被动过，可以安全留痕
            record(conn, &out, "restore");
            Ok(out)
        }
    }
}

/// 还原的纯逻辑，同 `run_with`：远端与密钥由调用方给，测试里不碰凭据管理器。
pub fn restore_with(
    conn: &mut Connection,
    paths: &Paths,
    remote: &Remote,
    secret: &str,
    remote_name: &str,
    ms: i64,
) -> Result<u64> {
    if remote_name.starts_with("device-") {
        return Err(Error::msg("那是药箱的同步文件，不是工作台的备份"));
    }

    let sealed = remote.get(remote_name)?;
    let gz = crypto::open(secret, &sealed)?;

    let mut gunzip = flate2::read::GzDecoder::new(&gz[..]);
    let mut db_bytes = Vec::new();
    gunzip.read_to_end(&mut db_bytes)?;

    let incoming = paths.backups.join(format!("incoming-{ms}.db"));
    std::fs::write(&incoming, &db_bytes)?;

    // 先验再换：integrity_check 不过就原样退回，绝不覆盖可用的库
    {
        let probe = Connection::open_with_flags(
            &incoming,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let ok: String = probe.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if ok != "ok" {
            let _ = std::fs::remove_file(&incoming);
            return Err(Error::msg("快照里的库没通过完整性检查，已放弃还原"));
        }
    }

    // 换库：先放开文件句柄（丢掉连接会顺带 checkpoint WAL），再改名
    *conn = Connection::open_in_memory()?;
    let stamp = days::day_of_ms(ms);
    let backup_of_current = paths.root.join(format!("app.pre-restore-{ms}.db"));
    if paths.db.exists() {
        std::fs::rename(&paths.db, &backup_of_current)?;
    }
    for suffix in ["-wal", "-shm"] {
        let side = format!("{}{suffix}", paths.db.display());
        let _ = std::fs::remove_file(&side);
    }
    std::fs::rename(&incoming, &paths.db)?;

    // 设备 id 必须换新的：整文件恢复会把旧机器的 created_by 带回来，
    // 不换的话「最后写入设备」在两台机器上会说同一个名字。
    let fresh = format!("pc-{}", uuid::Uuid::now_v7().simple());
    std::fs::write(&paths.device_id, &fresh)?;
    log::info!("已还原快照 {remote_name}（{stamp}），新设备 id {fresh}");

    Ok(db_bytes.len() as u64)
}

