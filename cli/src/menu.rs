//! Interactive flows (the "pretty CLI"). Every flow also exists as a non-interactive subcommand in main.rs.

use std::collections::HashMap;
use std::path::PathBuf;

use inquire::error::InquireError;
use inquire::{Confirm, MultiSelect, Select, Text};

use sapu_core::rules::{self, Cat, Kind, Risk, Rule, Special};
use sapu_core::scan::{self, RuleScan};
use sapu_core::{analyze, clean, devjunk, procs, startup, winenv};

use crate::{ui, views};

const CATS: [Cat; 7] = [Cat::System, Cat::Gpu, Cat::Browser, Cat::Dev, Cat::Docker, Cat::Apps, Cat::Privacy];

fn ask<T>(r: Result<T, InquireError>) -> Option<T> {
    match r {
        Ok(v) => Some(v),
        Err(InquireError::OperationCanceled) | Err(InquireError::OperationInterrupted) => None,
        Err(e) => {
            ui::fail(&e.to_string());
            None
        }
    }
}

fn confirm(msg: &str, default: bool) -> bool {
    ask(Confirm::new(msg).with_default(default).prompt()).unwrap_or(false)
}

pub fn risk_badge(r: Risk) -> String {
    match r {
        Risk::Safe => ui::badge("SAFE", ui::GREEN),
        Risk::Careful => ui::badge("CAREFUL", ui::YELLOW),
        Risk::Advanced => ui::badge("ADVANCED", ui::RED),
    }
}

fn risk_txt(r: Risk) -> &'static str {
    match r {
        Risk::Safe => "SAFE    ",
        Risk::Careful => "CAREFUL ",
        Risk::Advanced => "ADVANCED",
    }
}

pub fn system_header() {
    let si = procs::sys_info();
    let cpu = si.cpu;
    let (used, total) = (si.ram_used, si.ram_total);
    let f = used as f64 / total as f64;
    println!("   {}  {}", ui::dim("CPU "), cpu);
    println!("   {}  {} {} / {}", ui::dim("RAM "), ui::bar(f, 22, ui::heat(f)), ui::bold(&ui::human(used)), ui::human(total));
    for d in winenv::fixed_drives() {
        let uf = 1.0 - d.free as f64 / d.total as f64;
        println!("   {}  {} {} free of {}", ui::dim(&format!("{:<4}", d.root.trim_end_matches('\\'))), ui::bar(uf, 22, ui::heat(uf)), ui::bold(&ui::human(d.free)), ui::human(d.total));
    }
}

// ───────────────────────────── main menu ─────────────────────────────

pub fn main_menu(admin: bool) {
    loop {
        let items = vec![
            "🧹  Clean      — deep scan & remove junk (recommended)".to_string(),
            "🧠  Boost      — close background apps, free RAM & CPU".to_string(),
            "🚀  Startup    — control what launches with Windows".to_string(),
            "💽  Analyzer   — find what is eating your disk".to_string(),
            "🛠   Dev junk   — node_modules / target / venv of old projects".to_string(),
            "📖  Catalog    — everything this tool can clean, and why".to_string(),
            if admin { "🛡   Administrator — active ✔".to_string() } else { "🛡   Administrator — relaunch with full power".to_string() },
            "👋  Exit".to_string(),
        ];
        let Some(choice) = ask(Select::new("What do you want to do?", items).with_page_size(10).with_help_message("↑↓ move · enter select · esc exit").raw_prompt()) else { return };
        match choice.index {
            0 => {
                if clean_interactive(admin) {
                    return;
                }
            }
            1 => boost_interactive(admin),
            2 => startup_interactive(admin),
            3 => analyze_interactive(),
            4 => dev_interactive(),
            5 => catalog_view(),
            6 => {
                if admin {
                    ui::ok("Already running as Administrator.");
                } else if confirm("Windows will show a UAC prompt. Relaunch as Administrator now?", true) {
                    if winenv::relaunch_as_admin(&[]) {
                        return;
                    }
                    ui::warn("Elevation was cancelled.");
                }
            }
            _ => return,
        }
        println!();
    }
}

