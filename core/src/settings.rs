//! User settings, stored in %LOCALAPPDATA%\sapujagat\settings.json and shared by the CLI and the GUI.
//!
//!  * `exclusions`     folders the tool must never touch (honoured by the safety guard itself)
//!  * `disabled_rules` rules that are removed from scanning and cleaning entirely
//!  * `recommended`    per-rule override of "selected by default"
//!  * `restore_point`  create a System Restore point before system-level changes

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

fn yes() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Settings {
    #[serde(default)]
    pub exclusions: Vec<String>,
    #[serde(default)]
    pub disabled_rules: Vec<String>,
    #[serde(default)]
    pub recommended: HashMap<String, bool>,
    #[serde(default = "yes")]
    pub restore_point: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { exclusions: vec![], disabled_rules: vec![], recommended: HashMap::new(), restore_point: true }
    }
}

fn cell() -> &'static RwLock<Settings> {
    static C: OnceLock<RwLock<Settings>> = OnceLock::new();
    C.get_or_init(|| RwLock::new(Settings::default()))
}

pub fn file() -> PathBuf {
    PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into())).join("sapujagat").join("settings.json")
}

pub fn current() -> Settings {
    cell().read().unwrap().clone()
}

/// Make `s` the active configuration (does not write it to disk).
pub fn apply(s: Settings) {
    crate::fsx::set_exclusions(&s.exclusions);
    *cell().write().unwrap() = s;
}

pub fn load() -> Settings {
    std::fs::read_to_string(file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Load the saved settings and activate them. Call once at start-up.
pub fn init() {
    apply(load());
}

pub fn save(s: &Settings) -> std::io::Result<()> {
    let f = file();
    if let Some(dir) = f.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(f, serde_json::to_string_pretty(s).unwrap_or_default())
}

/// Validate user input and return the cleaned-up settings, or a message the UI can show.
pub fn validate(mut s: Settings, known_rules: &[&str]) -> Result<Settings, String> {
    let mut ex: Vec<String> = Vec::new();
    for raw in &s.exclusions {
        let t = raw.trim().trim_matches('"').trim_end_matches('\\').to_string();
        if t.is_empty() {
            continue;
        }
        let b = t.as_bytes();
        if b.len() < 3 || b[1] != b':' || b[2] != b'\\' {
            return Err(format!("\"{t}\" is not a full folder path such as D:\\Projects"));
        }
        if !Path::new(&t).is_dir() {
            return Err(format!("\"{t}\" is not an existing folder"));
        }
        if !ex.iter().any(|e| e.eq_ignore_ascii_case(&t)) {
            ex.push(t);
        }
    }
    s.exclusions = ex;
    s.disabled_rules.retain(|id| known_rules.contains(&id.as_str()));
    s.disabled_rules.sort();
    s.disabled_rules.dedup();
    s.recommended.retain(|id, _| known_rules.contains(&id.as_str()));
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_bad_paths_and_unknown_rules() {
        let bad = Settings { exclusions: vec!["relative\\path".into()], ..Settings::default() };
        assert!(validate(bad, &["a"]).is_err());
        let missing = Settings { exclusions: vec!["C:\\definitely\\not\\here\\xyz".into()], ..Settings::default() };
        assert!(validate(missing, &["a"]).is_err());
        let s = Settings { exclusions: vec![std::env::temp_dir().display().to_string() + "\\"], disabled_rules: vec!["a".into(), "ghost".into()], recommended: HashMap::from([("ghost".into(), true), ("a".into(), false)]), restore_point: true };
        let ok = validate(s, &["a"]).unwrap();
        assert_eq!(ok.disabled_rules, vec!["a"]);
        assert_eq!(ok.recommended.len(), 1);
        assert!(!ok.exclusions[0].ends_with('\\'));
    }
}
