//! 每日快照 → gzip → AES-GCM → 坚果云 `/dav/workbench-backup/`。
//!
//! 单写端（一台电脑）让这件事简单：快照不需要合并、不需要版本号，
//! 恢复就是整文件覆盖（PLAN §10）。备份产物里**不含外部缓存与凭据**：
//! linked_snapshots 里是药箱的健康数据，settings 里有本机绝对路径与邮箱。
//! 但"不许出门"不等于"还原时该清掉"—— 那几项按 `MACHINE_LOCAL_KEYS` 原地留在这台机器上。

pub mod crypto;
pub mod propfind;
pub mod schedule;
pub mod webdav;

use std::io::Write;
use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::repos::settings;
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
    /// 这次还原之后进程已经不能继续跑了（成功换库、或失败后连回滚都没成功）。
    /// 调用方要重启；不重启时成功路径会拿着空内存库满屏 "no such table"，
    /// 回滚失败路径会拿着 `db::open` 新建出来的空库装作还能用。
    pub restart_required: bool,
}

/// 保留多少份本地暂存副本。远端保留数由设置决定，默认不动远端历史。
///
/// 注意 `snap-{ms}.db` 与 `app.pre-restore-{ms}.db` 都是**明文**库文件，留 3 份是刻意的：
/// 崩在换库中间时那是本机唯一还能读回原样的东西（`prune_local` / `prune_pre_restore`
/// 只删超出这 3 份的旧件）。想改成「即用即删」之前，先想清楚 P0-2 那条失败路径怎么收场。
pub const KEEP_LOCAL: usize = 3;

/// 还原时解压上限：备份就是这台机器的库，比同步快照大得多，但仍要有个数 ——
/// 「不设上限」和「设一个装得下的数」是两件不同的事。
const MAX_RESTORED_DB_BYTES: usize = 512 * 1024 * 1024;

/// 属于**这台机器**的四项设置：两个坚果云邮箱（它解释凭据管理器里那条密码属于谁）
/// 与两个外部程序的绝对路径。
///
/// 同一份名单管两头：`export` 拿它剥（不许出门），`restore_with` 拿它留（还原是换库
/// 不是换机器）。两头共用一个常量，才不会哪天加了一项只记得改一头。
pub const MACHINE_LOCAL_KEYS: [&str; 4] = [
    crate::repos::settings::KEY_BLOG_REPO_DIR,
    crate::repos::settings::KEY_MEDICINE_DB,
    crate::repos::settings::KEY_WDAV_EMAIL,
    // 药箱那边用的邮箱同样是邮箱。目录地址留着（它机器无关，还原后仍然有效）
    crate::repos::settings::KEY_MEDICINE_EMAIL,
];

fn machine_local_rows(conn: &Connection) -> Result<Vec<(String, serde_json::Value)>> {
    let mut out = Vec::new();
    for key in MACHINE_LOCAL_KEYS {
        if let Some(v) = settings::get_json(conn, key)? {
            out.push((key.to_owned(), v));
        }
    }
    Ok(out)
}

/// 把抄下来的那几项写回刚还原好的库，返回写回了几项。
fn keep_machine_local(
    db: &Path,
    rows: &[(String, serde_json::Value)],
    ms: i64,
    device: &str,
) -> Result<usize> {
    if rows.is_empty() {
        return Ok(0)
    }
    let conn = Connection::open(db)?;
    for (key, value) in rows {
        settings::set_json(&conn, key, value, ms, device)?;
    }
    Ok(rows.len())
}

/// 还原之后把**这台机器**的待办全部标成待上传。
///
/// 为什么必须补这一刀：`todos.dirty` 的意思是「这台机器还没把它传出去」，
/// 而整文件还原带回来的是**源机器那一刻**的 dirty 值。若源机器当时同步干净，
/// 新机还原完 pending=0，而 §3.2 那条"没有本地写入就跳过 PUT"会让它
/// 在用户改动第一条待办之前**永远不发**自己的 `device-*.enc` —— 手机端于是永远
/// 看不到 PC 那份数据，而界面上那句"N 条待上传"还显示 0，看着像已经同步好了。
///
/// 只做 `todos`：`classes` 没有 dirty 列（手机只读它，PLAN-MOBILE §3.4）。
fn mark_all_pending(db: &Path) -> Result<usize> {
    let conn = Connection::open(db)?;
    Ok(conn.execute("UPDATE todos SET dirty = 1", [])?)
}

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
    for key in MACHINE_LOCAL_KEYS {
        stripped.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
    }
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