// ───────────────────────────── clean ─────────────────────────────

fn bytes_of(s: &RuleScan) -> u64 {
    s.bytes.unwrap_or(0)
}

/// Prints the categorised scan table. Returns (user-level bytes, admin-only bytes).
pub fn print_scan(rules: &[Rule], scans: &[RuleScan], admin: bool) -> (u64, u64) {
    let by: HashMap<&str, &RuleScan> = scans.iter().map(|s| (s.id, s)).collect();
    let maxb = scans.iter().filter_map(|s| s.bytes).max().unwrap_or(1).max(1);
    let (mut user_total, mut admin_total, mut empty) = (0u64, 0u64, 0usize);
    for cat in CATS {
        let mut t = ui::Table::new(&[("Item", false), ("Risk", false), ("Size", true), ("", false), ("Note", false)]);
        let mut any = false;
        for r in rules.iter().filter(|r| r.cat == cat) {
            let s = by[r.id];
            let is_paths = matches!(r.kind, Kind::Paths(_));
            if s.available && is_paths && s.bytes == Some(0) {
                empty += 1;
                continue;
            }
            any = true;
            let b = bytes_of(s);
            if s.available {
                if r.admin && !admin {
                    admin_total += b;
                } else {
                    user_total += b;
                }
            }
            let lock = if r.admin && !admin { " 🔒" } else { "" };
            let name = if s.available { format!("{}{}", r.name, lock) } else { ui::dim(&format!("{}{}", r.name, lock)) };
            let size = match (s.available, s.bytes) {
                (false, _) => ui::dim("—"),
                (_, None) => ui::dim("?"),
                (_, Some(0)) => ui::dim("—"),
                (_, Some(b)) => ui::bold(&ui::human(b)),
            };
            let bar = if s.available && b > 0 { ui::bar(b as f64 / maxb as f64, 14, ui::heat(b as f64 / maxb as f64)) } else { String::new() };
            let mut note = s.note.clone().map(|n| ui::dim(&n)).unwrap_or_default();
            if !s.running.is_empty() {
                note = ui::fg(&format!("⚠ running: {}", s.running.join(", ")), ui::YELLOW);
            }
            t.row(vec![name, risk_badge(r.risk), size, bar, note]);
        }
        if any {
            println!("\n  {} {}", cat.icon(), ui::bold_fg(cat.title(), ui::PURPLE));
            t.print();
        }
    }
    if empty > 0 {
        println!("\n  {}", ui::dim(&format!("{empty} more locations checked — already clean.")));
    }
    (user_total, admin_total)
}

fn start_docker() -> bool {
    let exe = "C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe";
    if !std::path::Path::new(exe).exists() {
        return false;
    }
    if std::process::Command::new(exe).spawn().is_err() {
        return false;
    }
    let sp = ui::spinner("Starting Docker Desktop (can take up to 2 minutes)…");
    for _ in 0..60 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        if winenv::run_ok("docker", &["info", "--format", "{{.ServerVersion}}"]) {
            sp.finish_and_clear();
            ui::ok("Docker is up.");
            return true;
        }
    }
    sp.finish_and_clear();
    ui::warn("Docker did not come up in time.");
    false
}

