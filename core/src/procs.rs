//! Background process manager: classify, show, close, trim, and a 1-second live sampler.
//!
//! Classification is deliberately conservative:
//!   Protected -> never offered (Windows core, drivers, security engine, your terminal, Claude, this tool)
//!   Helper    -> vendor background helpers / updaters / trays that are safe to close (pre-selected)
//!   UserApp   -> anything else you launched yourself (listed, never pre-selected: may hold unsaved work)

use std::collections::{HashMap, HashSet};

use serde::Serialize;
use sysinfo::{Networks, Pid, System};

use crate::winenv;

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Serialize)]
pub enum Class {
    Protected,
    Helper,
    UserApp,
}

/// One process inside a group (e.g. one of the 93 svchost.exe).
#[derive(Clone, Debug, Serialize)]
pub struct Member {
    pub pid: u32,
    pub mem: u64,
    pub cpu: f32,
}

#[derive(Clone, Debug, Serialize)]
pub struct Group {
    pub name: String,
    pub exe: String,
    pub pids: Vec<u32>,
    pub members: Vec<Member>,
    pub mem: u64,
    pub cpu: f32,
    /// bytes read + written since the previous sample
    pub disk: u64,
    pub class: Class,
    pub reason: String,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub groups: Vec<Group>,
    pub total_mem: u64,
    pub used_mem: u64,
    pub cpu_total: f32,
}

const PROTECTED: &[&str] = &[
    "system", "registry", "smss", "csrss", "wininit", "winlogon", "services", "lsass", "svchost", "dwm", "explorer",
    "fontdrvhost", "sihost", "taskhostw", "ctfmon", "runtimebroker", "shellexperiencehost", "startmenuexperiencehost",
    "searchhost", "searchindexer", "textinputhost", "audiodg", "msmpeng", "nissrv", "securityhealthservice",
    "securityhealthsystray", "conhost", "windowsterminal", "openconsole", "wt", "powershell", "pwsh", "cmd", "claude",
    "sapu", "sapu-gui", "nvdisplay.container", "nvcontainer", "dllhost", "wudfhost", "spoolsv", "lsm", "dashost", "wmiprvse",
    "applicationframehost", "systemsettings", "memory compression", "secure system", "system idle process", "wavessvc",
    "rtkauduservice64", "rtkaudservice64", "vmmem", "vmcompute", "vmwp", "claudevm", "coworkvmservice",
];

/// (substring of lowercase process name, reason)
const HELPERS: &[(&str, &str)] = &[
    ("dell.", "Dell background helper (telemetry / update agent)"),
    ("dellclient", "Dell client management agent"),
    ("supportassist", "Dell SupportAssist agent"),
    ("dellcommand", "Dell Command agent"),
    ("killer", "Killer network suite UI (networking works without it)"),
    ("avastui", "Avast tray UI (the protection engine keeps running)"),
    ("uninstallmonitor", "IObit uninstall monitor"),
    ("iobit", "IObit helper"),
    ("advancedsystemcare", "Advanced SystemCare helper"),
    ("lghub", "Logitech G Hub helper"),
    ("logi", "Logitech helper"),
    ("adobe", "Adobe background helper"),
    ("creative cloud", "Adobe Creative Cloud helper"),
    ("ccxprocess", "Adobe CC helper"),
    ("armsvc", "Adobe updater"),
    ("googleupdate", "Google updater"),
    ("googlecrashhandler", "Google crash reporter"),
    ("microsoftedgeupdate", "Edge updater"),
    ("ituneshelper", "iTunes helper"),
    ("applemobiledevice", "Apple device helper"),
    ("pdfmonitor", "PDFgear background monitor"),
    ("gamebar", "Xbox Game Bar helper"),
    ("gamingservices", "Xbox gaming services helper"),
    ("widgets", "Windows Widgets"),
    ("phoneexperiencehost", "Phone Link helper"),
    ("crossdevice", "Phone Link helper"),
    ("onedrive", "OneDrive sync client (restarts at next sign-in)"),
    ("officeclicktorun", "Office update service helper"),
];

/// Windows services that are pure vendor background agents. They respawn when merely killed, so the
/// only effective way to quiet them is stop + set to Manual start (reversible; needs Administrator).
pub struct Svc {
    pub name: &'static str,
    pub label: &'static str,
    pub why: &'static str,
}

pub const BLOAT_SERVICES: &[Svc] = &[
    Svc { name: "DellTechHub", label: "Dell TechHub", why: "Dell hub / telemetry agent" },
    Svc { name: "DellClientManagementService", label: "Dell Client Management", why: "Dell remote-management agent" },
    Svc { name: "Dell Digital Delivery Services", label: "Dell Digital Delivery", why: "Dell software-delivery agent" },
];

