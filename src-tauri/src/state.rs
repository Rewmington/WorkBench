use std::path::PathBuf;
use std::sync::Arc;

use rusqlite::Connection;

use crate::error::{Error, Result};

/// PLAN §5.0：这一层不可改 —— 它决定数据目录与 AppUserModelID。
pub const IDENTIFIER: &str = "dev.rewmington.anvil";

/// 数据一律落 `%LOCALAPPDATA%\dev.rewmington.anvil\`，不用 Tauri 默认的 appDataDir（那是
/// Roaming 目录）。两个真实风险：域控环境下 Roaming 随登录漫游，WAL 在复制中途被读就是损坏；
/// 更常见的是有人把 %APPDATA% 放进了 OneDrive 同步范围 —— SQLite + 云同步盘是经典损坏路径。
#[derive(Debug, Clone)]
pub struct Paths {
    pub root: PathBuf,
    pub db: PathBuf,
    pub icons: PathBuf,
    pub logs: PathBuf,
    pub tmp: PathBuf,
    /// 备份快照单独放一个子目录，方便整体排除。
    pub backups: PathBuf,
    /// 设备 id 存文件、不存库：备份含库，整文件恢复不能把旧机器的身份复活。
    pub device_id: PathBuf,
}

impl Paths {
    pub fn under(root: PathBuf) -> Self {
        let join = |name: &str| root.join(name);
        Self {
            root: root.clone(),
            db: join("app.db"),
            icons: join("icons"),
            logs: join("logs"),
            tmp: join("tmp"),
            backups: join("backups"),
            device_id: join("device.id"),
        }
    }

    /// 直接读环境变量而不是走 Tauri 的 BaseDirectory，语义由我们自己说了算，
    /// 也好在测试里传临时目录。
    pub fn local_app_data() -> Result<Self> {
        let local = std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .map_err(|_| Error::msg("环境变量 LOCALAPPDATA 不可用"))?;
        let root = local.join(IDENTIFIER);
        // 双保险：绝不允许落到 Roaming（%APPDATA%）下。
        if let Ok(roaming) = std::env::var("APPDATA") {
            let roaming = PathBuf::from(roaming);
            if root.starts_with(&roaming) {
                return Err(Error::msg("数据目录解析到了 Roaming，已拒绝启动"));
            }
        }
        Ok(Self::under(root))
    }

    pub fn ensure(&self) -> Result<()> {
        for dir in [
            &self.root,
            &self.icons,
            &self.logs,
            &self.tmp,
            &self.backups,
        ] {
            std::fs::create_dir_all(dir)?;
        }
        Ok(())
    }
}

/// 原样显示绝对路径就是 §2 原则 4「数据位置可感知」的落地。
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PathsView {
    pub root: String,
    pub db: String,
    pub icons: String,
    pub logs: String,
    pub backups: String,
    pub device_id: String,
    pub journal_mode: String,
}

impl Paths {
    pub fn view(&self, journal_mode: &str) -> PathsView {
        let s = |p: &PathBuf| p.display().to_string();
        PathsView {
            root: s(&self.root),
            db: s(&self.db),
            icons: s(&self.icons),
            logs: s(&self.logs),
            backups: s(&self.backups),
            device_id: s(&self.device_id),
            journal_mode: journal_mode.to_owned(),
        }
    }
}

pub struct AppState {
    /// 纪律：碰 DB 的命令一律写同步 `fn`，跨 `.await` 不持锁；repos 只收 `&Connection`。
    pub db: std::sync::Mutex<Connection>,
    /// 当前真正注册进系统的全局组合键。
    ///
    /// 单独立一份而不是在回调里读库：快捷键回调在 Windows 上跑于**主线程**
    /// （`global-hotkey` 的隐藏窗口建在主线程，WM_HOTKEY 同步派发），而同步那一整段
    /// 网络 IO 都持着 `db` 那把锁（P1-13 的第 2 步明确没做）。回调去读库等于把整个 UI
    /// 事件循环冻在锁上 —— 而用户恰恰是在窗口叫不上来的时候才按快捷键。
    pub hotkey: std::sync::Mutex<String>,
    pub paths: Paths,
    pub device_id: Arc<str>,
    /// 首次「关窗口 = 收进托盘」那句提示的状态机：0 还没收过、1 收了但话还没说出口、
    /// 2 已经说过（之后不再打扰）。为什么要中间那一档：藏起来的页面里 toast 计时器照跑，
    /// hide 之后立刻发就等于没人看见，得等窗口再出现时才补发（见 `ui::windows`）。
    pub tray_hint: std::sync::atomic::AtomicU8,
    /// 备份连续失败次数，决定下一次退避多久。
    pub backup_failures: std::sync::atomic::AtomicU32,
}

impl AppState {
    pub fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