/// Returns true if the process relaunched itself elevated and this instance should exit.
pub fn clean_interactive(admin: bool) -> bool {
    let all = rules::catalog();
    let sp = ui::spinner(&format!("Scanning {} cleaning rules across your system…", all.len()));
    let mut scans = scan::scan_all(&all);
    sp.finish_and_clear();

    // Docker installed but daemon is off → offer to start it
    let docker_off = all.iter().zip(scans.iter()).any(|(r, s)| matches!(r.kind, Kind::Special(Special::DockerBuildCache)) && s.note.as_deref().map(|n| n.contains("not running")).unwrap_or(false));
    if docker_off && confirm("Docker Desktop is installed but not running. Start it so images/build cache can be cleaned too?", false) && start_docker() {
        scans = scan::scan_all(&all);
    }

    ui::section("Scan result");
    let (user_total, admin_total) = print_scan(&all, &scans, admin);
    println!(
        "\n  {} {}{}",
        ui::bold_fg("Reclaimable now:", ui::CYAN),
        ui::bold_fg(&ui::human(user_total), ui::GREEN),
        if admin_total > 0 && !admin { format!("   {} {}", ui::dim("+ with Administrator:"), ui::bold_fg(&ui::human(admin_total), ui::ORANGE)) } else { String::new() }
    );

    if !admin && admin_total > 0 {
        ui::panel(
            "Administrator unlocks more",
            &[
                format!("{} of system junk (🔒) is out of reach in normal mode: Windows temp & logs,", ui::bold(&ui::human(admin_total))),
                "Windows Update cache, crash dumps, component store (WinSxS) cleanup and more.".into(),
                String::new(),
                ui::dim("Nothing is changed without showing you exactly what will happen first,"),
                ui::dim("and a System Restore point is created before any system-level change."),
            ],
            ui::ORANGE,
        );
        let opts = vec!["Continue without Administrator".to_string(), "Relaunch as Administrator (UAC prompt)".to_string(), "Cancel".to_string()];
        match ask(Select::new("How do you want to continue?", opts).raw_prompt()) {
            Some(c) if c.index == 1 => {
                if winenv::relaunch_as_admin(&["clean"]) {
                    ui::info("Continuing in the elevated window…");
                    return true;
                }
                ui::warn("Elevation was cancelled — continuing in normal mode.");
            }
            Some(c) if c.index == 0 => {}
            _ => return false,
        }
    }

    // build the pick list
    let by: HashMap<&str, &RuleScan> = scans.iter().map(|s| (s.id, s)).collect();
    let cands: Vec<&Rule> = all
        .iter()
        .filter(|r| {
            let s = by[r.id];
            s.available && (admin || !r.admin) && (s.bytes != Some(0) || matches!(r.kind, Kind::Special(_)))
        })
        .collect();
    if cands.is_empty() {
        ui::ok("Nothing to clean — your system is already tidy.");
        return false;
    }
    let labels: Vec<String> = cands
        .iter()
        .map(|r| {
            let s = by[r.id];
            let size = match s.bytes {
                Some(b) if b > 0 => ui::human(b),
                None => "?".into(),
                _ => "—".into(),
            };
            format!("{} {:<44} {:>10}{}", risk_txt(r.risk), ui::truncate(r.name, 44), size, if r.admin { "  [admin]" } else { "" })
        })
        .collect();
    let defaults: Vec<usize> = cands.iter().enumerate().filter(|(_, r)| r.default_on).map(|(i, _)| i).collect();
    println!();
    let Some(picked) = ask(
        MultiSelect::new("Select what to clean", labels)
            .with_default(&defaults)
            .with_page_size(16)
            .with_help_message("space toggle · → all · ← none · enter confirm · SAFE items are pre-selected")
            .with_formatter(&|a| format!("{} item(s) selected", a.len()))
            .raw_prompt(),
    ) else {
        return false;
    };
    let chosen: Vec<&Rule> = picked.iter().map(|p| cands[p.index]).collect();
    if chosen.is_empty() {
        ui::info("Nothing selected.");
        return false;
    }

    // the "tell me first" panel
    let expected: u64 = chosen.iter().map(|r| bytes_of(by[r.id])).sum();
    let mut lines = vec![format!("{} items · about {} to reclaim", ui::bold(&chosen.len().to_string()), ui::bold_fg(&ui::human(expected), ui::GREEN))];
    let notable: Vec<&&Rule> = chosen.iter().filter(|r| r.risk != Risk::Safe || r.admin).collect();
    if !notable.is_empty() {
        lines.push(String::new());
        lines.push(ui::bold("Heads-up on these:"));
        for r in notable {
            let tag = if r.admin { format!("{} {}", risk_badge(r.risk), ui::badge("ADMIN", ui::ORANGE)) } else { risk_badge(r.risk) };
            lines.push(format!("{} {}", tag, ui::bold(r.name)));
            lines.push(format!("      {}", ui::dim(r.effect)));
        }
    }
    if admin && chosen.iter().any(|r| r.admin || r.risk != Risk::Safe) {
        lines.push(String::new());
        lines.push(ui::fg("A System Restore point will be created first.", ui::GREEN));
    }
    println!();
    ui::panel("Plan", &lines, ui::CYAN);

    // apps that lock caches
    let mut running: Vec<String> = chosen.iter().flat_map(|r| by[r.id].running.clone()).collect();
    running.sort();
    running.dedup();
    let mut close_apps = false;
    if !running.is_empty() {
        println!();
        ui::warn(&format!("These apps are running and lock some caches: {}", ui::bold(&running.join(", "))));
        close_apps = confirm("Close them now for a complete clean? (unsaved work in them will be lost)", false);
    }

    println!();
    if !confirm(&format!("Clean {} item(s) now?", chosen.len()), true) {
        ui::info("Cancelled. Nothing was changed.");
        return false;
    }
    let result = views::run_clean(&chosen, &clean::Options { close_apps, restore_point: true }, admin);
    views::print_report(&result);
    false
}

