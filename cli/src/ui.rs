//! Terminal look & feel: truecolor, rounded boxes, tables, bars, badges.

use console::{measure_text_width, Term};

pub type Rgb = (u8, u8, u8);

pub const CYAN: Rgb = (56, 218, 255);
pub const BLUE: Rgb = (96, 140, 255);
pub const PURPLE: Rgb = (178, 120, 255);
pub const PINK: Rgb = (255, 105, 180);
pub const GREEN: Rgb = (80, 230, 140);
pub const YELLOW: Rgb = (255, 205, 80);
pub const ORANGE: Rgb = (255, 150, 70);
pub const RED: Rgb = (255, 90, 100);
pub const GRAY: Rgb = (130, 138, 155);
pub const WHITE: Rgb = (235, 238, 245);

pub fn init_console() {
    sapu_core::winenv::enable_vt();
}

pub fn fg(s: &str, c: Rgb) -> String {
    format!("\x1b[38;2;{};{};{}m{}\x1b[0m", c.0, c.1, c.2, s)
}
pub fn bold(s: &str) -> String {
    format!("\x1b[1m{s}\x1b[0m")
}
pub fn dim(s: &str) -> String {
    fg(s, GRAY)
}
pub fn bold_fg(s: &str, c: Rgb) -> String {
    format!("\x1b[1;38;2;{};{};{}m{}\x1b[0m", c.0, c.1, c.2, s)
}

pub fn gradient(text: &str, a: Rgb, b: Rgb) -> String {
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len().max(2) - 1;
    let mut out = String::new();
    for (i, ch) in chars.iter().enumerate() {
        let t = i as f32 / n as f32;
        let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
        out.push_str(&format!("\x1b[1;38;2;{};{};{}m{}", l(a.0, b.0), l(a.1, b.1), l(a.2, b.2), ch));
    }
    out.push_str("\x1b[0m");
    out
}

pub fn term_width() -> usize {
    (Term::stdout().size().1 as usize).clamp(78, 130)
}

pub fn human(b: u64) -> String {
    const U: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} B", b)
    } else if v >= 100.0 {
        format!("{:.0} {}", v, U[i])
    } else if v >= 10.0 {
        format!("{:.1} {}", v, U[i])
    } else {
        format!("{:.2} {}", v, U[i])
    }
}

pub fn pad(s: &str, w: usize, right: bool) -> String {
    let len = measure_text_width(s);
    if len >= w {
        return s.to_string();
    }
    let sp = " ".repeat(w - len);
    if right {
        format!("{sp}{s}")
    } else {
        format!("{s}{sp}")
    }
}

pub fn truncate(s: &str, max: usize) -> String {
    if measure_text_width(s) <= max {
        return s.to_string();
    }
    let mut out = String::new();
    for ch in s.chars() {
        if measure_text_width(&out) + 1 >= max {
            break;
        }
        out.push(ch);
    }
    out.push('…');
    out
}

/// Horizontal bar with 1/8 block precision.
pub fn bar(frac: f64, width: usize, c: Rgb) -> String {
    const PART: [char; 8] = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉'];
    let frac = frac.clamp(0.0, 1.0);
    let eighths = (frac * width as f64 * 8.0).round() as usize;
    let full = eighths / 8;
    let rem = eighths % 8;
    let mut s = "█".repeat(full);
    if rem > 0 && full < width {
        s.push(PART[rem]);
    }
    let used = measure_text_width(&s);
    let empty = width.saturating_sub(used);
    format!("{}{}", fg(&s, c), fg(&"░".repeat(empty), (55, 60, 75)))
}

pub fn heat(frac: f64) -> Rgb {
    if frac > 0.85 {
        RED
    } else if frac > 0.65 {
        ORANGE
    } else if frac > 0.4 {
        YELLOW
    } else {
        GREEN
    }
}

pub fn banner(admin: bool) {
    let rows = [
        ["╔═╗", "╔═╗", "╔═╗", "╦ ╦", "  ", " ╦", "╔═╗", "╔═╗", "╔═╗", "╔╦╗"],
        ["╚═╗", "╠═╣", "╠═╝", "║ ║", "  ", " ║", "╠═╣", "║ ╦", "╠═╣", " ║ "],
        ["╚═╝", "╩ ╩", "╩  ", "╚═╝", "  ", "╚╝", "╩ ╩", "╚═╝", "╩ ╩", " ╩ "],
    ];
    println!();
    for r in rows {
        let line = r.concat();
        println!("   {}", gradient(&line, CYAN, PINK));
    }
    println!(
        "   {}",
        dim("Deep clean  ·  RAM/CPU boost  ·  Disk analyzer  ·  safe by design")
    );
    let mode = if admin {
        bold_fg(" ● ADMIN ", GREEN)
    } else {
        bold_fg(" ● USER ", YELLOW)
    };
    println!("   {}\n", mode);
}

