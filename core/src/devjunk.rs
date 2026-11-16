//! Finds regenerable build artifacts of old projects: node_modules, Rust target, Python venv/__pycache__...
//!
//! Detection is marker-based: a folder is only reported if it has the exact name AND a sibling/child
//! file proving it belongs to a real project (package.json, Cargo.toml, pyvenv.cfg, *.csproj ...).

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rayon::prelude::*;
use serde::Serialize;
use walkdir::WalkDir;

use crate::fsx;

#[derive(Clone, Debug, Serialize)]
pub struct Junk {
    pub path: PathBuf,
    pub kind: &'static str,
    pub bytes: u64,
    pub age_days: u64,
    pub project: PathBuf,
    /// Re-creating this costs real time (reinstall/rebuild) — shown with a hint.
    pub rebuild_hint: &'static str,
}

const PRUNE: &[&str] = &[
    "appdata", "windows", "program files", "program files (x86)", "programdata", "$recycle.bin", "system volume information",
    ".git", ".cargo", ".rustup", ".docker", "recovery", "$av_asw", "steamapps", "winsxs",
];

fn has_file_with_ext(dir: &Path, exts: &[&str]) -> bool {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten().any(|e| {
                e.path().extension().map(|x| exts.iter().any(|w| x.to_string_lossy().eq_ignore_ascii_case(w))).unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn classify(dir: &Path, name: &str) -> Option<(&'static str, &'static str)> {
    let parent = dir.parent()?;
    match name {
        "node_modules" if parent.join("package.json").exists() => Some(("node_modules", "npm install")),
        "target" if parent.join("Cargo.toml").exists() => Some(("Rust target", "cargo build")),
        ".venv" | "venv" if dir.join("pyvenv.cfg").exists() => Some(("Python venv", "recreate + pip install")),
        "__pycache__" => Some(("Python bytecode", "regenerated automatically")),
        "obj" | "bin" if has_file_with_ext(parent, &["csproj", "fsproj", "vbproj"]) => Some((".NET build output", "dotnet build")),
        ".next" | ".nuxt" if parent.join("package.json").exists() => Some(("Web build cache", "next/nuxt build")),
        ".gradle" | "build" if parent.join("build.gradle").exists() || parent.join("build.gradle.kts").exists() => Some(("Gradle build", "gradle build")),
        _ => None,
    }
}

fn age_days(p: &Path) -> u64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|m| SystemTime::now().duration_since(m).ok())
        .map(|d| d.as_secs() / 86_400)
        .unwrap_or(0)
}

fn walk_root(root: &Path) -> Vec<Junk> {
    let mut found = Vec::new();
    let mut it = WalkDir::new(root).follow_links(false).max_depth(9).into_iter();
    while let Some(entry) = it.next() {
        let Ok(e) = entry else { continue };
        if !e.file_type().is_dir() || e.depth() == 0 {
            continue;
        }
        let name = e.file_name().to_string_lossy().to_string();
        let lc = name.to_lowercase();
        if PRUNE.contains(&lc.as_str()) {
            it.skip_current_dir();
            continue;
        }
        if let Some((kind, hint)) = classify(e.path(), &name) {
            let st = fsx::measure_tree(e.path());
            if st.bytes >= 1_000_000 {
                found.push(Junk {
                    path: e.path().to_path_buf(),
                    kind,
                    bytes: st.bytes,
                    age_days: age_days(e.path()),
                    project: e.path().parent().map(Path::to_path_buf).unwrap_or_default(),
                    rebuild_hint: hint,
                });
            }
            it.skip_current_dir();
        }
    }
    found
}

pub fn find(roots: &[PathBuf]) -> Vec<Junk> {
    // parallelise across the first level of every root
    let mut tops: Vec<PathBuf> = Vec::new();
    for r in roots {
        match std::fs::read_dir(r) {
            Ok(rd) => {
                for e in rd.flatten() {
                    if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        tops.push(e.path());
                    }
                }
            }
            Err(_) => continue,
        }
    }
    let mut all: Vec<Junk> = tops
        .par_iter()
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
            !PRUNE.contains(&n.as_str())
        })
        .flat_map(|p| {
            let mut v = walk_root(p);
            // the top-level folder itself might be the artifact (e.g. D:\node_modules)
            if let Some(name) = p.file_name().map(|n| n.to_string_lossy().to_string()) {
                if let Some((kind, hint)) = classify(p, &name) {
                    let st = fsx::measure_tree(p);
                    if st.bytes >= 1_000_000 {
                        v.push(Junk { path: p.clone(), kind, bytes: st.bytes, age_days: age_days(p), project: p.parent().map(Path::to_path_buf).unwrap_or_default(), rebuild_hint: hint });
                    }
                }
            }
            v
        })
        .collect();
    all.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    all
}

pub fn delete(j: &Junk) -> fsx::DelStat {
    fsx::remove_tree(&j.path, true)
}