// ───────────────────────────── boost ─────────────────────────────

fn mem_gauge(used: u64, total: u64, cpu: f32) {
    let f = used as f64 / total.max(1) as f64;
    println!("   RAM {} {} / {}   ({:.0}%)", ui::bar(f, 26, ui::heat(f)), ui::bold(&ui::human(used)), ui::human(total), f * 100.0);
    let c = cpu as f64 / 100.0;
    println!("   CPU {} {:.0}%", ui::bar(c, 26, ui::heat(c)), cpu);
}

fn class_badge(c: procs::Class) -> String {
    match c {
        procs::Class::Protected => ui::badge("PROTECTED", ui::GRAY),
        procs::Class::Helper => ui::badge("HELPER", ui::GREEN),
        procs::Class::UserApp => ui::badge("YOUR APP", ui::YELLOW),
    }
}

pub fn boost_interactive(admin: bool) {
    ui::section("Boost · background processes");
    let sp = ui::spinner("Sampling processes (CPU needs a moment to measure)…");
    let snap = procs::snapshot();
    sp.finish_and_clear();
    mem_gauge(snap.used_mem, snap.total_mem, snap.cpu_total);
    println!();

    let mut t = ui::Table::new(&[("Process", false), ("RAM", true), ("CPU", true), ("Class", false), ("Why", false)]);
    for g in snap.groups.iter().take(16) {
        let name = if g.pids.len() > 1 { format!("{} ×{}", g.name, g.pids.len()) } else { g.name.clone() };
        t.row(vec![ui::truncate(&name, 34), ui::bold(&ui::human(g.mem)), format!("{:.1}%", g.cpu), class_badge(g.class), ui::dim(&ui::truncate(&g.reason, 52))]);
    }
    t.print();

    let mut cands: Vec<&procs::Group> = snap.groups.iter().filter(|g| g.class != procs::Class::Protected && g.mem >= 20 * 1024 * 1024).collect();
    cands.sort_by(|a, b| a.class.cmp(&b.class).then(b.mem.cmp(&a.mem)));
    if cands.is_empty() {
        ui::ok("No removable background apps found. You are already lean.");
        return;
    }
    let labels: Vec<String> = cands
        .iter()
        .map(|g| {
            let n = if g.pids.len() > 1 { format!("{} ×{}", g.name, g.pids.len()) } else { g.name.clone() };
            let tag = if g.class == procs::Class::Helper { "HELPER  " } else { "YOUR APP" };
            format!("{} {:<30} {:>9}  {}", tag, ui::truncate(&n, 30), ui::human(g.mem), g.reason)
        })
        .collect();
    let defaults: Vec<usize> = cands.iter().enumerate().filter(|(_, g)| g.class == procs::Class::Helper).map(|(i, _)| i).collect();
    println!();
    ui::info("Protected processes (Windows core, drivers, security engine, your terminal, Claude) are never offered.");
    let Some(picked) = ask(
        MultiSelect::new("Close which apps?", labels)
            .with_default(&defaults)
            .with_page_size(14)
            .with_help_message("HELPER = vendor background junk (pre-selected) · YOUR APP = may hold unsaved work")
            .with_formatter(&|a| format!("{} selected", a.len()))
            .raw_prompt(),
    ) else {
        return;
    };
    if picked.is_empty() {
        return;
    }
    let chosen: Vec<&&procs::Group> = picked.iter().map(|p| &cands[p.index]).collect();
    if chosen.iter().any(|g| g.class == procs::Class::UserApp) {
        ui::warn("You selected your own apps. Closing them discards any unsaved work in them.");
    }
    if !confirm(&format!("Close {} app group(s)?", chosen.len()), true) {
        return;
    }
    let pids: Vec<u32> = chosen.iter().flat_map(|g| g.pids.clone()).collect();
    let killed = procs::kill_pids(&pids);
    std::thread::sleep(std::time::Duration::from_millis(1200));
    let sp = ui::spinner("Re-measuring…");
    let after = procs::snapshot();
    sp.finish_and_clear();
    let freed = snap.used_mem.saturating_sub(after.used_mem);
    ui::section("Result");
    ui::ok(&format!("Closed {} process(es) · RAM freed ≈ {}", killed, ui::bold_fg(&ui::human(freed), ui::GREEN)));
    if killed < pids.len() {
        ui::warn(&format!("{} process(es) could not be closed (Windows services need Administrator, or they already exited)", pids.len() - killed));
    }
    mem_gauge(after.used_mem, after.total_mem, after.cpu_total);
    boost_services(admin);

    println!();
    if confirm("Also trim memory of the remaining apps? (Windows pages out idle memory; apps keep running)", true) {
        let n = procs::trim_unprotected(&after.groups);
        std::thread::sleep(std::time::Duration::from_millis(600));
        let a2 = procs::snapshot();
        ui::ok(&format!("Trimmed {} process(es) · RAM now {} (was {})", n, ui::bold(&ui::human(a2.used_mem)), ui::human(after.used_mem)));
        ui::info("Freed RAM is a snapshot: Windows refills it with cache as you work. That is normal and good.");
    }
    if confirm("Stop helper apps from starting with Windows again? (opens Startup manager)", false) {
        startup_interactive(winenv::is_admin());
    }
}