/// 清空本机流水。**远端文件与本地暂存副本一个都不动** ——
/// 清的是「这张列表」，不是备份本身；判"今天要不要再备一次"的是 `last_success`，
/// 它读的是同一张表，所以清完之后当天会重新备一份（这是对的，不是 bug）。
pub fn clear_log(conn: &Connection) -> Result<usize> {
    Ok(conn.execute("DELETE FROM backup_log", [])?)
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
///
/// **但「凭据管理器读不出来」不是没配**：那必须算失败、进退避、记流水，界面才会在
/// 该红的地方红着。原来这里 `.ok()?` 把故障折成 `None`，于是服务被锁这类真故障
/// 只会说「你还没填密码」，用户就去重填一个本来就填对了的密码（THREAT-MODEL 承诺的不是这样）。
///
/// `pub(crate)`：`sync::relay` 复用**同一组**凭据与同一个构造方式（PLAN-MOBILE §4），
/// 不新增第三把密码，也正因为共用这一个入口，才绝不会误读药箱存在它自己凭据管理器里的那把。
pub(crate) fn credentials(conn: &Connection) -> Result<Option<(String, String)>> {
    let email = settings::get_json(conn, settings::KEY_WDAV_EMAIL)?
        .and_then(|v| v.as_str().map(str::to_owned))
        .filter(|s| !s.trim().is_empty());
    let Some(email) = email else {
        return Ok(None);
    };
    if !creds::webdav_password_present()? {
        return Ok(None);
    }
    // 上面刚说有、这里却拿不到，或压根读不出来：原样上抛，别再冒充「还没填」。
    Ok(Some((email, creds::webdav_password()?)))
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
            restart_required: false,
        },
        Err(e) => Outcome {
            at: ms,
            ok: false,
            bytes: None,
            remote_path: None,
            error: Some(e.to_string()),
            skipped: false,
            restart_required: false,
        },
    };

    record(conn, &outcome, kind);
    prune_local(tmp_dir);
    outcome
}

/// 生产入口：凭据不齐是「跳过」而不是「失败」—— 没填邮箱不该进失败退避，也不该记一条流水。
/// 但**读凭据用不了是失败**，不是没配：它记流水、进退避，界面才不会再把它说成"你还没填"。
pub fn run(conn: &Connection, tmp_dir: &Path, now: &dyn Now, kind: &str) -> Outcome {
    let ms = now.ms();
    let (email, password) = match credentials(conn) {
        Ok(Some(pair)) => pair,
        Ok(None) => {
            return Outcome {
                at: ms,
                ok: false,
                bytes: None,
                remote_path: None,
                error: Some("还没配好坚果云凭据，备份先不跑".to_owned()),
                skipped: true,
                restart_required: false,
            };
        }
        Err(e) => {
            let out = Outcome {
                at: ms,
                ok: false,
                bytes: None,
                remote_path: None,
                error: Some(e.to_string()),
                skipped: false,
                restart_required: false,
            };
            record(conn, &out, kind);
            return out;
        }
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
            restart_required: false,
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
    let outcome: std::result::Result<u64, RestoreFailure> =
        match (webdav_email(conn), creds::webdav_password()) {
            (Ok(email), Ok(password)) => {
                let secret = format!("{email}:{password}");
                match Remote::from_env(&email, &password) {
                    Ok(remote) => restore_with(conn, paths, &remote, &secret, remote_name, ms),
                    Err(e) => Err(e.into()),
                }
            }
            _ => Err(Error::msg("还没配好坚果云凭据，取不回快照").into()),
        };

    match outcome {
        Ok(bytes) => Ok(Outcome {
            at: ms,
            ok: true,
            bytes: Some(bytes),
            remote_path: Some(remote_name.to_owned()),
            error: None,
            skipped: false,
            // 成功换库后进程手里那个连接已经指向新库，但盘上文件名、设备 id、
            // 可能还有其它视图的缓存都还停在旧现场 —— 一律重启，让 bootstrap 重新认一遍。
            restart_required: true,
        }),
        Err(e) => {
            let restart_required = e.restart_required;
            let out = Outcome {
                at: ms,
                ok: false,
                bytes: None,
                remote_path: None,
                error: Some(e.message),
                skipped: false,
                restart_required,
            };
            // 回滚失败那一格，`conn` 已经不是真库了；往它里面写流水只会再报一次 no such table。
            // 其他失败路径上连接没被动过（或已指回原库），可以安全留痕。
            if !restart_required {
                record(conn, &out, "restore");
            }
            Ok(out)
        }
    }
}

/// 还原失败的现场比「一句错误」更重要：
/// `restart_required=true` 表示进程里已经没有可继续用的真库（或库文件已经换过），
/// 调用方必须重启；普通错误（凭据、网络、校验不过）则不该把用户已经配好的现场搅掉。
#[derive(Debug)]
pub struct RestoreFailure {
    message: String,
    restart_required: bool,
}

impl std::fmt::Display for RestoreFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl From<Error> for RestoreFailure {
    fn from(e: Error) -> Self {
        Self { message: e.to_string(), restart_required: false }
    }
}

impl From<std::io::Error> for RestoreFailure {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e).into()
    }
}

