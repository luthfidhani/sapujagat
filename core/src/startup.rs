//! Startup entries: list and disable/enable them the same reversible way Task Manager does
//! (StartupApproved flag), so nothing is deleted.

use std::path::Path;

use winreg::enums::*;
use serde::Serialize;
use winreg::{RegKey, RegValue};

use crate::procs::{classify_name, Class};

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub name: String,
    pub command: String,
    pub scope: &'static str,
    pub enabled: bool,
    pub needs_admin: bool,
    pub advice: Option<&'static str>,
    #[serde(skip)]
    hive: Hive,
    #[serde(skip)]
    approved_key: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Hive {
    User,
    Machine,
}

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN32: &str = r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const AP_RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const AP_RUN32: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32";
const AP_FOLDER: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder";

fn root(h: Hive) -> RegKey {
    RegKey::predef(if h == Hive::User { HKEY_CURRENT_USER } else { HKEY_LOCAL_MACHINE })
}

fn approved_enabled(h: Hive, key: &str, name: &str) -> bool {
    let Ok(k) = root(h).open_subkey_with_flags(key, KEY_READ) else { return true };
    match k.get_raw_value(name) {
        Ok(v) => v.bytes.first().map(|b| b & 1 == 0).unwrap_or(true),
        Err(_) => true,
    }
}

fn exe_stem(cmd: &str) -> String {
    let c = cmd.trim();
    let exe = if let Some(rest) = c.strip_prefix('"') { rest.split('"').next().unwrap_or("") } else { c.split(" /").next().unwrap_or(c).split(' ').next().unwrap_or(c) };
    Path::new(exe).file_stem().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn advice_for(cmd: &str) -> Option<&'static str> {
    match classify_name(&exe_stem(cmd)) {
        Some((Class::Helper, why)) => Some(why),
        _ => None,
    }
}

pub fn list() -> Vec<Entry> {
    let mut v = Vec::new();
    let sources: [(Hive, &str, &'static str, &'static str, &'static str); 3] = [
        (Hive::User, RUN, AP_RUN, "User (registry)", ""),
        (Hive::Machine, RUN, AP_RUN, "Machine (registry)", ""),
        (Hive::Machine, RUN32, AP_RUN32, "Machine 32-bit (registry)", ""),
    ];
    for (h, run, ap, scope, _) in sources {
        if let Ok(k) = root(h).open_subkey_with_flags(run, KEY_READ) {
            for item in k.enum_values().flatten() {
                let (name, val) = item;
                let command = String::from_utf8_lossy(&val.bytes).replace('\0', "");
                let command = if val.vtype == REG_SZ || val.vtype == REG_EXPAND_SZ {
                    // REG_SZ is UTF-16LE; decode properly
                    let u: Vec<u16> = val.bytes.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                    String::from_utf16_lossy(&u).trim_end_matches('\0').to_string()
                } else {
                    command
                };
                v.push(Entry {
                    enabled: approved_enabled(h, ap, &name),
                    advice: advice_for(&command),
                    name,
                    command,
                    scope,
                    needs_admin: h == Hive::Machine,
                    hive: h,
                    approved_key: ap,
                });
            }
        }
    }
    for (h, dir, scope) in [
        (Hive::User, "%APPDATA%\\Microsoft\\Windows\\Start Menu\\Programs\\Startup", "User (Startup folder)"),
        (Hive::Machine, "%ProgramData%\\Microsoft\\Windows\\Start Menu\\Programs\\StartUp", "Machine (Startup folder)"),
    ] {
        let Some(d) = crate::winenv::expand(dir) else { continue };
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                if name.eq_ignore_ascii_case("desktop.ini") {
                    continue;
                }
                v.push(Entry {
                    enabled: approved_enabled(h, AP_FOLDER, &name),
                    advice: None,
                    command: e.path().to_string_lossy().to_string(),
                    name,
                    scope,
                    needs_admin: h == Hive::Machine,
                    hive: h,
                    approved_key: AP_FOLDER,
                });
            }
        }
    }
    v
}

pub fn set_enabled(e: &Entry, enabled: bool) -> Result<(), String> {
    let (k, _) = root(e.hive).create_subkey(e.approved_key).map_err(|x| x.to_string())?;
    let mut bytes = vec![0u8; 12];
    bytes[0] = if enabled { 2 } else { 3 };
    k.set_raw_value(&e.name, &RegValue { vtype: REG_BINARY, bytes }).map_err(|x| x.to_string())
}