/// Vendor services that respawn when killed: stop them and switch to Manual start (admin, reversible).
fn boost_services(admin: bool) {
    let running: Vec<&procs::Svc> = procs::BLOAT_SERVICES.iter().filter(|s| procs::service_running(s.name)).collect();
    if running.is_empty() {
        return;
    }
    println!();
    if !admin {
        ui::info(&format!("{} vendor background service(s) are still running (Dell agents). They restart when killed; run as Administrator to stop them for good.", running.len()));
        return;
    }
    let labels: Vec<String> = running.iter().map(|s| format!("{:<26} {}", s.label, s.why)).collect();
    let defaults: Vec<usize> = (0..running.len()).collect();
    let Some(sel) = ask(
        MultiSelect::new("Stop these background services and set them to Manual start?", labels)
            .with_default(&defaults)
            .with_help_message("reversible: `sc config <name> start= auto` · they still run when something needs them")
            .raw_prompt(),
    ) else {
        return;
    };
    for s in sel {
        let svc = running[s.index];
        let before = procs::service_start_type(svc.name);
        if procs::stop_and_set_manual(svc.name) {
            ui::ok(&format!("{} → stopped, Manual start", svc.label));
            println!("      {}", ui::dim(&format!("undo: sc config \"{}\" start= {}", svc.name, before)));
        } else {
            ui::fail(&format!("{}: could not change start type", svc.label));
        }
    }
}

// ───────────────────────────── startup ─────────────────────────────

pub fn print_startup(entries: &[startup::Entry]) {
    let mut t = ui::Table::new(&[("Name", false), ("State", false), ("Scope", false), ("Advice", false)]);
    for e in entries {
        let state = if e.enabled { ui::fg("● enabled", ui::GREEN) } else { ui::dim("○ disabled") };
        t.row(vec![ui::truncate(&e.name, 30), state, ui::dim(e.scope), e.advice.map(|a| ui::fg(a, ui::YELLOW)).unwrap_or_default()]);
    }
    t.print();
}

