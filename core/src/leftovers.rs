//! Possible leftovers of uninstalled apps in AppData and ProgramData. REPORT ONLY: this module never deletes.
//! A folder is listed when no installed program, Store app, program folder or running process matches its name.
//! That is a heuristic, so the UI presents these as candidates and only offers "Show in Explorer" / Recycle Bin.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rayon::prelude::*;
use serde::Serialize;
use winreg::enums::*;
use winreg::RegKey;

use crate::fsx;

#[derive(Serialize, Clone, Debug)]
pub struct Leftover {
    pub path: String,
    pub name: String,
    pub place: &'static str,
    pub bytes: u64,
    pub age_days: u64,
    pub why: String,
}

pub fn norm_token(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase()
}

/// `token` is a folder name, `hay` the identifiers of everything installed (all normalised).
pub fn matches(token: &str, hay: &[String]) -> bool {
    token.len() >= 3 && hay.iter().any(|h| h.len() >= 4 && (h.contains(token) || (h.len() >= 5 && token.contains(h.as_str()))))
}

const SKIP: &[&str] = &[
    "microsoft", "windows", "packages", "temp", "programs", "connecteddevicesplatform", "publishers", "comms", "history", "vs", "sapujagat",
    "id.sapujagat.app", "d3dscache", "crashdumps", "placeholdertilelogofolder", "diagnostics", "tempstate", "application data",
    "package cache", "ssh", "regid", "usoshared", "usoprivate",
];

/// Folders that belong to Windows or to this app are never reported.
pub fn skipped(low: &str) -> bool {
    SKIP.contains(&low) || low.starts_with("microsoft") || low.starts_with("windows") || low.starts_with("regid") || low.starts_with('.') || low.starts_with('$')
}

fn installed_identifiers() -> Vec<String> {
    let mut hay: Vec<String> = Vec::new();
    let mut add = |s: &str| {
        let t = norm_token(s);
        if t.len() >= 3 {
            hay.push(t);
        }
    };
    for (hive, path) in [
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall"),
        (HKEY_CURRENT_USER, r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall"),
    ] {
        let Ok(k) = RegKey::predef(hive).open_subkey(path) else { continue };
        for name in k.enum_keys().flatten() {
            let Ok(sk) = k.open_subkey(&name) else { continue };
            add(&name);
            for v in ["DisplayName", "Publisher"] {
                if let Ok(s) = sk.get_value::<String, _>(v) {
                    add(&s);
                    if v == "DisplayName" {
                        if let Some(first) = s.split(|c: char| !c.is_alphanumeric()).find(|w| w.len() >= 4) {
                            add(first);
                        }
                    }
                }
            }
            if let Ok(loc) = sk.get_value::<String, _>("InstallLocation") {
                if let Some(l) = Path::new(loc.trim_end_matches('\\')).file_name() {
                    add(&l.to_string_lossy());
                }
            }
            if let Ok(icon) = sk.get_value::<String, _>("DisplayIcon") {
                let p = icon.trim_matches('"').split(',').next().unwrap_or("").to_string();
                if let Some(stem) = Path::new(&p).file_stem() {
                    add(&stem.to_string_lossy());
                }
            }
        }
    }
    let mut dirs: Vec<PathBuf> = vec![PathBuf::from("C:\\Program Files"), PathBuf::from("C:\\Program Files (x86)")];
    for tpl in ["%LOCALAPPDATA%\\Packages", "%LOCALAPPDATA%\\Programs", "%ProgramData%\\Microsoft\\Windows\\Start Menu\\Programs", "%APPDATA%\\Microsoft\\Windows\\Start Menu\\Programs"] {
        if let Some(p) = crate::winenv::expand(tpl) {
            dirs.push(PathBuf::from(p));
        }
    }
    for d in dirs {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                add(&e.file_name().to_string_lossy());
            }
        }
    }
    let mut sys = sysinfo::System::new();
    sys.refresh_processes();
    for p in sys.processes().values() {
        add(p.name().trim_end_matches(".exe"));
    }
    hay.sort();
    hay.dedup();
    hay
}

fn age_days(p: &Path) -> u64 {
    std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|m| SystemTime::now().duration_since(m).ok()).map(|d| d.as_secs() / 86_400).unwrap_or(0)
}

pub fn find() -> Vec<Leftover> {
    let hay = installed_identifiers();
    let mut cands: Vec<(PathBuf, String, &'static str)> = Vec::new();
    for (place, tpl) in [("AppData (Local)", "%LOCALAPPDATA%"), ("AppData (Roaming)", "%APPDATA%"), ("ProgramData", "%ProgramData%")] {
        let Some(root) = crate::winenv::expand(tpl) else { continue };
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for e in rd.flatten() {
            let Ok(md) = e.metadata() else { continue };
            if !md.is_dir() || fsx::is_link(&md) {
                continue;
            }
            let name = e.file_name().to_string_lossy().to_string();
            let low = name.to_lowercase();
            if skipped(&low) {
                continue;
            }
            if matches(&norm_token(&name), &hay) || fsx::is_excluded(&e.path()) || fsx::guard_opts(&e.path(), true).is_err() {
                continue;
            }
            cands.push((e.path(), name, place));
        }
    }
    let mut out: Vec<Leftover> = cands
        .par_iter()
        .filter_map(|(path, name, place)| {
            let bytes = fsx::measure_tree(path).bytes;
            if bytes < 5 * 1024 * 1024 {
                return None;
            }
            let age = age_days(path);
            Some(Leftover { path: path.display().to_string(), name: name.clone(), place, bytes, age_days: age, why: format!("No installed program, Store app or running process matches \"{name}\". Last changed {age} days ago.") })
        })
        .collect();
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    out.truncate(80);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_and_own_folders_are_never_reported() {
        for n in ["id.sapujagat.app", "sapujagat", "usoshared", "regid.1991-06.com.microsoft", "microsoft", "windowsapps", ".cache", "$recycle.bin"] {
            assert!(skipped(n), "{n} should be skipped");
        }
        assert!(!skipped("dbeaverdata"));
    }

    #[test]
    fn matching_is_substring_both_ways_and_ignores_short_noise() {
        let hay = vec!["brave".to_string(), "docker".to_string(), "nvidiacorporation".to_string()];
        assert!(matches(&norm_token("BraveSoftware"), &hay));
        assert!(matches(&norm_token("NVIDIA"), &hay));
        assert!(matches(&norm_token("Docker Desktop"), &hay));
        assert!(!matches(&norm_token("OrphanedVendorStuff"), &hay));
        assert!(!matches("ab", &hay));
    }
}
