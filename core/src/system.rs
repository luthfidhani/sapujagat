//! Static-ish facts about this PC: OS, hardware, power. (Live numbers come from `procs::Sampler`.)

use serde::Serialize;
use sysinfo::System;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use winreg::enums::*;
use winreg::RegKey;

use crate::winenv;

#[derive(Serialize, Clone)]
pub struct Battery {
    pub present: bool,
    pub percent: Option<u8>,
    pub charging: Option<bool>,
    pub ac: Option<bool>,
    pub minutes_left: Option<u32>,
}

pub fn battery() -> Battery {
    let mut s: SYSTEM_POWER_STATUS = unsafe { std::mem::zeroed() };
    let ok = unsafe { GetSystemPowerStatus(&mut s) } != 0;
    if !ok || s.BatteryFlag == 128 {
        return Battery { present: false, percent: None, charging: None, ac: (ok && s.ACLineStatus != 255).then_some(s.ACLineStatus == 1), minutes_left: None };
    }
    Battery {
        present: true,
        percent: (s.BatteryLifePercent <= 100).then_some(s.BatteryLifePercent),
        charging: (s.BatteryFlag != 255).then_some(s.BatteryFlag & 8 != 0),
        ac: (s.ACLineStatus != 255).then_some(s.ACLineStatus == 1),
        minutes_left: (s.BatteryLifeTime != u32::MAX).then_some(s.BatteryLifeTime / 60),
    }
}

#[derive(Serialize)]
pub struct Gpu {
    pub name: String,
    pub vram: Option<u64>,
}

/// Fallback that works without Administrator: ask WMI for the video controllers.
fn gpus_wmi() -> Vec<Gpu> {
    let script = "Get-CimInstance Win32_VideoController | Select-Object Name,AdapterRAM | ConvertTo-Json -Compress";
    let Some(text) = winenv::run_text("powershell", &["-NoProfile", "-NonInteractive", "-Command", script]) else { return vec![] };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(text.trim()) else { return vec![] };
    let list: Vec<serde_json::Value> = match v {
        serde_json::Value::Array(a) => a,
        o @ serde_json::Value::Object(_) => vec![o],
        _ => vec![],
    };
    list.iter()
        .filter_map(|g| Some(Gpu { name: g["Name"].as_str()?.to_string(), vram: g["AdapterRAM"].as_u64().filter(|v| *v > 0) }))
        .collect()
}

pub fn gpus() -> Vec<Gpu> {
    let from_reg = gpus_registry();
    if from_reg.is_empty() { gpus_wmi() } else { from_reg }
}

fn gpus_registry() -> Vec<Gpu> {
    let base = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11cd-bfc1-08002be10318}";
    let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
    let mut out = Vec::new();
    let Ok(k) = hklm.open_subkey(base) else { return out };
    for name in k.enum_keys().flatten() {
        if name.len() != 4 || !name.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let Ok(sk) = k.open_subkey(&name) else { continue };
        let Ok(desc) = sk.get_value::<String, _>("DriverDesc") else { continue };
        let vram = sk
            .get_value::<u64, _>("HardwareInformation.qwMemorySize")
            .ok()
            .or_else(|| sk.get_raw_value("HardwareInformation.qwMemorySize").ok().filter(|v| v.bytes.len() >= 8).map(|v| u64::from_le_bytes(v.bytes[..8].try_into().unwrap())));
        out.push(Gpu { name: desc, vram: vram.filter(|v| *v > 0) });
    }
    out
}

pub fn power_plan() -> String {
    let t = winenv::run_text("powercfg", &["/getactivescheme"]).unwrap_or_default();
    match (t.rfind('('), t.rfind(')')) {
        (Some(a), Some(b)) if b > a => t[a + 1..b].trim().to_string(),
        _ => String::new(),
    }
}

#[derive(Serialize)]
pub struct Details {
    pub host: String,
    pub os_name: String,
    pub os_display_version: String,
    pub os_build: String,
    pub kernel: String,
    pub uptime_secs: u64,
    pub cpu_brand: String,
    pub physical_cores: Option<usize>,
    pub logical_cores: usize,
    pub cpu_mhz: u64,
    pub ram_total: u64,
    pub swap_total: u64,
    pub gpus: Vec<Gpu>,
    pub power_plan: String,
    pub battery: Battery,
    pub arch: &'static str,
    pub engine_version: &'static str,
    pub log_dir: String,
}

pub fn details() -> Details {
    let mut sys = System::new();
    sys.refresh_memory();
    sys.refresh_cpu();
    let cv = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion").ok();
    let reg_s = |n: &str| cv.as_ref().and_then(|k| k.get_value::<String, _>(n).ok()).unwrap_or_default();
    let build = reg_s("CurrentBuild");
    let ubr = cv.as_ref().and_then(|k| k.get_value::<u32, _>("UBR").ok());
    let mut name = reg_s("ProductName");
    if build.parse::<u32>().unwrap_or(0) >= 22000 {
        name = name.replace("Windows 10", "Windows 11");
    }
    Details {
        host: System::host_name().unwrap_or_default(),
        os_name: name,
        os_display_version: reg_s("DisplayVersion"),
        os_build: match ubr {
            Some(u) => format!("{build}.{u}"),
            None => build,
        },
        kernel: System::kernel_version().unwrap_or_default(),
        uptime_secs: System::uptime(),
        cpu_brand: sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default(),
        physical_cores: sys.physical_core_count(),
        logical_cores: sys.cpus().len(),
        cpu_mhz: sys.cpus().first().map(|c| c.frequency()).unwrap_or(0),
        ram_total: sys.total_memory(),
        swap_total: sys.total_swap(),
        gpus: gpus(),
        power_plan: power_plan(),
        battery: battery(),
        arch: std::env::consts::ARCH,
        engine_version: env!("CARGO_PKG_VERSION"),
        log_dir: crate::clean::log_dir().display().to_string(),
    }
}
