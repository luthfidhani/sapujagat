//! Safe filesystem primitives. Everything that deletes goes through here.
//!
//! Safety rules enforced in this module:
//!  * never follow symlinks / junctions (classic cleaner disaster)
//!  * every target is vetted by `guard()` before anything is touched
//!  * files that are locked / in use are skipped, never forced
//!  * the tool's own host (Claude, terminals) is never touched

use std::fs;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use glob::{MatchOptions, Pattern};
use walkdir::WalkDir;

const REPARSE: u32 = 0x400;

pub fn is_link(md: &fs::Metadata) -> bool {
    md.file_attributes() & REPARSE != 0
}

/// Path fragments that must never be inside a cleaning target.
const NEVER_FRAGMENTS: &[&str] = &[
    "\\.ssh", "\\.aws", "\\.azure", "\\.gnupg", "\\.claude", "\\.kube", "\\claude_pzs8sxrjxfjjc", "\\claudevm",
    "\\temp\\claude", "\\onedrive", "\\saved games", "\\login data", "\\cookies", "\\system volume information", "\\$recycle.bin",
    "\\windows\\system32\\config", "\\windows\\system32\\drivers", "\\windows\\winsxs", "\\windows\\servicing",
    "\\program files\\windowsapps", "\\windows\\fonts",
];

/// Lower-case form with one backslash between parts, so `C:\\a//b\` and `c:\a\b` compare equal.
pub fn canon_key(s: &str) -> String {
    let s = s.strip_prefix("\\\\?\\").unwrap_or(s).replace('/', "\\");
    let mut out = String::with_capacity(s.len());
    let mut prev_sep = false;
    for (i, c) in s.chars().enumerate() {
        if c == '\\' {
            // keep a leading double backslash (UNC); collapse every other run
            if prev_sep && i != 1 {
                continue;
            }
            prev_sep = true;
        } else {
            prev_sep = false;
        }
        out.push(c);
    }
    out.trim_end_matches('\\').to_lowercase()
}

fn norm(p: &Path) -> String {
    canon_key(&p.to_string_lossy())
}

static EXCLUDED: std::sync::RwLock<Vec<String>> = std::sync::RwLock::new(Vec::new());

/// Folders the user excluded in Settings. Nothing inside them is ever measured for deletion or deleted.
pub fn set_exclusions(list: &[String]) {
    *EXCLUDED.write().unwrap() = list.iter().map(|p| norm(Path::new(p))).filter(|s| !s.is_empty()).collect();
}

pub fn is_excluded(p: &Path) -> bool {
    let with_slash = format!("{}\\", norm(p));
    EXCLUDED.read().unwrap().iter().any(|e| with_slash.starts_with(&format!("{e}\\")))
}

/// Returns Err(reason) if `p` must not be used as a cleaning target (or inside one).
pub fn guard(p: &Path) -> Result<(), String> {
    guard_opts(p, false)
}

/// `allow_user_dirs` lets exact-name build artifacts (node_modules, target...) be removed from inside
/// Documents/Desktop/Downloads project folders. Credentials, Claude data, system dirs stay protected.
pub fn guard_opts(p: &Path, allow_user_dirs: bool) -> Result<(), String> {
    if is_excluded(p) {
        return Err("excluded by you in Settings".into());
    }
    let s = norm(p);
    let bytes = s.as_bytes();
    if bytes.len() < 4 || !(bytes[1] == b':' && bytes[2] == b'\\') {
        return Err("not an absolute drive path".into());
    }
    let profile = std::env::var("USERPROFILE").unwrap_or_default().to_lowercase();
    let mut exact: Vec<String> = vec![
        "c:\\windows".into(),
        "c:\\windows\\system32".into(),
        "c:\\program files".into(),
        "c:\\program files (x86)".into(),
        "c:\\programdata".into(),
        "c:\\users".into(),
    ];
    if !profile.is_empty() {
        for sub in ["", "\\appdata", "\\appdata\\local", "\\appdata\\roaming", "\\documents", "\\desktop", "\\downloads", "\\pictures", "\\music", "\\videos"] {
            exact.push(format!("{profile}{sub}"));
        }
    }
    if exact.iter().any(|e| *e == s) {
        return Err("protected system/user root".into());
    }
    let with_slash = format!("{s}\\");
    for f in NEVER_FRAGMENTS {
        if with_slash.contains(f) {
            return Err(format!("inside protected area ({})", f.trim_matches('\\')));
        }
    }
    // Your personal folders: anchored to the real profile path, so e.g. `.rustup\downloads` is not confused with Downloads.
    if !allow_user_dirs && !profile.is_empty() {
        for d in ["documents", "desktop", "downloads", "pictures", "videos", "music"] {
            if with_slash.starts_with(&format!("{profile}\\{d}\\")) {
                return Err(format!("inside your {d} folder"));
            }
        }
    }
    Ok(())
}