pub fn startup_interactive(admin: bool) {
    ui::section("Startup · programs that launch with Windows");
    loop {
        let entries = startup::list();
        print_startup(&entries);
        println!("  {}", ui::dim("Disabling is reversible (same switch Task Manager uses). Nothing is deleted."));
        let opts = vec!["Disable entries…".to_string(), "Re-enable entries…".to_string(), "Back".to_string()];
        let Some(a) = ask(Select::new("Startup manager", opts).raw_prompt()) else { return };
        if a.index == 2 {
            return;
        }
        let want_enable = a.index == 1;
        let pool: Vec<&startup::Entry> = entries.iter().filter(|e| e.enabled != want_enable).collect();
        if pool.is_empty() {
            ui::info("Nothing to change here.");
            continue;
        }
        let labels: Vec<String> = pool.iter().map(|e| format!("{:<30} {}{}", ui::truncate(&e.name, 30), e.scope, if e.needs_admin && !admin { "  (needs Administrator)" } else { "" })).collect();
        let defaults: Vec<usize> = if want_enable { vec![] } else { pool.iter().enumerate().filter(|(_, e)| e.advice.is_some() && (admin || !e.needs_admin)).map(|(i, _)| i).collect() };
        let Some(sel) = ask(MultiSelect::new(if want_enable { "Re-enable which?" } else { "Disable which?" }, labels).with_default(&defaults).with_page_size(14).raw_prompt()) else { continue };
        for s in sel {
            let e = pool[s.index];
            if e.needs_admin && !admin {
                ui::warn(&format!("{}: needs Administrator — skipped", e.name));
                continue;
            }
            match startup::set_enabled(e, want_enable) {
                Ok(()) => ui::ok(&format!("{} → {}", e.name, if want_enable { "enabled" } else { "disabled" })),
                Err(err) => ui::fail(&format!("{}: {}", e.name, err)),
            }
        }
    }
}

// ───────────────────────────── analyzer ─────────────────────────────

pub fn analyze_interactive() {
    ui::section("Analyzer");
    views::print_drives();
    let drives = winenv::fixed_drives();
    let mut opts: Vec<String> = drives.iter().map(|d| format!("{}  ({} free)", d.root, ui::human(d.free))).collect();
    opts.push("Enter a folder path…".into());
    opts.push("Back".into());
    let Some(c) = ask(Select::new("Which drive/folder to analyze?", opts).raw_prompt()) else { return };
    let mut path: PathBuf = if c.index < drives.len() {
        PathBuf::from(&drives[c.index].root)
    } else if c.index == drives.len() {
        let Some(p) = ask(Text::new("Folder path:").prompt()) else { return };
        PathBuf::from(p.trim().trim_matches('"'))
    } else {
        return;
    };
    let ann = analyze::Annotator::new();
    loop {
        let sp = ui::spinner(&format!("Measuring {} (this can take a minute on big drives)…", path.display()));
        let a = analyze::analyze_dir(&path, 100 * 1024 * 1024);
        sp.finish_and_clear();
        views::print_analysis(&a, &ann, 18);
        let dirs: Vec<&analyze::Item> = a.items.iter().filter(|i| i.is_dir && i.bytes > 0).take(14).collect();
        let mut opts: Vec<String> = dirs.iter().map(|i| format!("📂 {}  ({})", i.name, ui::human(i.bytes))).collect();
        let up_idx = opts.len();
        opts.push("⬆  Up one level".into());
        opts.push("↩  Back to menu".into());
        let Some(c) = ask(Select::new("Dive deeper?", opts).with_page_size(14).raw_prompt()) else { return };
        if c.index < up_idx {
            path = dirs[c.index].path.clone();
        } else if c.index == up_idx {
            if let Some(p) = path.parent() {
                path = p.to_path_buf();
            }
        } else {
            return;
        }
    }
}

// ───────────────────────────── dev junk ─────────────────────────────

