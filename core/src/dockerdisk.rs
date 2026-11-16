//! Compacts Docker Desktop's virtual disk (VHDX). The file only grows; this hands unused space back to Windows.
//! Nothing inside the disk is deleted. Docker Desktop is stopped first because the disk must not be in use.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::{fsx, procs, winenv};

const TEMPLATES: &[&str] = &[
    "C:\\ProgramData\\DockerDesktop\\vm-data\\DockerDesktop.vhdx",
    "%LOCALAPPDATA%\\Docker\\wsl\\disk\\docker_data.vhdx",
    "%LOCALAPPDATA%\\Docker\\wsl\\data\\ext4.vhdx",
    "%LOCALAPPDATA%\\Docker\\wsl\\main\\ext4.vhdx",
];

/// Docker virtual disks that exist on this PC, with their current size.
pub fn disks() -> Vec<(PathBuf, u64)> {
    let mut v = Vec::new();
    for t in TEMPLATES {
        for p in fsx::resolve(t) {
            if let Ok(m) = std::fs::metadata(&p) {
                if m.is_file() && !fsx::is_excluded(&p) {
                    v.push((p, m.len()));
                }
            }
        }
    }
    v
}

fn is_locked(p: &std::path::Path) -> bool {
    std::fs::OpenOptions::new().read(true).write(true).open(p).is_err()
}

fn diskpart(commands: &[String]) -> String {
    let script = std::env::temp_dir().join(format!("sapu-diskpart-{}.txt", std::process::id()));
    let _ = std::fs::write(&script, commands.join("\r\n"));
    let out = winenv::run_text("diskpart", &["/s", &script.to_string_lossy()]).unwrap_or_default();
    let _ = std::fs::remove_file(&script);
    out
}

/// Returns (path, size before, size after) for each disk. Requires Administrator.
pub fn compact(stage: &mut dyn FnMut(String)) -> Result<Vec<(String, u64, u64)>, String> {
    let list = disks();
    if list.is_empty() {
        return Err("no Docker virtual disk found".into());
    }
    stage("Stopping Docker Desktop".into());
    procs::kill_by_names(&["Docker Desktop.exe", "com.docker.backend.exe", "com.docker.proxy.exe", "com.docker.dev-envs.exe"]);
    let _ = winenv::run("net", &["stop", "com.docker.service"]);
    if winenv::command_exists("wsl") {
        let _ = winenv::run("wsl", &["--shutdown"]);
    }
    let mut results = Vec::new();
    for (path, before) in list {
        let t0 = Instant::now();
        while is_locked(&path) {
            if t0.elapsed() > Duration::from_secs(90) {
                return Err(format!("{} is still in use. Quit Docker Desktop completely and try again.", path.display()));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        stage(format!("Compacting {} (this can take several minutes)", path.display()));
        let p = path.display().to_string();
        let out = diskpart(&[format!("select vdisk file=\"{p}\""), "attach vdisk readonly".into(), "compact vdisk".into(), "detach vdisk".into(), "exit".into()]);
        if !out.to_lowercase().contains("successfully compacted") {
            let _ = diskpart(&[format!("select vdisk file=\"{p}\""), "detach vdisk".into(), "exit".into()]);
            return Err(format!("compaction of {p} did not complete"));
        }
        let after = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(before);
        results.push((p, before, after));
    }
    Ok(results)
}