pub fn section(title: &str) {
    let w = term_width();
    let t = format!(" {} ", title);
    let left = fg("──", PURPLE);
    let right_len = w.saturating_sub(measure_text_width(&t) + 6);
    println!("\n{}{}{}", left, bold_fg(&t, CYAN), fg(&"─".repeat(right_len), (70, 75, 100)));
}

pub fn ok(msg: &str) {
    println!("  {} {}", fg("✔", GREEN), msg);
}
pub fn warn(msg: &str) {
    println!("  {} {}", fg("▲", YELLOW), msg);
}
pub fn fail(msg: &str) {
    println!("  {} {}", fg("✖", RED), msg);
}
pub fn info(msg: &str) {
    println!("  {} {}", fg("●", BLUE), msg);
}

/// Boxed panel with a coloured title.
pub fn panel(title: &str, lines: &[String], c: Rgb) {
    let w = term_width() - 4;
    let inner = w - 2;
    let t = format!(" {} ", title);
    println!("  {}{}{}", fg("╭", c), fg(&format!("─{}", bold_fg(&t, c)), c), fg(&format!("{}╮", "─".repeat(inner.saturating_sub(measure_text_width(&t) + 1))), c));
    for l in lines {
        for wrapped in wrap(l, inner - 2) {
            println!("  {} {} {}", fg("│", c), pad(&wrapped, inner - 2, false), fg("│", c));
        }
    }
    println!("  {}", fg(&format!("╰{}╯", "─".repeat(inner)), c));
}

/// Word-wrap that is ANSI-width aware (keeps escape codes intact because it only splits on spaces).
pub fn wrap(s: &str, width: usize) -> Vec<String> {
    if measure_text_width(s) <= width {
        return vec![s.to_string()];
    }
    let indent = s.len() - s.trim_start().len();
    let pre = " ".repeat(indent);
    let mut lines = Vec::new();
    let mut cur = String::new();
    for word in s.split(' ') {
        if word.is_empty() {
            continue;
        }
        let candidate_len = measure_text_width(&cur) + 1 + measure_text_width(word);
        if !cur.is_empty() && candidate_len > width {
            lines.push(std::mem::take(&mut cur));
            cur = format!("{pre}{word}");
        } else if cur.is_empty() {
            cur = if lines.is_empty() { format!("{pre}{word}") } else { format!("{pre}{word}") };
        } else {
            cur.push(' ');
            cur.push_str(word);
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

pub struct Table {
    headers: Vec<String>,
    right: Vec<bool>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new(cols: &[(&str, bool)]) -> Self {
        Table {
            headers: cols.iter().map(|c| c.0.to_string()).collect(),
            right: cols.iter().map(|c| c.1).collect(),
            rows: Vec::new(),
        }
    }
    pub fn row(&mut self, cells: Vec<String>) {
        self.rows.push(cells);
    }
    pub fn print(&self) {
        let n = self.headers.len();
        let mut w: Vec<usize> = self.headers.iter().map(|h| measure_text_width(h)).collect();
        for r in &self.rows {
            for (i, c) in r.iter().enumerate().take(n) {
                w[i] = w[i].max(measure_text_width(c));
            }
        }
        let line = |l: &str, m: &str, r: &str| {
            let parts: Vec<String> = w.iter().map(|x| "─".repeat(x + 2)).collect();
            fg(&format!("{}{}{}", l, parts.join(m), r), (70, 75, 100))
        };
        let bar_c = fg("│", (70, 75, 100));
        println!("  {}", line("╭", "┬", "╮"));
        let h: Vec<String> = self.headers.iter().enumerate().map(|(i, h)| format!(" {} ", pad(&bold_fg(h, CYAN), w[i], self.right[i]))).collect();
        println!("  {}{}{}", bar_c, h.join(&bar_c), bar_c);
        println!("  {}", line("├", "┼", "┤"));
        for r in &self.rows {
            let cells: Vec<String> = (0..n).map(|i| format!(" {} ", pad(r.get(i).map(String::as_str).unwrap_or(""), w[i], self.right[i]))).collect();
            println!("  {}{}{}", bar_c, cells.join(&bar_c), bar_c);
        }
        println!("  {}", line("╰", "┴", "╯"));
    }
}

pub fn badge(text: &str, c: Rgb) -> String {
    format!("\x1b[1;38;2;{};{};{}m[{}]\x1b[0m", c.0, c.1, c.2, text)
}

pub fn spinner(msg: &str) -> indicatif::ProgressBar {
    let pb = indicatif::ProgressBar::new_spinner();
    pb.set_style(
        indicatif::ProgressStyle::with_template("  {spinner:.cyan} {msg}")
            .unwrap()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "✔"]),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}

pub fn progress(len: u64, msg: &str) -> indicatif::ProgressBar {
    let pb = indicatif::ProgressBar::new(len);
    pb.set_style(
        indicatif::ProgressStyle::with_template("  {spinner:.cyan} {bar:32.cyan/blue} {pos}/{len}  {msg}")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏░")
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏", "✔"]),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(std::time::Duration::from_millis(80));
    pb
}