pub fn dev_interactive() {
    ui::section("Dev junk · regenerable project artifacts");
    let drives = winenv::fixed_drives();
    let labels: Vec<String> = drives.iter().map(|d| d.root.clone()).collect();
    let defaults: Vec<usize> = (0..labels.len()).collect();
    let Some(sel) = ask(MultiSelect::new("Where should I look?", labels).with_default(&defaults).with_help_message("system folders (Windows, Program Files, AppData) are always skipped").raw_prompt()) else { return };
    let roots: Vec<PathBuf> = sel.iter().map(|s| PathBuf::from(&drives[s.index].root)).collect();
    let sp = ui::spinner("Hunting for node_modules, target, venv, bytecode…");
    let found = devjunk::find(&roots);
    sp.finish_and_clear();
    if found.is_empty() {
        ui::ok("No build artifacts worth cleaning were found.");
        return;
    }
    let total: u64 = found.iter().map(|j| j.bytes).sum();
    let mut t = ui::Table::new(&[("#", true), ("Size", true), ("Kind", false), ("Idle", true), ("Project", false), ("Rebuild with", false)]);
    for (i, j) in found.iter().take(30).enumerate() {
        t.row(vec![(i + 1).to_string(), ui::bold(&ui::human(j.bytes)), j.kind.to_string(), format!("{}d", j.age_days), ui::truncate(&j.project.display().to_string(), 46), ui::dim(j.rebuild_hint)]);
    }
    t.print();
    println!("  {}", ui::dim(&format!("{} artifacts · {} total{}", found.len(), ui::human(total), if found.len() > 30 { " (top 30 shown)" } else { "" })));

    let labels: Vec<String> = found.iter().map(|j| format!("{:>9}  {:<16} idle {:>4}d  {}", ui::human(j.bytes), j.kind, j.age_days, j.path.display())).collect();
    let defaults: Vec<usize> = found.iter().enumerate().filter(|(_, j)| j.age_days >= 30).map(|(i, _)| i).collect();
    let Some(picked) = ask(
        MultiSelect::new("Delete which? (pre-selected: idle for 30+ days)", labels)
            .with_default(&defaults)
            .with_page_size(16)
            .with_formatter(&|a| format!("{} selected", a.len()))
            .raw_prompt(),
    ) else {
        return;
    };
    if picked.is_empty() {
        return;
    }
    let chosen: Vec<&devjunk::Junk> = picked.iter().map(|p| &found[p.index]).collect();
    let sum: u64 = chosen.iter().map(|j| j.bytes).sum();
    ui::panel("Plan", &[format!("Delete {} folder(s) · {} — all of it regenerable by rebuilding/reinstalling.", chosen.len(), ui::bold_fg(&ui::human(sum), ui::GREEN)), ui::dim("Links/junctions inside are unlinked, never followed.")], ui::CYAN);
    if !confirm("Proceed?", true) {
        return;
    }
    let pb = ui::progress(chosen.len() as u64, "");
    let (mut freed, mut locked) = (0u64, 0u64);
    for j in &chosen {
        pb.set_message(j.path.display().to_string());
        let d = devjunk::delete(j);
        freed += d.bytes;
        locked += d.locked;
        pb.inc(1);
    }
    pb.finish_and_clear();
    ui::ok(&format!("Freed {}{}", ui::bold_fg(&ui::human(freed), ui::GREEN), if locked > 0 { format!(" ({} files in use were skipped)", locked) } else { String::new() }));
}

// ───────────────────────────── catalog ─────────────────────────────

pub fn catalog_view() {
    ui::section("Catalog · everything Sapu Jagat can clean");
    ui::info("Allow-list design: anything not listed here is never touched. Cookies, passwords, documents, registry: never.");
    for cat in CATS {
        println!("\n  {} {}", cat.icon(), ui::bold_fg(cat.title(), ui::PURPLE));
        for r in rules::catalog().iter().filter(|r| r.cat == cat) {
            let admin = if r.admin { format!(" {}", ui::badge("ADMIN", ui::ORANGE)) } else { String::new() };
            println!("   {} {}{}", risk_badge(r.risk), ui::bold(r.name), admin);
            println!("      {}", ui::dim(r.what));
            println!("      {} {}", ui::fg("effect:", ui::GRAY), r.effect);
        }
    }
    println!();
}
