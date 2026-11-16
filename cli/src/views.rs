//! Terminal rendering of engine data (tables, reports, progress).

use sapu_core::analyze::{Analysis, Annotator, TagKind};
use sapu_core::clean::{self, Event, Options, Report};
use sapu_core::rules::Rule;
use sapu_core::winenv;

use crate::ui;

pub fn tag_color(k: TagKind) -> ui::Rgb {
    match k {
        TagKind::Cleanable | TagKind::Contains => ui::GREEN,
        TagKind::Danger => ui::RED,
        TagKind::Info => ui::BLUE,
        TagKind::Review => ui::YELLOW,
        TagKind::Warn => ui::ORANGE,
        TagKind::Protected => ui::PURPLE,
    }
}

pub fn print_drives() {
    ui::section("Drives");
    let mut t = ui::Table::new(&[("Drive", false), ("Usage", false), ("Used", true), ("Free", true), ("Total", true)]);
    for d in winenv::fixed_drives() {
        let used = d.total - d.free;
        let f = used as f64 / d.total as f64;
        t.row(vec![
            ui::bold_fg(&d.root, ui::WHITE),
            format!("{} {:>3.0}%", ui::bar(f, 24, ui::heat(f)), f * 100.0),
            ui::human(used),
            ui::bold_fg(&ui::human(d.free), if d.free < 20 * 1024 * 1024 * 1024 { ui::ORANGE } else { ui::GREEN }),
            ui::human(d.total),
        ]);
    }
    t.print();
}

pub fn print_analysis(a: &Analysis, ann: &Annotator, top: usize) {
    ui::section(&format!("Space map · {}", a.root.display()));
    let max = a.items.first().map(|i| i.bytes).unwrap_or(1).max(1);
    let mut t = ui::Table::new(&[("#", true), ("Size", true), ("Share", false), ("%", true), ("Name", false), ("What / advice", false)]);
    for (i, it) in a.items.iter().take(top).enumerate() {
        let disp = if it.is_dir { format!("{}\\", it.name) } else { it.name.clone() };
        let pct = if a.total > 0 { it.bytes as f64 * 100.0 / a.total as f64 } else { 0.0 };
        let advice = ann.tag(&it.path).map(|t| ui::fg(&t.text, tag_color(t.kind))).unwrap_or_default();
        t.row(vec![
            (i + 1).to_string(),
            ui::bold(&ui::human(it.bytes)),
            ui::bar(it.bytes as f64 / max as f64, 18, ui::heat(pct / 100.0)),
            format!("{pct:.1}"),
            ui::truncate(&disp, 40),
            advice,
        ]);
    }
    t.print();
    println!("  {}", ui::dim(&format!("Total here: {}  ·  {} items", ui::human(a.total), a.items.len())));
    if a.unreadable > 0 {
        ui::warn(&format!("{} item(s) could not be read (run as Administrator for a complete picture)", a.unreadable));
    }
    if !a.big_files.is_empty() {
        println!();
        println!("  {}", ui::bold_fg("Largest single files", ui::CYAN));
        for f in &a.big_files {
            let tag = ann.tag(&f.path).map(|t| format!("  {}", ui::fg(&t.text, tag_color(t.kind)))).unwrap_or_default();
            println!("   {}  {}{}", ui::pad(&ui::bold(&ui::human(f.bytes)), 9, true), ui::truncate(&f.path.display().to_string(), ui::term_width() - 30), tag);
        }
    }
}

/// Runs the cleaning engine and renders its progress events as a terminal progress bar.
pub fn run_clean(rules: &[&Rule], opts: &Options, admin: bool) -> Report {
    let mut pb: Option<indicatif::ProgressBar> = None;
    let report = clean::execute(rules, opts, admin, &mut |ev| match ev {
        Event::Stage { message } => {
            if let Some(p) = &pb {
                p.println(format!("  {} {}", ui::fg("●", ui::BLUE), message));
            } else {
                ui::info(&message);
            }
        }
        Event::RestorePoint { ok } => {
            if ok {
                ui::ok("Restore point created: \"Sapu Jagat\"");
            } else {
                ui::warn("Could not create a restore point — continuing only with non-system changes");
            }
        }
        Event::RuleStart { total, name, .. } => {
            let p = pb.get_or_insert_with(|| ui::progress(total as u64, ""));
            p.set_message(name);
        }
        Event::RuleDone { .. } => {
            if let Some(p) = &pb {
                p.inc(1);
            }
        }
    });
    if let Some(p) = pb {
        p.finish_and_clear();
    }
    report
}

pub fn print_report(rep: &Report) {
    ui::section("Result");
    let mut t = ui::Table::new(&[("What", false), ("Freed", true), ("Items", true), ("Status", false)]);
    let mut rows: Vec<&clean::RuleResult> = rep.results.iter().collect();
    rows.sort_by(|a, b| b.freed.cmp(&a.freed));
    for r in rows {
        let status = if !r.ok {
            ui::fg(&format!("✖ {}", r.note), ui::RED)
        } else if r.locked > 0 {
            ui::fg(&format!("✔ partial — {}", r.note), ui::YELLOW)
        } else if r.note.starts_with("skipped") {
            ui::fg(&format!("▲ {}", r.note), ui::YELLOW)
        } else if !r.note.is_empty() {
            ui::fg(&format!("✔ {}", r.note), ui::GREEN)
        } else {
            ui::fg("✔ done", ui::GREEN)
        };
        let freed = if r.freed > 0 { ui::bold_fg(&ui::human(r.freed), ui::GREEN) } else { ui::dim("—") };
        t.row(vec![r.name.clone(), freed, r.files.to_string(), status]);
    }
    t.print();

    println!();
    for (b, a) in rep.before.iter().zip(rep.after.iter()) {
        let gained = a.free as i128 - b.free as i128;
        let used_frac = 1.0 - a.free as f64 / a.total as f64;
        let delta = if gained >= 0 { format!("+{}", ui::human(gained as u64)) } else { format!("-{}", ui::human((-gained) as u64)) };
        println!(
            "  {}  {}  {} free of {}  {}",
            ui::bold_fg(&b.root, ui::WHITE),
            ui::bar(used_frac, 28, ui::heat(used_frac)),
            ui::bold(&ui::human(a.free)),
            ui::human(a.total),
            if gained > 0 { ui::bold_fg(&delta, ui::GREEN) } else { ui::dim(&delta) }
        );
    }
    println!("\n  {} {}", ui::bold_fg("Total reclaimed by cleaners:", ui::CYAN), ui::bold_fg(&ui::human(rep.total_freed), ui::GREEN));
    println!("  {}", ui::dim(&format!("Log saved in {}", clean::log_dir().display())));
}
