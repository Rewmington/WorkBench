//! `log` 只是接口，这一节是它的实现。
//!
//! 为什么不用 `env_logger`：release 是 GUI 程序（`main.rs` 的 `windows_subsystem`），
//! 没有控制台，stderr 无处可去。而 Settings 早已把 `paths.logs` 这个目录印给用户看 ——
//! 装了这一节，那句「日志」才不再是谎。
//!
//! 每行都即时 open/append：这个量级的日志不值得常驻句柄，而且崩溃时最后几行
//! 正是最要看的那几行。

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::Local;

use crate::state::Paths;

const FILE_NAME: &str = "anvil.log";
/// 到这个大小滚一代，留 `KEEP` 代。日备一天一次的节奏，这三代够查一周以上。
const MAX_BYTES: u64 = 2 * 1024 * 1024;
const KEEP: usize = 3;

/// 落盘目录。`install()` 之后、`use_dir()` 之前是 `None`，那时走 `startup_target()`
/// 自己算一遍（启动最早期的失败恰恰是最需要留证据的场合）。
static DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

pub fn install() {
    let filter = if cfg!(debug_assertions) {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };
    if log::set_logger(&FILE_LOG).is_ok() {
        log::set_max_level(filter);
    }
    // panic 默认只去 stderr，release 里等于没有：把它抄一份进日志。
    std::panic::set_hook(Box::new(|info| {
        eprintln!("{info}");
        write_line(&format!("PANIC  {info}"));
    }));
}

pub fn use_dir(dir: &Path) {
    if let Ok(mut guard) = DIR.lock() {
        *guard = Some(dir.to_path_buf());
    }
    log::info!("日志目录 {}", dir.display());
}

/// 启动失败：没有 UI、没有窗口，只能靠这个文件 + 非零退出码。
///
/// 但"只能靠日志文件"这一句在 2026-11-02 之前是**没人看见的**：release 是 GUI 程序，
/// 双击启动时 stderr 无处可去，于是库坏掉的表现就是"点开没反应"—— 与手机端批次 8 抓到的
/// N16 完全同一件事（那一版屏幕上也只剩启动图标，而最容易做的动作是卸载重装，
/// 那会连数据一起删）。这里补一个原生消息框：不引新依赖（`windows` crate 本来就在用，
/// `Win32_UI_WindowsAndMessaging` 也早开着），也不假装软件能自愈。
pub fn startup_failure(detail: &str) {
    eprintln!("Anvil 启动失败：{detail}");
    write_line(&format!("STARTUP-FAIL  {detail}"));
    tell_the_user(detail);
}

#[cfg(windows)]
fn tell_the_user(detail: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, MB_ICONERROR, MB_OK, MB_SETFOREGROUND,
    };

    let wide = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let where_to_look = Paths::local_app_data()
        .map(|p| format!("日志在：{}\\anvil.log", p.logs.display()))
        .unwrap_or_else(|_| "日志路径也读不出来：检查 %LOCALAPPDATA% 是否可用".to_owned());
    let text = wide(&format!(
        "Anvil 打不开本机的数据，进程已经退出。\n\n原因：{detail}\n\n\
         先别卸载重装 —— 卸载会连数据一起删掉。\n{where_to_look}"
    ));
    let caption = wide("Anvil 启动失败");
    // 没有窗口可归属也要弹：这一步的目的就是"让人看见"。MB_SETFOREGROUND 保证它
    // 不会被别的窗口压在下面（这时根本没有自己的窗口可以置顶）。
    unsafe {
        let _ = MessageBoxW(
            None,
            PCWSTR(text.as_ptr()),
            PCWSTR(caption.as_ptr()),
            MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
        );
    }
}

/// 非 Windows 平台上这一版本没有原生对话框可依赖，行为与以前一致（只落日志）。
#[cfg(not(windows))]
fn tell_the_user(_detail: &str) {}

/// debug 构建里同时打 stderr，`tauri dev` 的终端才看得见；release 没有控制台，跳过。
fn mirror_to_stderr(line: &str) {
    #[cfg(debug_assertions)]
    eprint!("{line}");
    #[cfg(not(debug_assertions))]
    let _ = line;
}

fn startup_target() -> Option<PathBuf> {
    let paths = Paths::local_app_data().ok()?;
    fs::create_dir_all(&paths.logs).ok()?;
    Some(paths.logs.join(FILE_NAME))
}

