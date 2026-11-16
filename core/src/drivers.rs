//! Old driver packages in the DriverStore.
//!
//! Windows keeps every previous version of a driver. For each driver (same file name, provider and class) we keep
//! the newest package and offer the older ones. Packages that are built into Windows, boot-critical, or still in
//! use are never removed: `pnputil /delete-driver` is called WITHOUT /force, so Windows itself refuses when a
//! device still uses the package.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{fsx, winenv};

const STORE: &str = "c:\\windows\\system32\\driverstore\\filerepository\\";

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct Raw {
    driver: Option<String>,
    original_file_name: Option<String>,
    provider_name: Option<String>,
    class_name: Option<String>,
    inbox: Option<bool>,
    boot_critical: Option<bool>,
    version: Option<String>,
    date: Option<String>,
}

#[derive(Clone, Debug)]
pub struct Pkg {
    pub published: String,
    pub orig_path: String,
    pub provider: String,
    pub class: String,
    pub version: String,
    pub date: String,
    pub inbox: bool,
    pub boot: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct OldDriver {
    pub published: String,
    pub name: String,
    pub provider: String,
    pub class: String,
    pub version: String,
    pub date: String,
    pub kept: String,
    pub folder: String,
    pub bytes: u64,
}

fn ver_key(v: &str) -> Vec<u64> {
    v.split('.').map(|p| p.trim().parse().unwrap_or(0)).collect()
}

fn leaf(p: &str) -> String {
    p.rsplit('\\').next().unwrap_or(p).to_lowercase()
}

/// Pure selection logic: older versions inside each (file, provider, class) group, never inbox or boot-critical.
pub fn pick_old(pkgs: &[Pkg]) -> Vec<(Pkg, String)> {
    let mut groups: HashMap<(String, String, String), Vec<&Pkg>> = HashMap::new();
    for p in pkgs {
        groups.entry((leaf(&p.orig_path), p.provider.to_lowercase(), p.class.to_lowercase())).or_default().push(p);
    }
    let mut out = Vec::new();
    for (_, mut g) in groups {
        if g.len() < 2 {
            continue;
        }
        g.sort_by(|a, b| (b.date.as_str(), ver_key(&b.version)).cmp(&(a.date.as_str(), ver_key(&a.version))));
        let kept = g[0].version.clone();
        for p in g.iter().skip(1) {
            if !p.inbox && !p.boot {
                out.push(((*p).clone(), kept.clone()));
            }
        }
    }
    out
}

fn valid_published(s: &str) -> bool {
    let l = s.to_lowercase();
    l.starts_with("oem") && l.ends_with(".inf") && l[3..l.len() - 4].chars().all(|c| c.is_ascii_digit()) && l.len() > 7
}

/// Needs Administrator (DISM). Returns the removable older packages with their on-disk size.
pub fn find() -> Result<Vec<OldDriver>, String> {
    let script = "$ErrorActionPreference='Stop'; Get-WindowsDriver -Online -All | Select-Object Driver,OriginalFileName,ProviderName,ClassName,Inbox,BootCritical,@{n='Version';e={$_.Version.ToString()}},@{n='Date';e={$_.Date.ToString('yyyy-MM-dd')}} | ConvertTo-Json -Compress";
    let out = winenv::run("powershell", &["-NoProfile", "-NonInteractive", "-Command", script]).map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("Windows would not list drivers (Administrator is required)".into());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let value: serde_json::Value = serde_json::from_str(text.trim()).map_err(|e| format!("unreadable driver list: {e}"))?;
    let items = match value {
        serde_json::Value::Array(a) => a,
        o @ serde_json::Value::Object(_) => vec![o],
        _ => vec![],
    };
    let mut pkgs = Vec::new();
    for it in items {
        let Ok(r) = serde_json::from_value::<Raw>(it) else { continue };
        let (Some(published), Some(orig)) = (r.driver, r.original_file_name) else { continue };
        if !valid_published(&published) || !orig.to_lowercase().starts_with(STORE) {
            continue;
        }
        pkgs.push(Pkg {
            published,
            orig_path: orig,
            provider: r.provider_name.unwrap_or_default(),
            class: r.class_name.unwrap_or_default(),
            version: r.version.unwrap_or_default(),
            date: r.date.unwrap_or_default(),
            inbox: r.inbox.unwrap_or(false),
            boot: r.boot_critical.unwrap_or(false),
        });
    }
    let mut out = Vec::new();
    for (p, kept) in pick_old(&pkgs) {
        let folder = Path::new(&p.orig_path).parent().map(|x| x.to_path_buf()).unwrap_or_default();
        let fl = folder.to_string_lossy().to_lowercase();
        if !fl.starts_with(STORE) || fl.trim_end_matches('\\').len() <= STORE.len() - 1 || fsx::is_excluded(&folder) {
            continue;
        }
        out.push(OldDriver {
            published: p.published.clone(),
            name: leaf(&p.orig_path),
            provider: p.provider.clone(),
            class: p.class.clone(),
            version: p.version.clone(),
            date: p.date.clone(),
            kept,
            folder: folder.display().to_string(),
            bytes: fsx::measure_tree(&folder).bytes,
        });
    }
    out.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    Ok(out)
}

/// Ask Windows to delete one package. Windows refuses if a device still uses it.
pub fn remove(d: &OldDriver) -> bool {
    valid_published(&d.published) && winenv::run_ok("pnputil", &["/delete-driver", &d.published])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(pub_: &str, file: &str, ver: &str, date: &str, inbox: bool, boot: bool) -> Pkg {
        Pkg { published: pub_.into(), orig_path: format!("C:\\Windows\\System32\\DriverStore\\FileRepository\\{file}_x\\{file}"), provider: "Acme".into(), class: "Display".into(), version: ver.into(), date: date.into(), inbox, boot }
    }

    #[test]
    fn keeps_the_newest_and_offers_older() {
        let v = vec![
            pkg("oem1.inf", "gpu.inf", "30.0.1.1", "2024-01-01", false, false),
            pkg("oem2.inf", "gpu.inf", "31.0.2.0", "2025-03-01", false, false),
            pkg("oem3.inf", "gpu.inf", "30.9.9.9", "2024-06-01", false, false),
            pkg("oem4.inf", "other.inf", "1.0.0.0", "2020-01-01", false, false),
        ];
        let mut old: Vec<String> = pick_old(&v).into_iter().map(|(p, _)| p.published).collect();
        old.sort();
        assert_eq!(old, vec!["oem1.inf", "oem3.inf"]);
        assert!(pick_old(&v).iter().all(|(_, kept)| kept == "31.0.2.0"));
    }

    #[test]
    fn never_offers_inbox_or_boot_critical_or_singletons() {
        let v = vec![
            pkg("oem1.inf", "net.inf", "1.0.0.0", "2020-01-01", true, false),
            pkg("oem2.inf", "net.inf", "2.0.0.0", "2022-01-01", false, false),
            pkg("oem3.inf", "disk.inf", "1.0.0.0", "2020-01-01", false, true),
            pkg("oem4.inf", "disk.inf", "2.0.0.0", "2022-01-01", false, false),
        ];
        assert!(pick_old(&v).is_empty());
    }

    #[test]
    fn version_compare_is_numeric_not_textual() {
        let v = vec![pkg("oem1.inf", "a.inf", "9.0.0.0", "2024-01-01", false, false), pkg("oem2.inf", "a.inf", "10.0.0.0", "2024-01-01", false, false)];
        let old = pick_old(&v);
        assert_eq!(old.len(), 1);
        assert_eq!(old[0].0.published, "oem1.inf");
    }

    #[test]
    fn published_name_is_validated() {
        assert!(valid_published("oem42.inf"));
        assert!(!valid_published("oem.inf"));
        assert!(!valid_published("oem1.inf & calc"));
        assert!(!valid_published("..\\oem1.inf"));
    }
}
