//! Executes cleaning rules. Reports progress through events; never prints.

use std::path::PathBuf;

use serde::Serialize;

use crate::fsx::{self, Filter};
use crate::rules::{Kind, Risk, Rule, Special};
use crate::scan::parse_docker_size;
use crate::winenv::{self, DriveInfo};

#[derive(Clone, Serialize)]
pub struct RuleResult {
    pub id: String,
    pub name: String,
    pub freed: u64,
    pub files: u64,
    pub locked: u64,
    pub ok: bool,
    pub note: String,
}

#[derive(Serialize)]
pub struct Report {
    pub before: Vec<DriveInfo>,
    pub after: Vec<DriveInfo>,
    pub results: Vec<RuleResult>,
    pub restore_point: Option<bool>,
    pub killed_apps: usize,
    pub total_freed: u64,
}

pub struct Options {
    pub close_apps: bool,
    pub restore_point: bool,
}

/// Progress events for front-ends.
#[derive(Clone, Serialize)]
#[serde(tag = "kind")]
pub enum Event {
    Stage { message: String },
    RestorePoint { ok: bool },
    RuleStart { index: usize, total: usize, id: String, name: String },
    RuleDone { index: usize, total: usize, result: RuleResult },
}

pub fn create_restore_point() -> bool {
    let script = "$ErrorActionPreference='Stop'; Enable-ComputerRestore -Drive 'C:\\'; \
        New-ItemProperty 'HKLM:\\SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\SystemRestore' -Name SystemRestorePointCreationFrequency -Value 0 -PropertyType DWord -Force | Out-Null; \
        Checkpoint-Computer -Description 'Sapu Jagat' -RestorePointType MODIFY_SETTINGS";
    winenv::run_ok("powershell", &["-NoProfile", "-NonInteractive", "-Command", script])
}

fn parse_reclaimed(text: &str) -> u64 {
    let mut best = 0;
    for line in text.lines() {
        if line.to_lowercase().contains("total") {
            if let Some(tok) = line.split_whitespace().last() {
                best = best.max(parse_docker_size(tok));
            }
        }
    }
    best
}

fn docker(args: &[&str]) -> (bool, u64) {
    match winenv::run("docker", args) {
        Ok(o) => (o.status.success(), parse_reclaimed(&String::from_utf8_lossy(&o.stdout))),
        Err(_) => (false, 0),
    }
}

fn gb(b: u64) -> String {
    format!("{:.2} GB", b as f64 / 1_073_741_824.0)
}

fn run_special(s: Special, stage: &mut dyn FnMut(String)) -> RuleResult {
    let mut r = RuleResult { id: String::new(), name: String::new(), freed: 0, files: 0, locked: 0, ok: true, note: String::new() };
    match s {
        Special::RecycleBin => {
            let (b, n) = winenv::recycle_bin_info().unwrap_or((0, 0));
            // Windows reports an error when the bin is already empty, so only call it when there is something to remove
            r.ok = n == 0 || winenv::empty_recycle_bin();
            r.freed = b;
            r.files = n;
        }
        Special::DnsCache => r.ok = winenv::run_ok("ipconfig", &["/flushdns"]),
        Special::DockerBuildCache => {
            let (ok, b) = docker(&["builder", "prune", "-af"]);
            r.ok = ok;
            r.freed = b;
        }
        Special::DockerStopped => {
            let (ok1, b1) = docker(&["container", "prune", "-f"]);
            let (ok2, b2) = docker(&["image", "prune", "-f"]);
            r.ok = ok1 && ok2;
            r.freed = b1 + b2;
        }
        Special::DockerUnusedImages => {
            let (ok, b) = docker(&["image", "prune", "-af"]);
            r.ok = ok;
            r.freed = b;
        }
        Special::DockerVolumes => {
            let (ok, b) = docker(&["volume", "prune", "-af"]);
            r.ok = ok;
            r.freed = b;
        }
        Special::ComponentStore => {
            r.ok = winenv::run_ok("dism.exe", &["/Online", "/Cleanup-Image", "/StartComponentCleanup"]);
            r.note = "result visible in the free-space change".into();
        }
        Special::EventLogs => {
            let list = winenv::run_text("wevtutil", &["el"]).unwrap_or_default();
            let mut cleared = 0;
            for name in list.lines().map(str::trim).filter(|l| !l.is_empty()) {
                if winenv::run_ok("wevtutil", &["cl", name]) {
                    cleared += 1;
                }
            }
            r.files = cleared;
            r.note = format!("{cleared} logs cleared");
            r.ok = cleared > 0;
        }
        Special::Hibernation => {
            let before = std::fs::metadata("C:\\hiberfil.sys").map(|m| m.len()).unwrap_or(0);
            r.ok = winenv::run_ok("powercfg", &["/h", "off"]);
            r.freed = if r.ok { before } else { 0 };
            r.note = "re-enable with: powercfg /h on".into();
        }
        Special::DockerCompact => match crate::dockerdisk::compact(stage) {
            Ok(done) => {
                r.freed = done.iter().map(|(_, b, a)| b.saturating_sub(*a)).sum();
                r.files = done.len() as u64;
                r.note = done.iter().map(|(_, b, a)| format!("{} to {}", gb(*b), gb(*a))).collect::<Vec<_>>().join(", ");
            }
            Err(e) => {
                r.ok = false;
                r.note = e;
            }
        },
        Special::OldDrivers => match crate::drivers::find() {
            Ok(list) => {
                let (mut removed, mut kept_in_use) = (0u64, 0u64);
                for d in &list {
                    stage(format!("Removing {} {} {}", d.provider, d.class, d.version));
                    crate::drivers::remove(d);
                    if std::path::Path::new(&d.folder).exists() {
                        kept_in_use += 1;
                    } else {
                        removed += 1;
                        r.freed += d.bytes;
                    }
                }
                r.files = removed;
                r.ok = removed > 0 || list.is_empty();
                r.note = if kept_in_use > 0 { format!("{removed} removed, {kept_in_use} kept because Windows still uses them") } else { format!("{removed} old driver packages removed") };
            }
            Err(e) => {
                r.ok = false;
                r.note = e;
            }
        },
    }
    r
}