fn write_line(text: &str) {
    let dir = DIR.lock().ok().and_then(|g| g.clone());
    let file = match dir {
        Some(dir) => dir.join(FILE_NAME),
        None => match startup_target() {
            Some(file) => file,
            // 连数据目录都解析不出、临时目录也写不动时，只能放弃留证据。
            None => return,
        },
    };
    if let Some(parent) = file.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }
    let stamped = format!("{} {}\n", Local::now().format("%Y-%m-%d %H:%M:%S%.3f"), text);
    if rotate_if_big(&file).is_err() {
        return;
    }
    if let Ok(mut handle) = OpenOptions::new().create(true).append(true).open(&file) {
        // 写失败没有地方可报：日志本身出错时，唯一诚实的做法是别再产生日志。
        let _ = handle.write_all(stamped.as_bytes());
    }
    mirror_to_stderr(&stamped);
}

/// 第 `gen` 代的路径：0 代是真文件，之后是 `.1`/`.2`。
fn gen_path(file: &Path, gen: usize) -> PathBuf {
    if gen == 0 {
        return file.to_path_buf();
    }
    PathBuf::from(format!("{}.{}", file.display(), gen))
}

/// 真文件 → `.1` → `.2` 依次后移，超出 `KEEP` 层的最老一代删掉。
fn rotate_if_big(file: &Path) -> std::io::Result<()> {
    let size = fs::metadata(file).map(|m| m.len()).unwrap_or(0);
    if size < MAX_BYTES {
        return Ok(());
    }
    let _ = fs::remove_file(gen_path(file, KEEP - 1));
    for gen in (1..KEEP).rev() {
        // 源不存在时这一步没东西可挪，失败也无需声张（下面紧接着会新建真文件）。
        let _ = fs::rename(gen_path(file, gen - 1), gen_path(file, gen));
    }
    Ok(())
}

/// `log::set_logger` 要 `'static`，而 `set_boxed_logger` 又挂在 `alloc` feature 上：
/// 空结构体 + 引用比为了装日志器多开一个 feature 干净。
struct FileLog;
static FILE_LOG: FileLog = FileLog;

impl log::Log for FileLog {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        write_line(&format!(
            "{:>5} {} — {}",
            record.level(),
            record.target(),
            record.args()
        ));
    }

    /// 每条都即时落盘了，这里没有缓冲要冲。
    fn flush(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 合成一条：写进去 → 涨到上限就后移 → 腾出的真文件能被下一条重新写上。
    /// 合成一条而不是三条，是因为 `DIR` 是全局的，并行跑的测试会互相改写它。
    #[test]
    fn write_rotate_and_resume() {
        let dir = std::env::temp_dir().join(format!("anvil-log-test-{}", uuid::Uuid::now_v7()));
        fs::create_dir_all(&dir).unwrap();
        use_dir(&dir);
        let file = dir.join(FILE_NAME);

        // 1) 一行日志要真的落盘，且行首带时间戳。
        write_line("FIRST");
        let text = fs::read_to_string(&file).expect("日志文件应该被写出来");
        assert!(text.contains("FIRST"), "落盘的行要带内容：{text}");
        assert!(
            text.chars().next().unwrap().is_ascii_digit(),
            "行首要打时间戳：{text}"
        );

        // 2) 涨过上限：真文件让位给 .1，旧 .1 让位给 .2，最老那一代消失。
        fs::write(gen_path(&file, 1), b"one").unwrap();
        fs::write(gen_path(&file, 2), b"two").unwrap();
        fs::write(&file, vec![b'b'; MAX_BYTES as usize + 1]).unwrap();
        rotate_if_big(&file).unwrap();

        assert!(!file.exists(), "轮转就是把真文件挪走，下一条再新建");
        assert_eq!(fs::read(gen_path(&file, 1)).unwrap()[0], b'b', "真文件挪成了 .1");
        assert_eq!(fs::read(gen_path(&file, 2)).unwrap(), b"one", "旧的 .1 挪成 .2");

        // 3) 后移之后还能接着写，且不会攒出第四代。
        write_line("SECOND");
        assert!(fs::read_to_string(&file).unwrap().contains("SECOND"));
        assert!(
            !gen_path(&file, KEEP).exists(),
            "留证据的层数必须封顶在 {KEEP}"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
