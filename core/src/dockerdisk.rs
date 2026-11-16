//! Compacts Docker Desktop's Hyper-V virtual disk (VHDX). The file only grows; this hands unused space back.
//! Nothing inside the disk is deleted. The Docker VM must be off, so Docker Desktop is stopped first.
//!
//! Permissions: Docker often restricts its disk file to its own service account, so even an administrator cannot
//! open it. This module never changes permissions or ownership of a VM disk and never borrows another account's
//! rights. If the disk cannot be opened it says so up front, before it stops anything.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::{fsx, procs, winenv};

const DISK: &str = "C:\\ProgramData\\DockerDesktop\\vm-data\\DockerDesktop.vhdx";
const VM_NAME: &str = "DockerDesktopVM";

pub const NO_ACCESS: &str = "Docker restricts this disk file to its own service account, so it cannot be compacted from outside. Compact it from Docker Desktop instead.";

/// Docker virtual disks that exist on this PC, with their current size.
pub fn disks() -> Vec<(PathBuf, u64)> {
    let p = PathBuf::from(DISK);
    match std::fs::metadata(&p) {
        Ok(m) if m.is_file() && !fsx::is_excluded(&p) => vec![(p, m.len())],
        _ => vec![],
    }
}

/// True when this account can open the disk file at all (read-only is enough for Optimize-VHD to work).
pub fn can_open(path: &std::path::Path) -> bool {
    std::fs::File::open(path).is_ok()
}

fn powershell(script: &str) -> Option<std::process::Output> {
    winenv::run("powershell", &["-NoProfile", "-NonInteractive", "-Command", script]).ok()
}

/// State of Docker Desktop's Hyper-V VM: Some("Off" | "Running" | ...) or None when there is no such VM.
fn vm_state() -> Option<String> {
    let out = powershell(&format!("(Get-VM -Name '{VM_NAME}' -ErrorAction SilentlyContinue).State"))?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn optimize_vhd_available() -> bool {
    powershell("[bool](Get-Command Optimize-VHD -ErrorAction SilentlyContinue)").map(|o| String::from_utf8_lossy(&o.stdout).trim().eq_ignore_ascii_case("true")).unwrap_or(false)
}

fn optimize_vhd(path: &str) -> Result<(), String> {
    let script = format!("$ErrorActionPreference='Stop'; try {{ Optimize-VHD -Path '{path}' -Mode Full }} catch {{ [Console]::Error.WriteLine($_.Exception.Message); exit 1 }}");
    match powershell(&script) {
        Some(o) if o.status.success() => Ok(()),
        Some(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        None => Err("could not start PowerShell".into()),
    }
}

/// Returns (path, size before, size after). Requires Administrator.
pub fn compact(stage: &mut dyn FnMut(String)) -> Result<Vec<(String, u64, u64)>, String> {
    let list = disks();
    if list.is_empty() {
        return Err("no Docker virtual disk found".into());
    }
    // Check access BEFORE touching Docker, so a refusal never costs the user a running Docker session.
    if let Some((p, _)) = list.iter().find(|(p, _)| !can_open(p)) {
        return Err(format!("{NO_ACCESS} ({})", p.display()));
    }
    if !optimize_vhd_available() {
        return Err("Hyper-V's Optimize-VHD is not available on this PC".into());
    }
    stage("Stopping Docker Desktop".into());
    procs::kill_by_names(&["Docker Desktop.exe", "com.docker.backend.exe", "com.docker.proxy.exe", "com.docker.dev-envs.exe"]);
    let _ = winenv::run("net", &["stop", "com.docker.service"]);
    if let Some(state) = vm_state() {
        if !state.eq_ignore_ascii_case("off") {
            stage(format!("Shutting down the {VM_NAME} virtual machine"));
            let _ = powershell(&format!("Stop-VM -Name '{VM_NAME}' -Force -ErrorAction SilentlyContinue"));
            let t0 = Instant::now();
            while vm_state().map(|s| !s.eq_ignore_ascii_case("off")).unwrap_or(false) {
                if t0.elapsed() > Duration::from_secs(120) {
                    return Err(format!("the {VM_NAME} virtual machine would not shut down"));
                }
                std::thread::sleep(Duration::from_secs(3));
            }
        }
    }
    let mut results = Vec::new();
    for (path, before) in list {
        let p = path.display().to_string();
        stage(format!("Compacting {p} (this can take several minutes)"));
        optimize_vhd(&p).map_err(|e| if e.to_lowercase().contains("denied") || e.to_lowercase().contains("permission") { NO_ACCESS.to_string() } else { format!("could not compact {p}: {e}") })?;
        let after = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(before);
        results.push((p, before, after));
    }
    Ok(results)
}