impl From<rusqlite::Error> for RestoreFailure {
    fn from(e: rusqlite::Error) -> Self {
        Error::Db(e).into()
    }
}

/// `swap_db` 的三种失败现场不能折成同一个 `Error`：
/// 回滚成功时把连接指回真库就够了；回滚失败时必须让调用方重启，
/// 绝不能再 `db::open(&paths.db)` —— 那个调用会在文件缺失时**新建一个空库**，
/// 把错误里那句「先别再造新的」当场变成事实。
#[derive(Debug)]
enum SwapFailure {
    /// 没动过现库，或已经把现库改回去了：进程可以继续，错误只是没还原成。
    Reversible(Error),
    /// 现库已被改名、incoming 换不进、改名回来又失败：只剩手工恢复这一条路。
    RollbackFailed(Error),
}

impl std::fmt::Display for SwapFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SwapFailure::Reversible(e) | SwapFailure::RollbackFailed(e) => f.write_str(&e.to_string()),
        }
    }
}

impl SwapFailure {
    fn into_restore_failure(self) -> RestoreFailure {
        match self {
            SwapFailure::Reversible(e) => e.into(),
            SwapFailure::RollbackFailed(e) => RestoreFailure {
                message: e.to_string(),
                restart_required: true,
            },
        }
    }
}

/// 把还原用的库文件换进来：现库 -> `app.pre-restore-{ms}.db`，`incoming` -> 现库。
///
/// 第二步失败时必须把第一步改回去：此时进程里已经没有可用连接了，而真库躺在一个
/// 带 `pre-restore` 名字的位置上 —— 半个换库比不换更糟，界面上什么都看不见的空库
/// 会让人以为数据没了，接着再还原一次，把唯一那份也盖掉。
/// 回滚本身也失败时不假装没事：错误里必须写清该手工改哪个文件。
fn swap_db(paths: &Paths, incoming: &Path, ms: i64) -> std::result::Result<(), SwapFailure> {
    let backup_of_current = paths.root.join(format!("app.pre-restore-{ms}.db"));
    let moved_current = paths.db.exists();
    if moved_current {
        std::fs::rename(&paths.db, &backup_of_current)
            .map_err(|e| SwapFailure::Reversible(Error::Io(e)))?;
    }
    for suffix in ["-wal", "-shm"] {
        let side = format!("{}{suffix}", paths.db.display());
        let _ = std::fs::remove_file(&side);
    }

    match std::fs::rename(incoming, &paths.db) {
        Ok(()) => Ok(()),
        Err(e) => {
            if !moved_current {
                // 原来就没有现库（全新机器），没什么可回滚的
                return Err(SwapFailure::Reversible(Error::msg(format!(
                    "还原没完成（{e}），这台机器上本来还没有库"
                ))));
            }
            if std::fs::rename(&backup_of_current, &paths.db).is_ok() {
                return Err(SwapFailure::Reversible(Error::msg(format!(
                    "还原没完成（{e}），这台机器的库已经改回原样，你的数据一条没动"
                ))));
            }
            Err(SwapFailure::RollbackFailed(Error::msg(format!(
                "还原没完成（{e}），而且没能自动改回去：现库在 {}，Anvil 现在会退出。请把那个文件改名回 {} 再启动，不要在这期间让程序新建 app.db。",
                backup_of_current.display(),
                paths.db.display()
            ))))
        }
    }
}

