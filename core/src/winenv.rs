//! Thin, safe wrappers around the Win32 bits we need.

use std::os::windows::process::CommandExt;
use std::process::{Command, Output};

use serde::Serialize;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY};
use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives};
use windows_sys::Win32::System::Console::{
    GetConsoleMode, GetStdHandle, SetConsoleMode, SetConsoleOutputCP, ENABLE_VIRTUAL_TERMINAL_PROCESSING,
    STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::ProcessStatus::EmptyWorkingSet;
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_INFORMATION, PROCESS_SET_QUOTA,
};
use windows_sys::Win32::UI::Shell::{SHEmptyRecycleBinW, SHQueryRecycleBinW, ShellExecuteW, SHQUERYRBINFO};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const DRIVE_FIXED: u32 = 3;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Enable UTF-8 + ANSI escape handling so box drawing and colours render everywhere.
pub fn enable_vt() {
    unsafe {
        SetConsoleOutputCP(65001);
        let h = GetStdHandle(STD_OUTPUT_HANDLE);
        let mut mode = 0u32;
        if GetConsoleMode(h, &mut mode) != 0 {
            SetConsoleMode(h, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
        }
    }
}

pub fn is_admin() -> bool {
    unsafe {
        let mut token: HANDLE = 0;
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }
        let mut elev = TOKEN_ELEVATION { TokenIsElevated: 0 };
        let mut ret = 0u32;
        let ok = GetTokenInformation(
            token,
            TokenElevation,
            &mut elev as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut ret,
        );
        CloseHandle(token);
        ok != 0 && elev.TokenIsElevated != 0
    }
}

/// Re-launch this executable through the UAC prompt with exactly `cli_args`.
/// Returns true if Windows accepted the request (the user clicked Yes).
pub fn relaunch_as_admin(cli_args: &[&str]) -> bool {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return false,
    };
    let mut args: Vec<String> = vec!["--elevated".into()];
    args.extend(cli_args.iter().map(|s| s.to_string()));
    let params = args
        .iter()
        .map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() })
        .collect::<Vec<_>>()
        .join(" ");
    let r = unsafe {
        ShellExecuteW(
            0,
            wide("runas").as_ptr(),
            wide(&exe.to_string_lossy()).as_ptr(),
            wide(&params).as_ptr(),
            std::ptr::null(),
            1,
        )
    };
    r > 32
}

/// Expand %VAR% references. Returns None if any variable is undefined (so we never build a bogus path).
pub fn expand(tpl: &str) -> Option<String> {
    let mut out = String::new();
    let mut rest = tpl;
    while let Some(i) = rest.find('%') {
        out.push_str(&rest[..i]);
        let after = &rest[i + 1..];
        let j = after.find('%')?;
        let name = &after[..j];
        out.push_str(&std::env::var(name).ok()?);
        rest = &after[j + 1..];
    }
    out.push_str(rest);
    Some(out)
}

#[derive(Clone, Debug, Serialize)]
pub struct DriveInfo {
    pub root: String,
    pub total: u64,
    pub free: u64,
}

pub fn disk_space(root: &str) -> Option<(u64, u64)> {
    let (mut avail, mut total, mut free) = (0u64, 0u64, 0u64);
    let ok = unsafe { GetDiskFreeSpaceExW(wide(root).as_ptr(), &mut avail, &mut total, &mut free) };
    (ok != 0).then_some((total, avail))
}

pub fn fixed_drives() -> Vec<DriveInfo> {
    let mask = unsafe { GetLogicalDrives() };
    let mut v = Vec::new();
    for i in 0..26u32 {
        if mask & (1 << i) == 0 {
            continue;
        }
        let root = format!("{}:\\", (b'A' + i as u8) as char);
        if unsafe { GetDriveTypeW(wide(&root).as_ptr()) } != DRIVE_FIXED {
            continue;
        }
        if let Some((total, free)) = disk_space(&root) {
            v.push(DriveInfo { root, total, free });
        }
    }
    v
}

pub fn recycle_bin_info() -> Option<(u64, u64)> {
    let mut info = SHQUERYRBINFO { cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32, i64Size: 0, i64NumItems: 0 };
    let hr = unsafe { SHQueryRecycleBinW(std::ptr::null(), &mut info) };
    (hr == 0).then_some((info.i64Size.max(0) as u64, info.i64NumItems.max(0) as u64))
}

pub fn empty_recycle_bin() -> bool {
    // SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND
    let hr = unsafe { SHEmptyRecycleBinW(0, std::ptr::null(), 0x7) };
    // S_FALSE (1) is returned when the bin was already empty
    hr == 0 || hr == 1
}

/// Ask Windows to page out as much of a process's working set as possible. Harmless: pages fault back in on demand.
pub fn trim_working_set(pid: u32) -> bool {
    unsafe {
        let h = OpenProcess(PROCESS_SET_QUOTA | PROCESS_QUERY_INFORMATION, 0, pid);
        if h == 0 {
            return false;
        }
        let ok = EmptyWorkingSet(h) != 0;
        CloseHandle(h);
        ok
    }
}

/// Run a program without flashing a console window and capture its output.
pub fn run(program: &str, args: &[&str]) -> std::io::Result<Output> {
    Command::new(program).args(args).creation_flags(CREATE_NO_WINDOW).output()
}

pub fn run_ok(program: &str, args: &[&str]) -> bool {
    run(program, args).map(|o| o.status.success()).unwrap_or(false)
}

pub fn run_text(program: &str, args: &[&str]) -> Option<String> {
    let o = run(program, args).ok()?;
    Some(String::from_utf8_lossy(&o.stdout).to_string())
}

pub fn command_exists(program: &str) -> bool {
    Command::new("where").arg(program).creation_flags(CREATE_NO_WINDOW).output().map(|o| o.status.success()).unwrap_or(false)
}