pub fn service_running(name: &str) -> bool {
    winenv::run_text("sc", &["query", name]).map(|t| t.contains("RUNNING")).unwrap_or(false)
}

pub fn service_start_type(name: &str) -> String {
    let t = winenv::run_text("sc", &["qc", name]).unwrap_or_default();
    if t.contains("AUTO_START") {
        "auto".into()
    } else if t.contains("DEMAND_START") {
        "demand".into()
    } else if t.contains("DISABLED") {
        "disabled".into()
    } else {
        "auto".into()
    }
}

/// Set the service to Manual start and stop it. Returns true if the start type was changed.
pub fn stop_and_set_manual(name: &str) -> bool {
    let changed = winenv::run_ok("sc", &["config", name, "start=", "demand"]);
    let _ = winenv::run("sc", &["stop", name]);
    changed
}

pub fn classify_name(name_lc: &str) -> Option<(Class, &'static str)> {
    let n = name_lc.trim_end_matches(".exe");
    if n == "msedgewebview2" {
        return Some((Class::Protected, "Shared web runtime used by other apps (incl. Claude, Widgets)"));
    }
    if PROTECTED.contains(&n) {
        return Some((Class::Protected, "Core Windows / driver / your terminal"));
    }
    if (n.starts_with("asw") || n.starts_with("avast")) && !n.contains("avastui") {
        return Some((Class::Protected, "Security engine"));
    }
    for (frag, why) in HELPERS {
        if n.contains(frag) {
            return Some((Class::Helper, why));
        }
    }
    None
}

fn ancestors(sys: &System) -> HashSet<u32> {
    let mut set = HashSet::new();
    let mut cur = Some(Pid::from_u32(std::process::id()));
    while let Some(p) = cur {
        if !set.insert(p.as_u32()) {
            break;
        }
        cur = sys.process(p).and_then(|pr| pr.parent());
    }
    set
}

/// Without Administrator Windows hides the path of most system services. Built-in ones live in System32,
/// so for display (icon + path) we look for the same file name there. Never used for classification.
fn guess_system_exe(name: &str) -> String {
    use std::sync::{Mutex, OnceLock};
    static CACHE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(v) = cache.lock().unwrap().get(name) {
        return v.clone();
    }
    let found = ["C:\\Windows\\System32\\", "C:\\Windows\\"]
        .iter()
        .map(|d| format!("{d}{name}"))
        .find(|p| name.to_lowercase().ends_with(".exe") && std::path::Path::new(p).is_file())
        .unwrap_or_default();
    cache.lock().unwrap().insert(name.to_string(), found.clone());
    found
}

fn build_groups(sys: &System, anc: &HashSet<u32>) -> Vec<Group> {
    let ncpu = sys.cpus().len().max(1) as f32;
    let mut map: HashMap<String, Group> = HashMap::new();
    for (pid, p) in sys.processes() {
        let name = p.name().to_string();
        let lc = name.to_lowercase();
        let exe_path = p.exe().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
        let in_windows = exe_path.to_lowercase().starts_with("c:\\windows\\");
        let (class, reason) = if anc.contains(&pid.as_u32()) {
            (Class::Protected, "Runs this session (parent of Sapu Jagat)".to_string())
        } else if let Some((c, r)) = classify_name(&lc) {
            (c, r.to_string())
        } else if in_windows {
            (Class::Protected, "Windows component".to_string())
        } else if exe_path.is_empty() {
            (Class::Protected, "System process (not inspectable)".to_string())
        } else {
            (Class::UserApp, "Application you launched".to_string())
        };
        let du = p.disk_usage();
        let shown_exe = if exe_path.is_empty() { guess_system_exe(&name) } else { exe_path };
        let g = map.entry(lc).or_insert(Group { name, exe: shown_exe, pids: vec![], members: vec![], mem: 0, cpu: 0.0, disk: 0, class, reason });
        g.pids.push(pid.as_u32());
        g.members.push(Member { pid: pid.as_u32(), mem: p.memory(), cpu: p.cpu_usage() / ncpu });
        g.mem += p.memory();
        g.cpu += p.cpu_usage() / ncpu;
        g.disk += du.read_bytes + du.written_bytes;
        // a group is only as unprotected as its most protected member
        if class < g.class {
            g.class = class;
        }
    }
    let mut groups: Vec<Group> = map.into_values().collect();
    for g in &mut groups {
        g.members.sort_by(|a, b| b.mem.cmp(&a.mem));
    }
    groups.sort_by(|a, b| b.mem.cmp(&a.mem));
    groups
}