pub struct Filter {
    pats: Vec<Pattern>,
    min_age: Option<Duration>,
}

impl Filter {
    pub fn new(pattern: Option<&str>, age_days: u32) -> Self {
        let pats = pattern
            .map(|p| p.split('|').filter_map(|x| Pattern::new(x.trim()).ok()).collect())
            .unwrap_or_default();
        let min_age = (age_days > 0).then(|| Duration::from_secs(age_days as u64 * 86_400));
        Filter { pats, min_age }
    }
    fn name_ok(&self, name: &str) -> bool {
        if self.pats.is_empty() {
            return true;
        }
        let opt = MatchOptions { case_sensitive: false, require_literal_separator: false, require_literal_leading_dot: false };
        self.pats.iter().any(|p| p.matches_with(name, opt))
    }
    fn age_ok(&self, md: &fs::Metadata) -> bool {
        match self.min_age {
            None => true,
            Some(min) => md
                .modified()
                .ok()
                .and_then(|m| SystemTime::now().duration_since(m).ok())
                .map(|age| age >= min)
                .unwrap_or(false),
        }
    }
    fn pass(&self, name: &str, md: &fs::Metadata) -> bool {
        self.name_ok(name) && self.age_ok(md)
    }
    fn has_patterns(&self) -> bool {
        !self.pats.is_empty()
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Stat {
    pub bytes: u64,
    pub files: u64,
}

#[derive(Default, Clone, Copy, Debug)]
pub struct DelStat {
    pub bytes: u64,
    pub files: u64,
    pub locked: u64,
}

/// Expand env vars and globs in a target template into concrete existing paths.
pub fn resolve(tpl: &str) -> Vec<PathBuf> {
    let Some(expanded) = crate::winenv::expand(tpl) else { return vec![] };
    if !expanded.contains('*') && !expanded.contains('?') {
        let p = PathBuf::from(&expanded);
        return if p.exists() { vec![p] } else { vec![] };
    }
    glob::glob(&expanded).map(|it| it.flatten().collect()).unwrap_or_default()
}

pub fn measure(root: &Path, f: &Filter) -> Stat {
    let mut st = Stat::default();
    if guard(root).is_err() {
        return st;
    }
    let Ok(md) = fs::symlink_metadata(root) else { return st };
    if is_link(&md) {
        return st;
    }
    if md.is_file() {
        let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if f.pass(&name, &md) {
            st.bytes = md.len();
            st.files = 1;
        }
        return st;
    }
    for e in WalkDir::new(root).follow_links(false).into_iter().flatten() {
        if !e.file_type().is_file() {
            continue;
        }
        if let Ok(m) = e.metadata() {
            if is_link(&m) {
                continue;
            }
            if f.pass(&e.file_name().to_string_lossy(), &m) {
                st.bytes += m.len();
                st.files += 1;
            }
        }
    }
    st
}

fn try_remove_file(p: &Path) -> bool {
    if fs::remove_file(p).is_ok() {
        return true;
    }
    // read-only attribute is a common blocker; clear it and retry once
    if let Ok(md) = fs::metadata(p) {
        let mut perm = md.permissions();
        if perm.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            let _ = fs::set_permissions(p, perm);
            return fs::remove_file(p).is_ok();
        }
    }
    false
}

/// Delete the *contents* of `root` (or the single file if root is a file) that pass the filter.
pub fn purge(root: &Path, f: &Filter) -> DelStat {
    let mut d = DelStat::default();
    if guard(root).is_err() {
        return d;
    }
    let Ok(md) = fs::symlink_metadata(root) else { return d };
    if is_link(&md) {
        return d;
    }
    if md.is_file() {
        let name = root.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if f.pass(&name, &md) {
            if try_remove_file(root) {
                d.bytes = md.len();
                d.files = 1;
            } else {
                d.locked = 1;
            }
        }
        return d;
    }
    for e in WalkDir::new(root).follow_links(false).contents_first(true).min_depth(1).into_iter().flatten() {
        let path = e.path();
        let ft = e.file_type();
        if ft.is_file() {
            let Ok(m) = e.metadata() else { continue };
            if is_link(&m) || !f.pass(&e.file_name().to_string_lossy(), &m) {
                continue;
            }
            if guard(path).is_err() {
                continue;
            }
            if try_remove_file(path) {
                d.bytes += m.len();
                d.files += 1;
            } else {
                d.locked += 1;
            }
        } else if ft.is_dir() && !f.has_patterns() {
            // only removes directories that are already empty
            let _ = fs::remove_dir(path);
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// The exclusion list is process-global, so tests that set it must not run at the same time.
    static EXCL_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn sandbox(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("sapu-test-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    fn profile() -> String {
        std::env::var("USERPROFILE").unwrap()
    }

    #[test]
    fn guard_blocks_dangerous_roots() {
        let p = profile();
        for bad in [
            "C:\\Windows".to_string(),
            "C:\\Windows\\System32".to_string(),
            "C:\\Windows\\WinSxS\\x".to_string(),
            "C:\\Program Files".to_string(),
            "D:\\".to_string(),
            "relative\\path".to_string(),
            p.clone(),
            format!("{p}\\Documents"),
            format!("{p}\\Documents\\work\\file"),
            format!("{p}\\Downloads\\x"),
            format!("{p}\\.ssh\\keys"),
            format!("{p}\\.claude\\projects"),
            format!("{p}\\AppData\\Local\\Temp\\claude\\x"),
            format!("{p}\\AppData\\Local\\Packages\\Claude_pzs8sxrjxfjjc\\LocalCache"),
        ] {
            assert!(guard(Path::new(&bad)).is_err(), "should be blocked: {bad}");
        }
    }

    #[test]
    fn guard_allows_real_cache_targets() {
        let p = profile();
        for ok in [
            format!("{p}\\.rustup\\downloads"),
            format!("{p}\\AppData\\Local\\Temp"),
            format!("{p}\\AppData\\Local\\NVIDIA\\DXCache"),
            "C:\\Windows\\Temp".to_string(),
            "C:\\Windows\\SoftwareDistribution\\Download".to_string(),
        ] {
            assert!(guard(Path::new(&ok)).is_ok(), "should be allowed: {ok}");
        }
    }

    #[test]
    fn user_dirs_only_open_for_exact_build_artifacts() {
        let p = format!("{}\\Documents\\proj\\node_modules", profile());
        assert!(guard_opts(Path::new(&p), false).is_err());
        assert!(guard_opts(Path::new(&p), true).is_ok());
        // credentials stay protected even with the flag
        assert!(guard_opts(Path::new(&format!("{}\\.ssh", profile())), true).is_err());
    }

    #[test]
    fn purge_honours_pattern() {
        let d = sandbox("pattern");
        fs::write(d.join("a.tmp"), b"1234").unwrap();
        fs::write(d.join("b.log"), b"1234").unwrap();
        let st = purge(&d, &Filter::new(Some("*.tmp"), 0));
        assert_eq!(st.files, 1);
        assert!(!d.join("a.tmp").exists());
        assert!(d.join("b.log").exists());
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn purge_honours_min_age() {
        let d = sandbox("age");
        fs::write(d.join("fresh.bin"), b"1234").unwrap();
        let st = purge(&d, &Filter::new(None, 2));
        assert_eq!(st.files, 0, "a file created just now must survive a 2-day age filter");
        assert!(d.join("fresh.bin").exists());
        let _ = fs::remove_dir_all(&d);
    }

    fn make_junction(link: &Path, target: &Path) {
        let out = Command::new("cmd").args(["/C", "mklink", "/J", &link.to_string_lossy(), &target.to_string_lossy()]).output().unwrap();
        assert!(out.status.success(), "mklink failed: {}", String::from_utf8_lossy(&out.stdout));
    }

    #[test]
    fn purge_never_follows_junctions() {
        let precious = sandbox("precious");
        fs::write(precious.join("keep.txt"), b"do not delete me").unwrap();
        let root = sandbox("root-purge");
        fs::write(root.join("junk.tmp"), b"x").unwrap();
        make_junction(&root.join("link"), &precious);
        let _ = purge(&root, &Filter::new(None, 0));
        assert!(precious.join("keep.txt").exists(), "purge followed a junction and deleted outside its target!");
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&precious);
    }

    #[test]
    fn remove_tree_never_follows_junctions() {
        let precious = sandbox("precious2");
        fs::write(precious.join("keep.txt"), b"do not delete me").unwrap();
        let root = sandbox("root-tree");
        fs::write(root.join("junk.bin"), b"x").unwrap();
        make_junction(&root.join("link"), &precious);
        let _ = remove_tree(&root, true);
        assert!(precious.join("keep.txt").exists(), "remove_tree followed a junction and deleted outside its target!");
        assert!(!root.join("junk.bin").exists());
        let _ = fs::remove_dir_all(&root);
        let _ = fs::remove_dir_all(&precious);
    }

    #[test]
    fn measure_and_purge_agree() {
        let d = sandbox("agree");
        fs::create_dir_all(d.join("sub")).unwrap();
        fs::write(d.join("a"), vec![0u8; 1000]).unwrap();
        fs::write(d.join("sub").join("b"), vec![0u8; 2500]).unwrap();
        let f = Filter::new(None, 0);
        let m = measure(&d, &f);
        assert_eq!((m.bytes, m.files), (3500, 2));
        let p = purge(&d, &f);
        assert_eq!((p.bytes, p.files), (3500, 2));
        assert!(!d.join("sub").exists(), "empty subdirectories should be removed");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn canon_key_ignores_separator_noise_and_case() {
        assert_eq!(canon_key("C:\\\\Users\\\\Me\\"), "c:\\users\\me");
        assert_eq!(canon_key("c:/users//me/"), "c:\\users\\me");
        assert_eq!(canon_key("\\\\?\\C:\\Users\\Me"), "c:\\users\\me");
        assert_eq!(canon_key("\\\\server\\share\\x"), "\\\\server\\share\\x");
    }

    #[test]
    fn exclusion_with_doubled_backslashes_still_protects() {
        let _g = EXCL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = sandbox("excl-dbl");
        let keep = root.join("keepme");
        fs::create_dir_all(&keep).unwrap();
        fs::write(keep.join("important.dat"), b"precious").unwrap();
        let weird = keep.display().to_string().replace('\\', "\\\\");
        set_exclusions(&[weird]);
        let _ = purge(&root, &Filter::new(None, 0));
        set_exclusions(&[]);
        assert!(keep.join("important.dat").exists(), "exclusion written with doubled backslashes did not protect");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn exclusions_protect_files_even_inside_a_cleaning_target() {
        let _g = EXCL_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let root = sandbox("excl-root");
        let keep = root.join("keepme");
        fs::create_dir_all(&keep).unwrap();
        fs::write(keep.join("important.dat"), b"precious").unwrap();
        fs::write(root.join("junk.tmp"), b"x").unwrap();
        set_exclusions(&[keep.display().to_string()]);
        let st = purge(&root, &Filter::new(None, 0));
        set_exclusions(&[]);
        assert!(keep.join("important.dat").exists(), "an excluded folder was cleaned!");
        assert!(!root.join("junk.tmp").exists());
        assert_eq!(st.files, 1);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn expand_requires_defined_vars() {
        std::env::set_var("SAPU_TEST_VAR", "abc");
        assert_eq!(crate::winenv::expand("x\\%SAPU_TEST_VAR%\\y").as_deref(), Some("x\\abc\\y"));
        assert!(crate::winenv::expand("%SAPU_SURELY_UNDEFINED%\\y").is_none());
    }
}

/// Size of a whole tree without the cleaning guard (read-only; used by the analyzer and dev-junk finder).
pub fn measure_tree(root: &Path) -> Stat {
    let mut st = Stat::default();
    for e in WalkDir::new(root).follow_links(false).into_iter().flatten() {
        if e.file_type().is_file() {
            if let Ok(m) = e.metadata() {
                st.bytes += m.len();
                st.files += 1;
            }
        }
    }
    st
}

/// Remove an entire directory tree (used for build artifacts like node_modules / target). Never follows links.
pub fn remove_tree(root: &Path, allow_user_dirs: bool) -> DelStat {
    let mut d = DelStat::default();
    if guard_opts(root, allow_user_dirs).is_err() {
        return d;
    }
    let Ok(md) = fs::symlink_metadata(root) else { return d };
    if is_link(&md) || !md.is_dir() {
        return d;
    }
    for e in WalkDir::new(root).follow_links(false).contents_first(true).into_iter().flatten() {
        let path = e.path();
        let ft = e.file_type();
        if is_excluded(path) {
            continue;
        }
        if ft.is_file() {
            let len = e.metadata().map(|m| m.len()).unwrap_or(0);
            if try_remove_file(path) {
                d.bytes += len;
                d.files += 1;
            } else {
                d.locked += 1;
            }
        } else if ft.is_symlink() {
            // remove the link itself, never its target
            if fs::remove_file(path).is_err() {
                let _ = fs::remove_dir(path);
            }
        } else if ft.is_dir() {
            let _ = fs::remove_dir(path);
        }
    }
    d
}
