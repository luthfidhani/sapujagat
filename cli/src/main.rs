mod menu;
mod ui;
mod views;

use std::collections::HashMap;
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use sapu_core::{analyze, clean, devjunk, procs, rules, scan, startup, winenv};

#[derive(Parser)]
#[command(name = "sapu", version, about = "Sapu Jagat — deep, safe Windows cleaner, booster and disk analyzer")]
struct Cli {
    /// internal: set when re-launched through UAC
    #[arg(long, hide = true)]
    elevated: bool,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Scan for junk and clean it (interactive when no flags are given)
    Clean {
        /// show what would be cleaned, change nothing
        #[arg(long)]
        dry_run: bool,
        /// do not ask for confirmation
        #[arg(short, long)]
        yes: bool,
        /// also include CAREFUL rules that have a visible side effect
        #[arg(long)]
        include_careful: bool,
        /// only these rule ids (comma separated, see `sapu rules`)
        #[arg(long, value_delimiter = ',')]
        only: Vec<String>,
        /// close apps that lock caches (browsers, Steam...)
        #[arg(long)]
        close_apps: bool,
        /// skip the System Restore point
        #[arg(long)]
        no_restore_point: bool,
    },
    /// Read-only scan report
    Scan,
    /// Disk analyzer for a folder or drive
    Analyze {
        path: Option<String>,
        #[arg(long, default_value_t = 20)]
        top: usize,
    },
    /// Background processes: show, close, trim
    Boost,
    /// Manage startup entries
    Startup {
        /// just print the list
        #[arg(long)]
        list: bool,
    },
    /// Find node_modules / target / venv of old projects
    Dev {
        #[arg(long)]
        clean: bool,
        #[arg(long, default_value_t = 30)]
        older_than: u64,
        #[arg(short, long)]
        yes: bool,
        roots: Vec<String>,
    },
    /// List everything the cleaner knows, with risk and side effects
    Rules,
}

fn main() {
    ui::init_console();
    let cli = Cli::parse();
    let admin = winenv::is_admin();
    let interactive = std::io::stdin().is_terminal();

    sapu_core::settings::init();
    let mut relaunched = false;
    match cli.cmd {
        None => {
            ui::banner(admin);
            menu::system_header();
            println!();
            if interactive {
                menu::main_menu(admin);
            } else {
                println!("  Not an interactive terminal. Try: sapu --help");
            }
        }
        Some(Cmd::Clean { dry_run, yes, include_careful, only, close_apps, no_restore_point }) => {
            ui::banner(admin);
            if !dry_run && !yes && only.is_empty() && !include_careful && interactive {
                relaunched = menu::clean_interactive(admin);
            } else {
                clean_cli(admin, dry_run, yes, include_careful, only, close_apps, no_restore_point);
            }
        }
        Some(Cmd::Scan) => {
            ui::banner(admin);
            let all = rules::catalog();
            let sp = ui::spinner("Scanning…");
            let scans = scan::scan_all(&all);
            sp.finish_and_clear();
            let (u, a) = menu::print_scan(&all, &scans, admin);
            println!("\n  Reclaimable now: {}   (+{} with Administrator)", ui::human(u), ui::human(a));
        }
        Some(Cmd::Analyze { path, top }) => {
            ui::banner(admin);
            views::print_drives();
            let p = path.map(PathBuf::from).unwrap_or_else(|| PathBuf::from("C:\\"));
            let sp = ui::spinner(&format!("Measuring {}…", p.display()));
            let a = analyze::analyze_dir(&p, 100 * 1024 * 1024);
            sp.finish_and_clear();
            views::print_analysis(&a, &analyze::Annotator::new(), top);
        }
        Some(Cmd::Boost) => {
            ui::banner(admin);
            if interactive {
                menu::boost_interactive(admin);
            } else {
                let snap = procs::snapshot();
                for g in snap.groups.iter().take(25) {
                    println!("{:>9}  {:>5.1}%  {:?}  {}  ({})", ui::human(g.mem), g.cpu, g.class, g.name, g.reason);
                }
            }
        }
        Some(Cmd::Startup { list }) => {
            ui::banner(admin);
            if list || !interactive {
                menu::print_startup(&startup::list());
            } else {
                menu::startup_interactive(admin);
            }
        }
        Some(Cmd::Dev { clean, older_than, yes, roots }) => {
            ui::banner(admin);
            if !clean && roots.is_empty() && interactive {
                menu::dev_interactive();
            } else {
                dev_cli(roots, clean, older_than, yes);
            }
        }
        Some(Cmd::Rules) => {
            ui::banner(admin);
            menu::catalog_view();
        }
    }

    if cli.elevated && !relaunched {
        println!("\n  Press Enter to close this window…");
        let _ = std::io::stdin().read(&mut [0u8]);
    }
    if relaunched {
        std::process::exit(0);
    }
}