/// 盘上的 `app.pre-restore-*.db`，新的在前。文件名里就是毫秒时间戳，
/// 位数相同所以按字符串倒序等于按时间倒序（真实时间戳一律 13 位）。
fn pre_restores(root: &Path) -> Vec<(String, std::path::PathBuf)> {
    let Ok(entries) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found: Vec<_> = entries
        .flatten()
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .filter(|(name, _)| name.starts_with("app.pre-restore-"))
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found
}

/// 最近一份「还原前的库」—— 上一次还原中断时，这是本机唯一的退路。
pub fn newest_pre_restore(root: &Path) -> Option<(std::path::PathBuf, String)> {
    pre_restores(root).first().map(|(name, path)| (path.clone(), name.clone()))
}

/// 换库留下的 `app.pre-restore-*.db` 是本机唯一的「还原前状态」，一份都不该乱删；
/// 但还原做多了会堆一串明文库，所以按时间只留最近 `KEEP_LOCAL` 份（与快照同一口径）。
fn prune_pre_restore(root: &Path) {
    for (_, path) in pre_restores(root).into_iter().skip(KEEP_LOCAL) {
        let _ = std::fs::remove_file(path);
    }
}

/// 启动时发现本机库是新建的、而盘上躺着 `app.pre-restore-*`：上一次还原走到一半断了。
///
/// **只报不修**：自动把它改名回 `app.db` 会盖掉用户在这之后写进新库的东西，
/// 那是第二次不可逆损失，而且多半是在用户不知情的时候发生的。
pub fn notice_interrupted_restore(conn: &Connection, now: &dyn Now, root: &Path) {
    let Some((path, _)) = newest_pre_restore(root) else {
        return;
    };
    let msg = format!(
        "上一次还原没走完：现在这个库是新建的空库，旧数据在 {}。先别再点还原，把那个文件改名回 app.db 再启动",
        path.display()
    );
    log::error!("{msg}");
    record(
        conn,
        &Outcome {
            at: now.ms(),
            ok: false,
            bytes: None,
            remote_path: None,
            error: Some(msg),
            skipped: false,
            restart_required: false,
        },
        "restore",
    );
}

