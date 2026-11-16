//! Read-only disk analyzer: where did my space go, and what is safe to do about it?

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::fsx;
use crate::rules::{self, Kind};

#[derive(Serialize, Clone)]
pub struct Item {
    pub path: PathBuf,
    pub name: String,
    pub bytes: u64,
    pub is_dir: bool,
}

#[derive(Serialize, Clone)]
pub struct BigFile {
    pub path: PathBuf,
    pub bytes: u64,
}

#[derive(Serialize)]
pub struct Analysis {
    pub root: PathBuf,
    pub items: Vec<Item>,
    pub big_files: Vec<BigFile>,
    pub total: u64,
    pub unreadable: u64,
}

pub fn analyze_dir(root: &Path, big_min: u64) -> Analysis {
    let children: Vec<PathBuf> = std::fs::read_dir(root).map(|rd| rd.flatten().map(|e| e.path()).collect()).unwrap_or_default();
    let big: Mutex<Vec<BigFile>> = Mutex::new(Vec::new());
    let unreadable = AtomicU64::new(0);

    let name_of = |p: &Path| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string());

    let mut items: Vec<Item> = children
        .par_iter()
        .map(|p| {
            let name = name_of(p);
            let md = match std::fs::symlink_metadata(p) {
                Ok(m) => m,
                Err(_) => {
                    unreadable.fetch_add(1, Ordering::Relaxed);
                    return Item { path: p.clone(), name, bytes: 0, is_dir: false };
                }
            };
            if fsx::is_link(&md) {
                return Item { path: p.clone(), name, bytes: 0, is_dir: md.is_dir() };
            }
            if md.is_file() {
                if md.len() >= big_min {
                    big.lock().unwrap().push(BigFile { path: p.clone(), bytes: md.len() });
                }
                return Item { path: p.clone(), name, bytes: md.len(), is_dir: false };
            }
            let mut sum = 0u64;
            for e in WalkDir::new(p).follow_links(false).into_iter() {
                match e {
                    Ok(e) => {
                        if e.file_type().is_file() {
                            if let Ok(m) = e.metadata() {
                                sum += m.len();
                                if m.len() >= big_min {
                                    big.lock().unwrap().push(BigFile { path: e.path().to_path_buf(), bytes: m.len() });
                                }
                            }
                        }
                    }
                    Err(_) => {
                        unreadable.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
            Item { path: p.clone(), name, bytes: sum, is_dir: true }
        })
        .collect();

    items.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    let mut big_files = big.into_inner().unwrap();
    big_files.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    big_files.truncate(15);
    let total = items.iter().map(|i| i.bytes).sum();
    Analysis { root: root.to_path_buf(), items, big_files, total, unreadable: unreadable.load(Ordering::Relaxed) }
}

/// Every concrete directory/file that a cleaning rule targets, with the rule name.
fn cleanable_map() -> Vec<(String, &'static str)> {
    let mut v = Vec::new();
    for r in rules::catalog() {
        if let Kind::Paths(ts) = &r.kind {
            for t in ts {
                for p in fsx::resolve(&t.path) {
                    v.push((p.to_string_lossy().to_lowercase(), r.name));
                }
            }
        }
    }
    v
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum TagKind {
    /// This path is (or is inside) something a cleaning rule handles.
    Cleanable,
    /// A cleanable location lives somewhere below this path.
    Contains,
    /// Windows / driver files: leave alone.
    Danger,
    /// Informational (installed apps, app data).
    Info,
    /// Your own files: review manually.
    Review,
    /// Managed by Windows (page file, restore points, Recycle Bin).
    Warn,
    /// Protected by this tool (e.g. Claude app data).
    Protected,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Tag {
    pub text: String,
    pub kind: TagKind,
}

pub struct Annotator {
    clean: Vec<(String, &'static str)>,
}

impl Default for Annotator {
    fn default() -> Self {
        Self::new()
    }
}

impl Annotator {
    pub fn new() -> Self {
        Annotator { clean: cleanable_map() }
    }

    pub fn tag(&self, p: &Path) -> Option<Tag> {
        let s = p.to_string_lossy().to_lowercase();
        let trimmed = s.trim_end_matches('\\');
        // relationship between this path and the cleaning rules
        let mut inside: Option<&str> = None;
        let mut contains: Option<&str> = None;
        for (cp, name) in &self.clean {
            if cp == trimmed || (cp.starts_with(trimmed) && cp.as_bytes().get(trimmed.len()) == Some(&b'\\')) {
                if cp == trimmed {
                    inside = Some(name);
                } else if contains.is_none() {
                    contains = Some(name);
                }
            } else if trimmed.starts_with(cp.as_str()) && trimmed.as_bytes().get(cp.len()) == Some(&b'\\') {
                inside = Some(name);
            }
        }
        let profile = std::env::var("USERPROFILE").unwrap_or_default().to_lowercase();
        let tag = |t: &str, k: TagKind| Some(Tag { text: t.to_string(), kind: k });
        if trimmed.ends_with("\\windows") {
            return tag("Windows — do not touch", TagKind::Danger);
        }
        if trimmed.ends_with("\\winsxs") {
            return tag("Windows component store (use DISM cleanup)", TagKind::Danger);
        }
        if trimmed.ends_with("\\driverstore") || trimmed.contains("\\driverstore\\") {
            return tag("Driver store — do not touch", TagKind::Danger);
        }
        if trimmed.contains("\\program files") && trimmed.matches('\\').count() <= 1 {
            return tag("Installed apps", TagKind::Info);
        }
        if trimmed.ends_with("\\programdata") {
            return tag("Shared app data", TagKind::Info);
        }
        if trimmed.ends_with("\\system volume information") {
            return tag("Restore points / shadow copies", TagKind::Warn);
        }
        if trimmed.ends_with("\\$recycle.bin") {
            return tag("Recycle Bin", TagKind::Warn);
        }
        if trimmed.ends_with("pagefile.sys") || trimmed.ends_with("swapfile.sys") || trimmed.ends_with("hiberfil.sys") {
            return tag("Windows virtual memory/hibernation file", TagKind::Warn);
        }
        if trimmed.ends_with("\\claude_pzs8sxrjxfjjc") || trimmed.contains("\\claudevm") {
            return tag("Claude app data — protected", TagKind::Protected);
        }
        if let Some(name) = inside {
            return Some(Tag { text: format!("Cleanable: {name}"), kind: TagKind::Cleanable });
        }
        if !profile.is_empty() {
            for d in ["documents", "downloads", "desktop", "pictures", "videos", "music"] {
                if trimmed == format!("{profile}\\{d}") {
                    return tag("Your files — review manually", TagKind::Review);
                }
            }
            if trimmed == format!("{profile}\\appdata") {
                return tag("App settings & caches", TagKind::Info);
            }
        }
        // odd top-level folders on a drive root are usually installers / leftovers / personal stuff
        let depth = trimmed.matches('\\').count();
        if depth == 1 && trimmed.len() > 3 {
            let std_dirs = [
                "\\users", "\\program files", "\\program files (x86)", "\\windows", "\\programdata", "\\recovery", "\\perflogs",
                "\\inetpub", "\\documents and settings", "\\$av_asw", "\\config.msi", "\\$recycle.bin", "\\system volume information",
            ];
            if !std_dirs.iter().any(|d| trimmed.ends_with(d)) {
                return tag("Your folder — review manually", TagKind::Review);
            }
        }
        contains.map(|name| Tag { text: format!("Contains cleanable: {name}"), kind: TagKind::Contains })
    }
}