fn clean_cli(admin: bool, dry_run: bool, yes: bool, include_careful: bool, only: Vec<String>, close_apps: bool, no_restore_point: bool) {
    let all = rules::catalog();
    let sp = ui::spinner("Scanning…");
    let scans = scan::scan_all(&all);
    sp.finish_and_clear();
    let by: HashMap<&str, &scan::RuleScan> = scans.iter().map(|s| (s.id, s)).collect();

    let mut chosen: Vec<&rules::Rule> = Vec::new();
    for r in &all {
        let s = by[r.id];
        let wanted = if !only.is_empty() { only.iter().any(|o| o == r.id) } else { r.default_on || (include_careful && r.risk == rules::Risk::Careful) };
        if wanted && !s.available {
            ui::warn(&format!("skipping '{}': {}", r.name, s.note.clone().unwrap_or_else(|| "not available on this PC".into())));
            continue;
        }
        if !wanted || !s.available {
            continue;
        }
        if r.admin && !admin {
            ui::warn(&format!("skipping '{}' — needs Administrator", r.name));
            continue;
        }
        chosen.push(r);
    }
    if chosen.is_empty() {
        ui::ok("Nothing to clean.");
        return;
    }
    let mut t = ui::Table::new(&[("Rule", false), ("Id", false), ("Risk", false), ("Size", true)]);
    let mut total = 0u64;
    for r in &chosen {
        let b = by[r.id].bytes.unwrap_or(0);
        total += b;
        t.row(vec![r.name.to_string(), ui::dim(r.id), menu::risk_badge(r.risk), if by[r.id].bytes.is_some() { ui::human(b) } else { "?".into() }]);
    }
    t.print();
    println!("  Total: {}", ui::bold_fg(&ui::human(total), ui::GREEN));
    if dry_run {
        ui::info("Dry run — nothing was changed.");
        return;
    }
    if !yes && !inquire::Confirm::new("Proceed?").with_default(false).prompt().unwrap_or(false) {
        return;
    }
    let rep = views::run_clean(&chosen, &clean::Options { close_apps, restore_point: !no_restore_point }, admin);
    views::print_report(&rep);
}

fn dev_cli(roots: Vec<String>, clean: bool, older_than: u64, yes: bool) {
    let roots: Vec<PathBuf> = if roots.is_empty() { winenv::fixed_drives().into_iter().map(|d| PathBuf::from(d.root)).collect() } else { roots.into_iter().map(PathBuf::from).collect() };
    let sp = ui::spinner("Hunting for build artifacts…");
    let found = devjunk::find(&roots);
    sp.finish_and_clear();
    let mut t = ui::Table::new(&[("Size", true), ("Kind", false), ("Idle", true), ("Path", false)]);
    for j in found.iter().take(40) {
        t.row(vec![ui::human(j.bytes), j.kind.to_string(), format!("{}d", j.age_days), ui::truncate(&j.path.display().to_string(), 70)]);
    }
    t.print();
    if !clean {
        return;
    }
    let sel: Vec<&devjunk::Junk> = found.iter().filter(|j| j.age_days >= older_than).collect();
    let sum: u64 = sel.iter().map(|j| j.bytes).sum();
    println!("  {} folder(s) idle ≥ {}d · {}", sel.len(), older_than, ui::human(sum));
    if sel.is_empty() || (!yes && !inquire::Confirm::new("Delete them?").with_default(false).prompt().unwrap_or(false)) {
        return;
    }
    let freed: u64 = sel.iter().map(|j| devjunk::delete(j).bytes).sum();
    ui::ok(&format!("Freed {}", ui::human(freed)));
}
