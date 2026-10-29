//! 系统资源部件的数据源：CPU 占用与物理内存。
//!
//! CPU 百分比要**两次采样作差**才出得来 —— `GetSystemTimes` 给的是开机以来的累计
//! 时间，单次调用什么也算不出。所以第一次调用返回 `None`，界面那一格先空着，
//! 不拿 0% 假装「机器很闲」（§2 原则 5）。前端 1 秒 tick 调一次，第二次起就有数。

use std::sync::Mutex;
use std::time::Instant;

use windows::Win32::Foundation::FILETIME;
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;

use crate::model::SysLoad;

#[derive(Clone, Copy)]
struct Sample {
    idle: u64,
    kernel: u64,
    user: u64,
}

/// 上一次采样的时刻（单调钟，不受改系统时间影响）与累计值。
static PREV: Mutex<Option<(Sample, Instant)>> = Mutex::new(None);

fn to_u64(ft: FILETIME) -> u64 {
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

fn cpu_percent() -> Option<f64> {
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    let ok = unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)) };
    if ok.is_err() {
        return None;
    }
    let cur = Sample {
        idle: to_u64(idle),
        kernel: to_u64(kernel),
        user: to_u64(user),
    };
    let mut guard = PREV.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let prev = guard.replace((cur, Instant::now()));
    let prev = prev?.0;

    // kernel 时间已经包含 idle，所以 total = kernel + user
    let total = cur.kernel.checked_add(cur.user)?.checked_sub(prev.kernel + prev.user)?;
    let busy = total.checked_sub(cur.idle.checked_sub(prev.idle)?)?;
    if total == 0 {
        return None;
    }
    Some((busy as f64 / total as f64 * 100.0).clamp(0.0, 100.0))
}

fn memory() -> Option<(u64, u64)> {
    let mut status = MEMORYSTATUSEX {
        dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
        ..Default::default()
    };
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    if ok.is_err() {
        return None;
    }
    Some((status.ullTotalPhys, status.ullAvailPhys))
}

pub fn sample_load() -> SysLoad {
    let (mem_total, mem_avail) = memory().unwrap_or((0, 0));
    SysLoad {
        cpu_percent: cpu_percent(),
        mem_total,
        mem_avail,
    }
}