/// 还原的纯逻辑，同 `run_with`：远端与密钥由调用方给，测试里不碰凭据管理器。
pub fn restore_with(
    conn: &mut Connection,
    paths: &Paths,
    remote: &Remote,
    secret: &str,
    remote_name: &str,
    ms: i64,
) -> std::result::Result<u64, RestoreFailure> {
    if remote_name.starts_with("device-") {
        return Err(Error::msg("那是药箱的同步文件，不是工作台的备份").into());
    }

    // 本机那四项要在换库之前抄：换完之后 conn 指向的是快照里那份库，
    // 而快照里根本没有它们（`export` 剥掉了）。
    let local = machine_local_rows(conn)?;

    let sealed = remote.get(remote_name)?;
    let gz = crypto::open(secret, &sealed)?;

    let db_bytes = crate::sync::snapshot::gunzip_bounded(&gz, MAX_RESTORED_DB_BYTES)?;

    let incoming = paths.backups.join(format!("incoming-{ms}.db"));
    std::fs::write(&incoming, &db_bytes)?;

    // 先验再换：integrity_check 不过、版本比这个程序新、或表结构根本不是 Anvil，
    // 都原样退回。换库不是修复数据；把一个本机读不懂的库换进来只会把事故变成空库。
    validate_incoming_db(&incoming).inspect_err(|_| {
        let _ = std::fs::remove_file(&incoming);
    })?;

    // 换库：先放开文件句柄（丢掉连接会顺带 checkpoint WAL），再改名。
    // 那个内存连接是为「让 Windows 允许 rename」付出的代价 —— 中途失败时进程手里就
    // 没有活库了，所以换库本身必须能把文件改回去，并把连接重新指回真库。
    *conn = Connection::open_in_memory()?;
    let stamp = days::day_of_ms(ms);
    if let Err(e) = swap_db(paths, &incoming, ms) {
        // 那份明文副本到此没用了：留着就是"备份目录里躺着整库明文"，而 `prune_local` 只扫
        // `snap-*`、`prune_pre_restore` 只扫 `app.pre-restore-*`，谁都不会来收它。
        let _ = std::fs::remove_file(&incoming);
        // 回滚成功或根本没动过现库时，把连接指回 `paths.db`，进程还能继续讲那句错误。
        if let SwapFailure::Reversible(err) = e {
            *conn = crate::db::open(&paths.db)?;
            return Err(err.into());
        }
        // 回滚也失败：这里绝不能 `db::open(&paths.db)`。那个调用会在文件缺失时
        // 新建一个空库，然后把「先别再造新的」变成程序自己造了一个。
        // 让命令层重启；下一次 bootstrap 会走 `notice_interrupted_restore`。
        Err(e.into_restore_failure())
    } else {
        // 换库成功也要把连接指回真库。原来这一句只有失败分支有，于是成功之后进程手里那个
        // 还是空内存库，一直留到 `app.restart()` 才死 —— 中间在 mutex 上排队的每条命令都会
        // 报 "no such table"（还原含一次网络 GET，那几秒是常态）。打开失败不该把已经成功的
        // 还原说成失败 —— 库已经换好了 —— 重启会修好，日志要说清。
        match crate::db::open(&paths.db) {
            Ok(reopened) => *conn = reopened,
            Err(e) => log::error!("还原后重新打开真库失败，重启前的命令会读不到表：{e}"),
        }

        // 设备 id 必须换新的：整文件恢复会把旧机器的 created_by 带回来，
        // 不换的话「最后写入设备」在两台机器上会说同一个名字。
        let fresh = format!("pc-{}", uuid::Uuid::now_v7().simple());
        match std::fs::write(&paths.device_id, &fresh) {
            Ok(()) => log::info!("已还原快照 {remote_name}（{stamp}），新设备 id {fresh}"),
            // 库已经换好了，这一步失败不该把还原说成失败（那会诱发第二次还原）。
            // 继续用旧 id 的后果是「最后写入设备」暂时还是旧机器那台，日志要说清。
            Err(e) => log::error!("新设备 id 没写进去（{e}），这台机器暂时还沿用旧设备 id 上报"),
        }

        prune_pre_restore(&paths.root);

        // 还原进来的每一行都要重新标脏（见 `mark_all_pending`）。
        // 这一步失败不该把已经成功的还原说成失败 —— 库已经换好了 —— 但它会让手机端
        // 看不到这台 PC 的数据，所以日志要说清后果，别说成"无害"。
        match mark_all_pending(&paths.db) {
            Ok(n) => log::info!("还原后把 {n} 条待办标成待上传：这台机器从没传过它们"),
            Err(e) => {
                log::error!("还原后没能把待办标成待上传，在手机看到这台 PC 的数据之前先检查它：{e}")
            }
        }

        // 写回本机设置。这一步失败不该把已经成功的还原说成失败：库已经换好了，
        // 顶多邮箱空着、重填一次就能用，所以只记日志。
        match keep_machine_local(&paths.db, &local, ms, &fresh) {
            Ok(0) => log::info!("这台机器还没有本机专属的设置，还原后无需补回"),
            Ok(n) => log::info!("还原后保留本机设置 {n} 项（两个邮箱与两个绝对路径）"),
            Err(e) => log::error!("还原后写回本机设置失败，需要重填：{e}"),
        }

        Ok(db_bytes.len() as u64)
    }
}