fn run_paths(rule: &Rule) -> RuleResult {
    let mut r = RuleResult { id: rule.id.into(), name: rule.name.into(), freed: 0, files: 0, locked: 0, ok: true, note: String::new() };
    let Kind::Paths(targets) = &rule.kind else { return r };
    let mut seen: Vec<PathBuf> = Vec::new();
    for t in targets {
        let f = Filter::new(t.pattern.as_deref(), t.age_days);
        for p in fsx::resolve(&t.path) {
            if seen.contains(&p) {
                continue;
            }
            seen.push(p.clone());
            if let Err(why) = fsx::guard(&p) {
                r.note = format!("skipped {}: {}", p.display(), why);
                continue;
            }
            let d = fsx::purge(&p, &f);
            r.freed += d.bytes;
            r.files += d.files;
            r.locked += d.locked;
        }
    }
    if r.locked > 0 {
        r.note = format!("{} file(s) in use were skipped", r.locked);
    }
    r
}

fn service(action: &str, name: &str) {
    let _ = winenv::run("net", &[action, name]);
}

pub fn execute(rules: &[&Rule], opts: &Options, is_admin: bool, on_event: &mut dyn FnMut(Event)) -> Report {
    let before = winenv::fixed_drives();
    let mut report = Report { before, after: vec![], results: vec![], restore_point: None, killed_apps: 0, total_freed: 0 };

    let needs_rp = is_admin && opts.restore_point && rules.iter().any(|r| r.admin || r.risk != Risk::Safe);
    if needs_rp {
        on_event(Event::Stage { message: "Creating a System Restore point first (safety net)…".into() });
        let ok = create_restore_point();
        on_event(Event::RestorePoint { ok });
        report.restore_point = Some(ok);
    }

    if opts.close_apps {
        let mut names: Vec<&str> = rules.iter().flat_map(|r| r.close.iter().copied()).collect();
        names.sort();
        names.dedup();
        report.killed_apps = crate::procs::kill_by_names(&names);
        if report.killed_apps > 0 {
            on_event(Event::Stage { message: format!("Closed {} app process(es) that were locking caches", report.killed_apps) });
            std::thread::sleep(std::time::Duration::from_millis(1500));
        }
    }

    let total = rules.len();
    for (index, rule) in rules.iter().enumerate() {
        on_event(Event::RuleStart { index, total, id: rule.id.into(), name: rule.name.into() });
        let mut res = if rule.admin && !is_admin {
            RuleResult { id: rule.id.into(), name: rule.name.into(), freed: 0, files: 0, locked: 0, ok: false, note: "needs Administrator".into() }
        } else {
            for s in &rule.services {
                service("stop", s);
            }
            let mut res = match &rule.kind {
                Kind::Paths(_) => run_paths(rule),
                Kind::Special(sp) => run_special(*sp, &mut |m| on_event(Event::Stage { message: m })),
            };
            for s in &rule.services {
                service("start", s);
            }
            res.id = rule.id.into();
            res.name = rule.name.into();
            res
        };
        if res.note.is_empty() && !res.ok {
            res.note = "command failed".into();
        }
        on_event(Event::RuleDone { index, total, result: res.clone() });
        report.results.push(res);
    }

    report.after = winenv::fixed_drives();
    report.total_freed = report.results.iter().map(|r| r.freed).sum();
    write_log(&report);
    report
}

#[derive(Serialize)]
pub struct HistoryEntry {
    pub when: String,
    pub total: u64,
    pub items: usize,
    pub top: Vec<(String, u64)>,
}

/// Past cleaning runs, newest first, read back from the JSON logs this engine writes.
pub fn history() -> Vec<HistoryEntry> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(log_dir()) else { return out };
    for e in rd.flatten() {
        if e.path().extension().map(|x| x != "json").unwrap_or(true) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        let ts = v["time"].as_str().unwrap_or("");
        let when = if ts.len() == 15 {
            format!("{}-{}-{} {}:{}:{}", &ts[0..4], &ts[4..6], &ts[6..8], &ts[9..11], &ts[11..13], &ts[13..15])
        } else {
            ts.to_string()
        };
        let mut rows: Vec<(String, u64)> = v["results"]
            .as_array()
            .map(|a| a.iter().filter_map(|r| Some((r["name"].as_str()?.to_string(), r["freed"].as_u64()?))).filter(|(_, f)| *f > 0).collect())
            .unwrap_or_default();
        rows.sort_by(|a, b| b.1.cmp(&a.1));
        out.push(HistoryEntry { when, total: v["total_freed_by_rules"].as_u64().unwrap_or(0), items: rows.len(), top: rows.into_iter().take(6).collect() });
    }
    out.sort_by(|a, b| b.when.cmp(&a.when));
    out.truncate(100);
    out
}

pub fn log_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into());
    PathBuf::from(base).join("sapujagat").join("logs")
}

fn write_log(rep: &Report) {
    #[derive(Serialize)]
    struct Log<'a> {
        time: String,
        results: &'a [RuleResult],
        total_freed_by_rules: u64,
    }
    let dir = log_dir();
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
    let log = Log { time: ts.clone(), results: &rep.results, total_freed_by_rules: rep.total_freed };
    if let Ok(json) = serde_json::to_string_pretty(&log) {
        let _ = std::fs::write(dir.join(format!("{ts}.json")), json);
    }
}