#[derive(Serialize)]
pub struct SysInfo {
    pub cpu: String,
    pub cores: usize,
    pub ram_used: u64,
    pub ram_total: u64,
    pub host: String,
}

pub fn sys_info() -> SysInfo {
    let mut sys = System::new();
    sys.refresh_memory();
    sys.refresh_cpu();
    SysInfo {
        cpu: sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default(),
        cores: sys.cpus().len(),
        ram_used: sys.used_memory(),
        ram_total: sys.total_memory().max(1),
        host: System::host_name().unwrap_or_default(),
    }
}

/// One-shot snapshot (blocks ~0.5 s so CPU can be measured). The GUI uses `Sampler` instead.
pub fn snapshot() -> Snapshot {
    let mut sys = System::new();
    sys.refresh_cpu();
    sys.refresh_processes();
    std::thread::sleep(std::time::Duration::from_millis(500));
    sys.refresh_cpu();
    sys.refresh_processes();
    sys.refresh_memory();
    let anc = ancestors(&sys);
    let groups = build_groups(&sys, &anc);
    Snapshot { groups, total_mem: sys.total_memory(), used_mem: sys.used_memory(), cpu_total: sys.global_cpu_info().cpu_usage() }
}

/// Everything the GUI shows live, sampled once per second.
#[derive(Clone, Serialize)]
pub struct LiveSample {
    pub cpu_total: f32,
    pub cpu_cores: Vec<f32>,
    pub ram_used: u64,
    pub ram_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub proc_count: usize,
    pub net_rx: u64,
    pub net_tx: u64,
    pub disk_read: u64,
    pub disk_write: u64,
    pub uptime: u64,
    pub groups: Vec<Group>,
}

pub struct Sampler {
    sys: System,
    nets: Networks,
    anc: HashSet<u32>,
}

impl Default for Sampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler {
    pub fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu();
        sys.refresh_processes();
        sys.refresh_memory();
        let nets = Networks::new_with_refreshed_list();
        let anc = ancestors(&sys);
        Sampler { sys, nets, anc }
    }

    /// Call roughly once per second: rates are "since the previous call".
    pub fn sample(&mut self) -> LiveSample {
        self.sys.refresh_cpu();
        self.sys.refresh_processes();
        self.sys.refresh_memory();
        self.nets.refresh();
        let (mut rx, mut tx) = (0u64, 0u64);
        for (_, d) in &self.nets {
            rx += d.received();
            tx += d.transmitted();
        }
        let (mut dr, mut dw) = (0u64, 0u64);
        for p in self.sys.processes().values() {
            let du = p.disk_usage();
            dr += du.read_bytes;
            dw += du.written_bytes;
        }
        let mut groups = build_groups(&self.sys, &self.anc);
        let proc_count = self.sys.processes().len();
        groups.truncate(160);
        LiveSample {
            cpu_total: self.sys.global_cpu_info().cpu_usage(),
            cpu_cores: self.sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
            ram_used: self.sys.used_memory(),
            ram_total: self.sys.total_memory().max(1),
            swap_used: self.sys.used_swap(),
            swap_total: self.sys.total_swap(),
            proc_count,
            net_rx: rx,
            net_tx: tx,
            disk_read: dr,
            disk_write: dw,
            uptime: System::uptime(),
            groups,
        }
    }
}

/// Terminate the given PIDs. Protected processes are filtered out again here as a second line of defence.
pub fn kill_pids(pids: &[u32]) -> usize {
    let mut sys = System::new();
    sys.refresh_processes();
    let anc = ancestors(&sys);
    let mut killed = 0;
    for pid in pids {
        if anc.contains(pid) {
            continue;
        }
        if let Some(p) = sys.process(Pid::from_u32(*pid)) {
            if let Some((Class::Protected, _)) = classify_name(&p.name().to_lowercase()) {
                continue;
            }
            if p.kill() {
                killed += 1;
            }
        }
    }
    killed
}

pub fn kill_by_names(names: &[&str]) -> usize {
    let lc: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    let mut sys = System::new();
    sys.refresh_processes();
    let pids: Vec<u32> = sys
        .processes()
        .iter()
        .filter(|(_, p)| lc.contains(&p.name().to_lowercase()))
        .map(|(pid, _)| pid.as_u32())
        .collect();
    kill_pids(&pids)
}

/// Page out working sets of non-protected processes. Returns how many were trimmed.
pub fn trim_unprotected(groups: &[Group]) -> usize {
    let mut n = 0;
    for g in groups.iter().filter(|g| g.class != Class::Protected) {
        for pid in &g.pids {
            if winenv::trim_working_set(*pid) {
                n += 1;
            }
        }
    }
    n
}
