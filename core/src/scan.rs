//! Read-only measurement of every rule. Nothing is modified here.

use std::collections::{HashMap, HashSet};

use rayon::prelude::*;
use serde::Serialize;
use sysinfo::System;

use crate::fsx::{self, Filter};
use crate::rules::{Kind, Rule, Special};
use crate::winenv;

#[derive(Clone, Debug, Serialize)]
pub struct RuleScan {
    pub id: &'static str,
    /// None = size can only be known after running (e.g. DISM component cleanup).
    pub bytes: Option<u64>,
    pub files: u64,
    pub available: bool,
    pub note: Option<String>,
    /// Apps from `rule.close` that are currently running (they lock files).
    pub running: Vec<String>,
}

pub struct DockerState {
    pub cli: bool,
    pub up: bool,
    pub reclaim: HashMap<String, u64>,
}

pub fn running_names() -> HashSet<String> {
    let mut sys = System::new();
    sys.refresh_processes();
    sys.processes().values().map(|p| p.name().to_lowercase()).collect()
}

/// Parse sizes printed by docker, e.g. "1.234GB (45%)", "512MB", "12.3kB", "0B".
pub fn parse_docker_size(s: &str) -> u64 {
    let tok = s.split_whitespace().next().unwrap_or("");
    let split = tok.find(|c: char| c.is_alphabetic()).unwrap_or(tok.len());
    let (num, unit) = tok.split_at(split);
    let n: f64 = num.parse().unwrap_or(0.0);
    let mult = match unit.to_lowercase().as_str() {
        "kb" => 1e3,
        "mb" => 1e6,
        "gb" => 1e9,
        "tb" => 1e12,
        _ => 1.0,
    };
    (n * mult) as u64
}

#[cfg(test)]
mod tests {
    use super::parse_docker_size;

    #[test]
    fn docker_sizes() {
        assert_eq!(parse_docker_size("0B"), 0);
        assert_eq!(parse_docker_size("512MB"), 512_000_000);
        assert_eq!(parse_docker_size("1.5GB (45%)"), 1_500_000_000);
        assert_eq!(parse_docker_size("12.3kB"), 12_300);
        assert_eq!(parse_docker_size(""), 0);
    }
}

pub fn docker_state() -> DockerState {
    let mut st = DockerState { cli: winenv::command_exists("docker"), up: false, reclaim: HashMap::new() };
    if !st.cli {
        return st;
    }
    st.up = winenv::run_ok("docker", &["info", "--format", "{{.ServerVersion}}"]);
    if st.up {
        if let Some(text) = winenv::run_text("docker", &["system", "df", "--format", "{{json .}}"]) {
            for line in text.lines() {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
                    let ty = v["Type"].as_str().unwrap_or("").to_string();
                    let rec = parse_docker_size(v["Reclaimable"].as_str().unwrap_or("0B"));
                    st.reclaim.insert(ty, rec);
                }
            }
        }
    }
    st
}

pub fn scan_all(rules: &[Rule]) -> Vec<RuleScan> {
    let running = running_names();
    let docker = docker_state();
    let admin = winenv::is_admin();
    rules.par_iter().map(|r| scan_one(r, &running, &docker, admin)).collect()
}

fn scan_one(r: &Rule, running: &HashSet<String>, docker: &DockerState, admin: bool) -> RuleScan {
    let running_apps: Vec<String> = r.close.iter().filter(|c| running.contains(&c.to_lowercase())).map(|s| s.to_string()).collect();
    let mut s = RuleScan { id: r.id, bytes: Some(0), files: 0, available: true, note: None, running: running_apps };
    match &r.kind {
        Kind::Paths(targets) => {
            let mut any = false;
            for t in targets {
                let f = Filter::new(t.pattern.as_deref(), t.age_days);
                for p in fsx::resolve(&t.path) {
                    any = true;
                    let st = fsx::measure(&p, &f);
                    s.bytes = Some(s.bytes.unwrap_or(0) + st.bytes);
                    s.files += st.files;
                }
            }
            if !any {
                s.available = false;
                s.note = Some("not present on this PC".into());
            } else if r.admin && !admin {
                s.note = Some("estimate — needs Administrator".into());
            }
        }
        Kind::Special(sp) => match sp {
            Special::RecycleBin => {
                let (b, n) = winenv::recycle_bin_info().unwrap_or((0, 0));
                s.bytes = Some(b);
                s.files = n;
            }
            Special::DnsCache => {}
            Special::DockerBuildCache | Special::DockerStopped | Special::DockerUnusedImages | Special::DockerVolumes => {
                if !docker.cli {
                    s.available = false;
                    s.note = Some("Docker not installed".into());
                } else if !docker.up {
                    s.available = false;
                    s.note = Some("Docker is not running — start Docker Desktop to clean this".into());
                } else {
                    let key = match sp {
                        Special::DockerBuildCache => "Build Cache",
                        Special::DockerStopped => "Containers",
                        Special::DockerUnusedImages => "Images",
                        _ => "Local Volumes",
                    };
                    s.bytes = Some(*docker.reclaim.get(key).unwrap_or(&0));
                }
            }
            Special::ComponentStore => {
                s.bytes = None;
                s.note = Some("size known after run".into());
            }
            Special::EventLogs => {
                let p = std::path::Path::new("C:\\Windows\\System32\\winevt\\Logs");
                s.bytes = Some(fsx::measure(p, &Filter::new(None, 0)).bytes);
            }
            Special::Hibernation => match std::fs::metadata("C:\\hiberfil.sys") {
                Ok(m) => s.bytes = Some(m.len()),
                Err(_) => {
                    if admin {
                        s.available = false;
                        s.note = Some("hibernation is already off".into());
                    } else {
                        s.bytes = None;
                        s.note = Some("needs Administrator to inspect".into());
                    }
                }
            },
        },
    }
    s
}