/// 换库前能便宜做的体检全做一遍。`integrity_check` 只证明「这是 SQLite 认得的字节」，
/// 不证明「这台机器能继续用它」：
/// - 比当前程序新的库会在重启时直接起不来，而旧数据只躺在 pre-restore 里，提示不指向那里；
/// - 没有 `todos` / `settings` 的库会被当成一个可用空库启动，用户看到的是「数据没了」。
fn validate_incoming_db(path: &Path) -> Result<()> {
    let probe = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let ok: String = probe.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
    if ok != "ok" {
        return Err(Error::msg("快照里的库没通过完整性检查，已放弃还原"));
    }
    let version: i64 = probe.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > crate::db::migrations::target_version() as i64 {
        return Err(Error::msg(format!(
            "这份快照来自更新的 Anvil（库里第 {version} 版，本机只认到第 {} 版）。先装回不低于它的版本再还原，别拿旧程序去猜新表结构。",
            crate::db::migrations::target_version()
        )));
    }
    for table in ["todos", "settings"] {
        let has: bool = probe
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
                params![table],
                |_| Ok(true),
            )
            .optional()?
            .unwrap_or(false);
        if !has {
            return Err(Error::msg(format!(
                "这份快照里没有 {table} 表，不是 Anvil 的库；已放弃还原"
            )));
        }
    }
    Ok(())
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::migrate;
    use serde_json::json;

    fn fresh_db() -> Connection {
        let mut c = crate::db::open_in_memory().expect("内存库");
        migrate(&mut c).expect("migrate");
        c
    }

    fn count(conn: &Connection, key: &str) -> i64 {
        conn.query_row(
            "SELECT count(*) FROM settings WHERE key = ?1",
            params![key],
            |r| r.get(0),
        )
        .unwrap()
    }

    /// 「没配」与「读不出来」是两句话（P2-7）。`DROP TABLE settings` 是最便宜的真故障，
    /// 而且不用碰凭据管理器 —— 后者在一台没填过 Anvil 的机器上根本没法测。
    /// 这里断言的正是那条不许再犯的形状：故障折成 `None`，界面就只会催人重填一个
    /// 本来就填对了的密码。
    #[test]
    fn a_broken_settings_read_is_a_failure_not_an_absence() {
        let conn = fresh_db();
        conn.execute("DROP TABLE settings", []).unwrap();
        let err = credentials(&conn).expect_err("读库故障必须是 Err，不能是 None");
        assert!(
            !err.to_string().contains("还没配"),
            "故障的措辞不能借用没配那句：{err}"
        );
    }

    /// 反过来那半：真没填（没有行，或填了一串空白）必须是 `Ok(None)`，并且**走不到凭据管理器**。
    /// 这条测试能在任何机器上跑，正因为邮箱缺席时函数就地返回了。
    #[test]
    fn an_absent_or_blank_mailbox_is_an_absence_not_a_failure() {
        let conn = fresh_db();
        assert!(credentials(&conn).unwrap().is_none(), "空库是没配，不是故障");
        settings::set_json(&conn, settings::KEY_WDAV_EMAIL, &json!("   "), 1, "pc-test").unwrap();
        assert!(credentials(&conn).unwrap().is_none(), "一串空白的邮箱也是没填");
    }

    /// 备份不许带凭据，但还原也不该把这台机器自己的设置清掉 —— 两头都要测，
    /// 因为它们是同一份名单（`MACHINE_LOCAL_KEYS`）。
    #[test]
    fn the_artifact_drops_them_and_the_restore_puts_them_back() {
        let tmp = std::env::temp_dir().join(format!("anvil-keep-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();

        let conn = fresh_db();
        let ms = 1_793_000_000_000_i64;
        for (key, value) in [
            (settings::KEY_WDAV_EMAIL, json!("me@example.com")),
            (settings::KEY_MEDICINE_EMAIL, json!("me@example.com")),
            (settings::KEY_BLOG_REPO_DIR, json!("D:/blog")),
            (settings::KEY_MEDICINE_DB, json!("D:/med.db")),
            (settings::KEY_THEME_MODE, json!("dark")),
        ] {
            settings::set_json(&conn, key, &value, ms, "pc-old").unwrap();
        }

        let gz = export(&conn, &tmp, ms).expect("导出");
        // 走生产那一条有上限的解压路径，而不是在测试里另写一份无上限的
        let raw = crate::sync::snapshot::gunzip_bounded(&gz, MAX_RESTORED_DB_BYTES).unwrap();
        let file = tmp.join("restored.db");
        std::fs::write(&file, &raw).unwrap();

        // 出门的那份：四项都没了，机器无关的留着
        let artifact = Connection::open(&file).unwrap();
        for key in MACHINE_LOCAL_KEYS {
            assert_eq!(count(&artifact, key), 0, "{key} 不该出门");
        }
        assert_eq!(count(&artifact, settings::KEY_THEME_MODE), 1, "主题是机器无关的");
        drop(artifact);

        // 还原：换库之前抄，换库之后写回
        let rows = machine_local_rows(&conn).expect("抄本机设置");
        assert_eq!(rows.len(), 4);
        assert_eq!(keep_machine_local(&file, &rows, ms + 1, "pc-new").expect("写回"), 4);

        let after = Connection::open(&file).unwrap();
        for key in MACHINE_LOCAL_KEYS {
            assert_eq!(count(&after, key), 1, "还原之后 {key} 必须还在");
        }
        assert_eq!(
            settings::get_json(&after, settings::KEY_WDAV_EMAIL).unwrap(),
            Some(json!("me@example.com")),
            "写回的必须是这台机器还原前那个值"
        );
        assert_eq!(
            settings::get_json(&after, settings::KEY_THEME_MODE).unwrap(),
            Some(json!("dark")),
            "快照里的主题归快照管，还原不该顺手改它"
        );

        std::fs::remove_dir_all(&tmp).ok();
    }

    /// 换机还原后那台机器"欠着上传"这件事要有钉子：pending 若还是 0，
    /// §3.2 那句"没有本地写入就跳过 PUT"会让它永远不发自己那份，手机端一直看不到 PC。
    #[test]
    fn a_restored_machine_owes_every_row_it_just_got() {
        use crate::model::TodoDraft;
        use crate::repos::todos;
        use crate::sync::merge;
        use crate::time::FixedNow;

        let tmp = std::env::temp_dir().join(format!("anvil-restore-dirty-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("restored.db");
        let ms = 1_793_000_000_000_i64;

        {
            let mut conn = Connection::open(&file).unwrap();
            migrate(&mut conn).unwrap();
            let now = FixedNow(ms);
            for i in 0..3 {
                todos::create(
                    &conn,
                    &now,
                    "pc-old",
                    TodoDraft {
                        title: format!("还原带来的 {i}"),
                        ..Default::default()
                    },
                )
                .unwrap();
            }
            // 源机器那一刻是同步干净的：快照里的 dirty 全是 0
            merge::clear_dirty(&conn, ms).unwrap();
            assert_eq!(merge::pending(&conn).unwrap(), 0, "假设源机器已经把这些传上去了");
        }

        assert_eq!(mark_all_pending(&file).unwrap(), 3);
        let conn = Connection::open(&file).unwrap();
        assert_eq!(
            merge::pending(&conn).unwrap(),
            3,
            "刚还原的这台机器从没传过它们，一条都不该装作已同步"
        );
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// 界面上那个「清空流水」清的是这张列表，不是备份。
    #[test]
    fn clearing_the_log_removes_rows_and_touches_nothing_else() {
        let conn = fresh_db();
        for i in 0..3 {
            record(
                &conn,
                &Outcome {
                    at: 1_793_000_000_000 + i,
                    ok: i != 1,
                    bytes: Some(1024),
                    remote_path: Some(format!("anvil-{i}.abackup")),
                    error: if i == 1 { Some(" dns error".into()) } else { None },
                    skipped: false,
                    restart_required: false,
                },
                "manual",
            );
        }
        let todos = || -> i64 {
            conn.query_row("SELECT count(*) FROM todos WHERE deleted_at IS NULL", [], |r| r.get(0))
                .unwrap()
        };
        crate::repos::todos::create(
            &conn,
            &crate::time::FixedNow(1_793_000_000_000),
            "pc-t",
            crate::model::TodoDraft {
                title: "清流水不该动它".to_owned(),
                ..Default::default()
            },
        )
        .unwrap();
        let before = todos();

        assert_eq!(clear_log(&conn).unwrap(), 3, "报告删掉了几行");
        let left: i64 = conn
            .query_row("SELECT count(*) FROM backup_log", [], |r| r.get(0))
            .unwrap();
        assert_eq!(left, 0);
        assert_eq!(todos(), before, "待办一行都不该少");
        assert!(schedule::last_success(&conn).is_none(), "清完就是「没备过」，别拿旧记忆骗人");
    }

    /// 新机器第一次还原：本机一项设置都还没有，就不该往库里塞空值。
    #[test]
    fn nothing_is_written_back_when_this_machine_has_no_local_settings() {
        let tmp = std::env::temp_dir().join(format!("anvil-keep-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("db.db");
        std::fs::write(&file, []).ok();

        let conn = fresh_db();
        assert!(machine_local_rows(&conn).unwrap().is_empty());
        assert_eq!(keep_machine_local(&file, &[], 1, "pc-new").unwrap(), 0);
        std::fs::remove_dir_all(&tmp).ok();
    }

    /// 换库的失败面只有一种测法：让第二步的 rename 真失败（目标不存在）。
    #[test]
    fn a_half_done_swap_puts_the_real_database_back_where_it_was() {
        let tmp = Paths::under(std::env::temp_dir().join(format!("anvil-swap-{}", uuid::Uuid::now_v7())));
        std::fs::create_dir_all(&tmp.root).unwrap();
        std::fs::create_dir_all(&tmp.backups).unwrap();
        std::fs::write(&tmp.db, "这一年的待办都在这一份里").unwrap();
        let missing_incoming = tmp.backups.join("incoming-7.db");

        let err = swap_db(&tmp, &missing_incoming, 7).unwrap_err().to_string();
        assert!(err.contains("改回原样"), "{err}");
        assert!(tmp.db.exists(), "现库必须还在原位");
        assert_eq!(std::fs::read(&tmp.db).unwrap(), "这一年的待办都在这一份里".as_bytes());
        assert!(!tmp.root.join("app.pre-restore-7.db").exists(), "回滚后不该留下改名件");

        std::fs::remove_dir_all(&tmp.root).ok();
    }

    #[test]
    fn a_successful_swap_keeps_the_previous_database_on_disk() {
        let tmp = Paths::under(std::env::temp_dir().join(format!("anvil-swap-ok-{}", uuid::Uuid::now_v7())));
        std::fs::create_dir_all(&tmp.root).unwrap();
        std::fs::create_dir_all(&tmp.backups).unwrap();
        std::fs::write(&tmp.db, "旧的").unwrap();
        let incoming = tmp.backups.join("incoming-8.db");
        std::fs::write(&incoming, "快照里的那份").unwrap();

        swap_db(&tmp, &incoming, 8).expect("两步都顺就该成功");
        assert_eq!(std::fs::read(&tmp.db).unwrap(), "快照里的那份".as_bytes());
        assert_eq!(
            std::fs::read(tmp.root.join("app.pre-restore-8.db")).unwrap(),
            "旧的".as_bytes(),
            "还原前的状态是唯一的可退路，得留在盘上"
        );
        assert!(!incoming.exists(), "换进来的那份不该再留着");

        std::fs::remove_dir_all(&tmp.root).ok();
    }

    /// 回滚失败那一格没法便宜地造出来（要让 rename 反过来也失败，得在现库已被改名之后
    /// 再把目标占住），但它的后果可以单独钉死：这种失败必须让调用方重启，
    /// 绝不能用普通错误把「进程里没有真库了」说成「这次没还原成，还能继续用」。
    #[test]
    fn a_failed_rollback_is_the_one_failure_that_demands_a_restart() {
        let reversible = SwapFailure::Reversible(Error::msg("已经改回去了"));
        assert!(!reversible.into_restore_failure().restart_required);

        let stuck = SwapFailure::RollbackFailed(Error::msg("没能自动改回去"));
        let failure = stuck.into_restore_failure();
        assert!(failure.restart_required);
        assert!(failure.message.contains("没能自动改回去"), "{}", failure.message);
    }

    #[test]
    fn a_database_this_program_cannot_use_is_refused_before_the_swap() {
        let tmp = std::env::temp_dir().join(format!("anvil-validate-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&tmp).unwrap();
        let file = tmp.join("incoming.db");

        let mut conn = Connection::open(&file).unwrap();
        migrate(&mut conn).unwrap();
        drop(conn);
        validate_incoming_db(&file).expect("刚迁移好的库当然能用");

        conn = Connection::open(&file).unwrap();
        conn.execute_batch("PRAGMA user_version = 99999;").unwrap();
        drop(conn);
        let err = validate_incoming_db(&file).unwrap_err().to_string();
        assert!(err.contains("更新的 Anvil"), "{err}");

        let foreign = tmp.join("foreign.db");
        let conn = Connection::open(&foreign).unwrap();
        conn.execute_batch("CREATE TABLE unrelated (x);").unwrap();
        drop(conn);
        let err = validate_incoming_db(&foreign).unwrap_err().to_string();
        assert!(err.contains("不是 Anvil 的库"), "{err}");

        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn pre_restore_files_are_pruned_to_keep_local_newest() {
        let root = std::env::temp_dir().join(format!("anvil-prune-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&root).unwrap();
        for ms in [1_000_i64, 2_000, 3_000, 4_000, 5_000] {
            std::fs::write(root.join(format!("app.pre-restore-{ms}.db")), b"x").unwrap();
        }
        // 名字里带数字宽度不同，字典序会把 1000 排在 200 前面 —— 这里全用同宽时间戳，
        // 真实文件名（毫秒）本来就同宽
        prune_pre_restore(&root);
        let left: Vec<_> = std::fs::read_dir(&root)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with("app.pre-restore-"))
            .collect();
        assert_eq!(left.len(), KEEP_LOCAL, "{left:?}");
        for ms in [3_000_i64, 4_000, 5_000] {
            assert!(root.join(format!("app.pre-restore-{ms}.db")).exists(), "{ms} 该留下");
        }
        std::fs::remove_dir_all(&root).ok();
    }
}
