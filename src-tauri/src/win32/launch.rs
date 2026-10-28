//! 启动与「已运行则调到前台」。
//!
//! 用 `ShellExecuteExW`（不是 `Command("start")`）：管理员清单里的程序、.lnk、协议处理器
//! 都只有 ShellExecute 走得通，而且 `SEE_MASK_NOCLOSEPROCESS` 能拿到进程句柄去问 PID。
//!
//! 前台锁是 Windows 的规则，不是我们的 bug：后台进程不能随便抢前台。所以这里尽力调，
//! 抢不到就闪任务栏图标 —— 原生软件都这么做，比伪造一个「已置顶」提示诚实（§2 原则 5）。

use windows::core::{BOOL, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, TRUE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::Threading::GetProcessId;
use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, FlashWindowEx, GetWindowTextLengthW, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, SetForegroundWindow, ShowWindow, FLASHWINFO, FLASHW_ALL, SW_RESTORE,
    SW_SHOWNORMAL,
};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchOutcome {
    /// 新起一个进程，带 PID（拿不到 PID 时为 0）
    Started(u32),
    /// 本来就在跑，把它的窗口带到前台
    Raised,
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 按可执行文件名找正在跑的进程。用镜像名而不是记上次的 PID：
/// 那个进程早就退了，而用户可能自己已经开了这个程序。
pub fn running_pids_by_image(want: &str) -> Vec<u32> {
    let Ok(snapshot) = (unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    unsafe {
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        if Process32FirstW(snapshot, &mut entry as *mut _).is_ok() {
            loop {
                let name = String::from_utf16_lossy(&entry.szExeFile);
                let name = name.split('\0').next().unwrap_or_default();
                if name.eq_ignore_ascii_case(want) {
                    out.push(entry.th32ProcessID);
                }
                if Process32NextW(snapshot, &mut entry as *mut _).is_err() {
                    break;
                }
            }
        }
        let _ = CloseHandle(snapshot);
    }
    out
}

/// 这个可执行文件当前有没有进程在跑（用于磁贴上的「运行中」小点）。
pub fn is_image_running(want: &str) -> bool {
    !running_pids_by_image(want).is_empty()
}

struct RaiseCtx {
    pids: Vec<u32>,
    best: Option<HWND>,
    best_score: i32,
}

unsafe extern "system" fn pick_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let ctx = &mut *(lparam.0 as *mut RaiseCtx);
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if !ctx.pids.contains(&pid) || !IsWindowVisible(hwnd).as_bool() {
        return TRUE;
    }
    // 优先带标题的主窗口：工具窗口与隐藏的消息窗口都不该被带到前台
    let title_len = GetWindowTextLengthW(hwnd);
    if title_len == 0 {
        return TRUE;
    }
    let score = title_len + if IsIconic(hwnd).as_bool() { 0 } else { 50 };
    if score > ctx.best_score {
        ctx.best_score = score;
        ctx.best = Some(hwnd);
    }
    TRUE
}

/// 把某个进程的窗口带到前台。抢不到前台就闪任务栏，返回是否真的带起来了。
pub fn raise_window(pids: &[u32]) -> bool {
    if pids.is_empty() {
        return false;
    }
    let mut ctx = RaiseCtx {
        pids: pids.to_vec(),
        best: None,
        best_score: 0,
    };
    unsafe {
        let _ = EnumWindows(Some(pick_window), LPARAM(&mut ctx as *mut _ as isize));
        let Some(hwnd) = ctx.best else {
            return false;
        };
        let _ = ShowWindow(hwnd, SW_RESTORE);
        if SetForegroundWindow(hwnd).as_bool() {
            return true;
        }
        let info = FLASHWINFO {
            cbSize: std::mem::size_of::<FLASHWINFO>() as u32,
            hwnd,
            dwFlags: FLASHW_ALL,
            uCount: 3,
            dwTimeout: 0,
        };
        let _ = FlashWindowEx(&info);
        false
    }
}

/// 启动一个目标。`target` 是 exe / lnk / 文档 / URL 的绝对路径。
pub fn launch_or_raise(target: &str, args: &str) -> Result<LaunchOutcome> {
    let path = std::path::Path::new(target);
    // 只有程序类目标才做「已在运行就调到前台」；文档/URL 每次都该新开。
    // .lnk 不解析目标（不想为这个引 COM），退化成按快捷方式名猜镜像名：
    // Chrome.lnk → chrome.exe。猜不到就是再启动一次，浏览器自己会合并到已有实例。
    let image_to_find = match path.extension().and_then(|e| e.to_str()) {
        Some(ext) if ext.eq_ignore_ascii_case("exe") => {
            path.file_name().and_then(|n| n.to_str()).map(str::to_owned)
        }
        Some(ext) if ext.eq_ignore_ascii_case("lnk") => path
            .file_stem()
            .and_then(|n| n.to_str())
            .map(|stem| format!("{stem}.exe")),
        _ => None,
    };

    if let Some(name) = image_to_find {
        let alive = running_pids_by_image(&name);
        if !alive.is_empty() && raise_window(&alive) {
            return Ok(LaunchOutcome::Raised);
        }
    }

    let verb = to_wide("open");
    let file = to_wide(target);
    let params = to_wide(args);
    let dir = path
        .parent()
        .and_then(|p| p.to_str())
        .map(to_wide)
        .unwrap_or_else(|| vec![0]);

    unsafe {
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS,
            lpVerb: PCWSTR(verb.as_ptr()),
            lpFile: PCWSTR(file.as_ptr()),
            lpParameters: PCWSTR(params.as_ptr()),
            lpDirectory: PCWSTR(dir.as_ptr()),
            nShow: SW_SHOWNORMAL.0,
            ..Default::default()
        };
        ShellExecuteExW(&mut info)
            .map_err(|e| Error::msg(format!("启动失败：{e}。检查目标是否还在原来的位置")))?;

        let pid = if info.hProcess.is_invalid() {
            0
        } else {
            let id = GetProcessId(info.hProcess);
            let _ = CloseHandle(info.hProcess);
            id
        };
        Ok(LaunchOutcome::Started(pid))
    }
}

/// 打开一个 URL / 文件夹（走系统默认关联，不自己拉浏览器）。
pub fn open_shell(target: &str) -> Result<()> {
    launch_or_raise(target, "").map(|_| ())
}
