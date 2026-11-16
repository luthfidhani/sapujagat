#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! Tauri backend. Security model:
//!  * the web UI can only send rule IDs, PIDs, service/startup names and read-only folder paths
//!  * it can never send a path to delete: deletion paths always come from the engine's own catalog or
//!    from the last dev-junk scan stored here
//!  * the engine re-validates everything (path guard, protected processes)

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use sapu_core::analyze::{self, Annotator, Tag};
use sapu_core::clean::{self, Options, Report};
use sapu_core::devjunk::{self, Junk};
use sapu_core::fsx;
use sapu_core::leftovers::{self, Leftover};
use sapu_core::settings::{self, Settings};
use sapu_core::procs::{self, LiveSample, Sampler, Snapshot, SysInfo};
use sapu_core::system::{self, Battery, Details};
use sapu_core::rules::{self, Cat, Kind, Risk, Rule};
use sapu_core::scan;
use sapu_core::startup::{self, Entry};
use sapu_core::winenv::{self, DriveInfo};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, State};

struct DevState(Mutex<Vec<Junk>>);

/// Executable paths of the processes currently running. Icons are only ever extracted for these.
#[derive(Clone)]
struct LiveExes(Arc<Mutex<HashSet<String>>>);

#[derive(Clone)]
struct IconCache(Arc<Mutex<HashMap<String, Option<String>>>>);

#[tauri::command]
async fn process_icons(known: State<'_, LiveExes>, cache: State<'_, IconCache>, paths: Vec<String>) -> Result<HashMap<String, String>, String> {
    let allowed = known.0.lock().unwrap().clone();
    let wanted: Vec<String> = paths.into_iter().filter(|p| allowed.contains(p)).take(64).collect();
    let cache = cache.inner().clone();
    blocking(move || {
        let mut out = HashMap::new();
        for p in wanted {
            let hit = cache.0.lock().unwrap().get(&p).cloned();
            let icon = match hit {
                Some(v) => v,
                None => {
                    let v = sapu_core::icons::exe_icon_data_uri(&p);
                    cache.0.lock().unwrap().insert(p.clone(), v.clone());
                    v
                }
            };
            if let Some(u) = icon {
                out.insert(p, u);
            }
        }
        out
    })
    .await
}

fn blocking<T, F>(f: F) -> impl std::future::Future<Output = Result<T, String>>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    let h = tauri::async_runtime::spawn_blocking(f);
    async move { h.await.map_err(|e| e.to_string()) }
}

fn cat_key(c: Cat) -> &'static str {
    match c {
        Cat::System => "system",
        Cat::Browser => "browser",
        Cat::Gpu => "gpu",
        Cat::Dev => "dev",
        Cat::Docker => "docker",
        Cat::Apps => "apps",
        Cat::Privacy => "privacy",
    }
}

fn risk_key(r: Risk) -> &'static str {
    match r {
        Risk::Safe => "safe",
        Risk::Careful => "careful",
        Risk::Advanced => "advanced",
    }
}

#[derive(Serialize)]
struct Boot {
    admin: bool,
    sys: SysInfo,
    drives: Vec<DriveInfo>,
    version: &'static str,
}

#[tauri::command]
fn boot() -> Boot {
    Boot { admin: winenv::is_admin(), sys: procs::sys_info(), drives: winenv::fixed_drives(), version: env!("CARGO_PKG_VERSION") }
}

#[tauri::command]
fn drives() -> Vec<DriveInfo> {
    winenv::fixed_drives()
}

#[derive(Serialize)]
struct RuleRow {
    id: &'static str,
    name: &'static str,
    cat: &'static str,
    cat_title: &'static str,
    risk: &'static str,
    admin: bool,
    default_on: bool,
    what: &'static str,
    effect: &'static str,
    close: Vec<&'static str>,
    special: bool,
    locations: Vec<String>,
    items: Vec<String>,
    bytes: Option<u64>,
    files: u64,
    available: bool,
    note: Option<String>,
    running: Vec<String>,
}

fn row(r: &Rule, s: Option<&scan::RuleScan>) -> RuleRow {
    RuleRow {
        id: r.id,
        name: r.name,
        cat: cat_key(r.cat),
        cat_title: r.cat.title(),
        risk: risk_key(r.risk),
        admin: r.admin,
        default_on: r.default_on,
        what: r.what,
        effect: r.effect,
        close: r.close.clone(),
        special: matches!(r.kind, Kind::Special(_)),
        locations: match &r.kind {
            Kind::Paths(ts) => ts
                .iter()
                .map(|t| {
                    let mut s = t.path.clone();
                    if let Some(p) = &t.pattern {
                        s.push_str(&format!("   matching {p}"));
                    }
                    if t.age_days > 0 {
                        s.push_str(&format!("   older than {} days", t.age_days));
                    }
                    s
                })
                .collect(),
            Kind::Special(_) => vec!["Built-in system action (no folder is deleted by path)".to_string()],
        },
        items: s.map(|s| s.items.clone()).unwrap_or_default(),
        bytes: s.and_then(|s| s.bytes),
        files: s.map(|s| s.files).unwrap_or(0),
        available: s.map(|s| s.available).unwrap_or(true),
        note: s.and_then(|s| s.note.clone()),
        running: s.map(|s| s.running.clone()).unwrap_or_default(),
    }
}

#[tauri::command]
fn catalog() -> Vec<RuleRow> {
    rules::catalog_all().iter().map(|r| row(r, None)).collect()
}

#[derive(Serialize)]
struct RuleSetting {
    id: &'static str,
    name: &'static str,
    cat: &'static str,
    cat_title: &'static str,
    risk: &'static str,
    admin: bool,
    enabled: bool,
    recommended: bool,
    base_recommended: bool,
}

#[derive(Serialize)]
struct SettingsView {
    settings: Settings,
    rules: Vec<RuleSetting>,
}

fn settings_view() -> SettingsView {
    let s = settings::current();
    let rules = rules::catalog_all()
        .iter()
        .map(|r| RuleSetting {
            id: r.id,
            name: r.name,
            cat: cat_key(r.cat),
            cat_title: r.cat.title(),
            risk: risk_key(r.risk),
            admin: r.admin,
            enabled: !s.disabled_rules.iter().any(|d| d == r.id),
            recommended: s.recommended.get(r.id).copied().unwrap_or(r.default_on),
            base_recommended: r.default_on,
        })
        .collect();
    SettingsView { settings: s, rules }
}

#[tauri::command]
fn settings_get() -> SettingsView {
    settings_view()
}

#[tauri::command]
fn settings_save(new: Settings) -> Result<SettingsView, String> {
    let ids: Vec<&str> = rules::catalog_all().iter().map(|r| r.id).collect();
    let clean = settings::validate(new, &ids)?;
    settings::save(&clean).map_err(|e| format!("could not save settings: {e}"))?;
    settings::apply(clean);
    Ok(settings_view())
}

#[tauri::command]
async fn scan_rules() -> Result<Vec<RuleRow>, String> {
    blocking(|| {
        let all = rules::catalog();
        let scans = scan::scan_all(&all);
        all.iter().zip(scans.iter()).map(|(r, s)| row(r, Some(s))).collect()
    })
    .await
}

#[tauri::command]
async fn run_clean(app: AppHandle, ids: Vec<String>, close_apps: bool) -> Result<Report, String> {
    blocking(move || {
        let all = rules::catalog();
        let wanted: HashSet<&str> = ids.iter().map(String::as_str).collect();
        let chosen: Vec<&Rule> = all.iter().filter(|r| wanted.contains(r.id)).collect();
        let admin = winenv::is_admin();
        clean::execute(&chosen, &Options { close_apps, restore_point: settings::current().restore_point }, admin, &mut |ev| {
            let _ = app.emit("clean-event", ev);
        })
    })
    .await
}

#[tauri::command]
async fn procs_snapshot() -> Result<Snapshot, String> {
    blocking(procs::snapshot).await
}

#[derive(Serialize)]
struct KillResult {
    killed: usize,
    requested: usize,
}

#[tauri::command]
async fn kill_pids(pids: Vec<u32>) -> Result<KillResult, String> {
    blocking(move || KillResult { killed: procs::kill_pids(&pids), requested: pids.len() }).await
}

#[derive(Serialize)]
struct TrimResult {
    trimmed: usize,
    used_before: u64,
    used_after: u64,
}

#[tauri::command]
async fn trim_memory() -> Result<TrimResult, String> {
    blocking(|| {
        let snap = procs::snapshot();
        let used_before = snap.used_mem;
        let trimmed = procs::trim_unprotected(&snap.groups);
        std::thread::sleep(std::time::Duration::from_millis(700));
        TrimResult { trimmed, used_before, used_after: procs::sys_info().ram_used }
    })
    .await
}

#[derive(Serialize)]
struct SvcView {
    name: &'static str,
    label: &'static str,
    why: &'static str,
    running: bool,
}

#[tauri::command]
async fn services_list() -> Result<Vec<SvcView>, String> {
    blocking(|| {
        procs::BLOAT_SERVICES
            .iter()
            .map(|s| SvcView { name: s.name, label: s.label, why: s.why, running: procs::service_running(s.name) })
            .collect()
    })
    .await
}

#[tauri::command]
async fn service_manual(name: String) -> Result<String, String> {
    blocking(move || {
        if !procs::BLOAT_SERVICES.iter().any(|s| s.name == name) {
            return Err("unknown service".to_string());
        }
        if !winenv::is_admin() {
            return Err("needs Administrator".to_string());
        }
        let before = procs::service_start_type(&name);
        if procs::stop_and_set_manual(&name) {
            Ok(format!("sc config \"{name}\" start= {before}"))
        } else {
            Err("could not change the start type".to_string())
        }
    })
    .await?
}

#[tauri::command]
fn startup_list() -> Vec<Entry> {
    startup::list()
}

#[tauri::command]
fn startup_set(name: String, scope: String, enabled: bool) -> Result<(), String> {
    let e = startup::list().into_iter().find(|e| e.name == name && e.scope == scope).ok_or("entry not found")?;
    if e.needs_admin && !winenv::is_admin() {
        return Err("needs Administrator".into());
    }
    startup::set_enabled(&e, enabled)
}

#[derive(Serialize, Deserialize, Clone)]
struct ItemView {
    name: String,
    path: String,
    bytes: u64,
    is_dir: bool,
    tag: Option<Tag>,
}

#[derive(Serialize, Deserialize, Clone)]
struct BigView {
    path: String,
    bytes: u64,
    tag: Option<Tag>,
}

#[derive(Serialize, Deserialize, Clone)]
struct AnalysisView {
    root: String,
    parent: Option<String>,
    items: Vec<ItemView>,
    big_files: Vec<BigView>,
    total: u64,
    unreadable: u64,
    /// unix seconds when this analysis finished
    analyzed_at: u64,
    took_secs: f32,
}

/// Analysis results survive page changes and app restarts until a new analysis replaces them.
#[derive(Clone)]
struct AnCache(Arc<Mutex<HashMap<String, AnalysisView>>>);

fn cache_file() -> PathBuf {
    PathBuf::from(std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".into())).join("sapujagat").join("cache").join("storage.json")
}

fn load_cache() -> HashMap<String, AnalysisView> {
    std::fs::read_to_string(cache_file()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

fn an_key(p: &str) -> String {
    fsx::canon_key(p)
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn run_analysis(path: &str) -> Result<AnalysisView, String> {
    let p = PathBuf::from(path);
    if !p.is_dir() {
        return Err("not a folder".to_string());
    }
    let t0 = std::time::Instant::now();
    let a = analyze::analyze_dir(&p, 100 * 1024 * 1024);
    let ann = Annotator::new();
    Ok(AnalysisView {
        root: a.root.display().to_string(),
        parent: p.parent().filter(|x| x.as_os_str().len() > 0).map(|x| x.display().to_string()),
        items: a
            .items
            .iter()
            .take(60)
            .map(|i| ItemView { name: i.name.clone(), path: i.path.display().to_string(), bytes: i.bytes, is_dir: i.is_dir, tag: ann.tag(&i.path) })
            .collect(),
        big_files: a.big_files.iter().map(|f| BigView { path: f.path.display().to_string(), bytes: f.bytes, tag: ann.tag(&f.path) }).collect(),
        total: a.total,
        unreadable: a.unreadable,
        analyzed_at: now_secs(),
        took_secs: t0.elapsed().as_secs_f32(),
    })
}

fn store_analysis(cache: &AnCache, v: &AnalysisView) {
    let mut m = cache.0.lock().unwrap();
    m.insert(an_key(&v.root), v.clone());
    while m.len() > 150 {
        if let Some(oldest) = m.iter().min_by_key(|(_, x)| x.analyzed_at).map(|(k, _)| k.clone()) {
            m.remove(&oldest);
        } else {
            break;
        }
    }
    if let Some(dir) = cache_file().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string(&*m) {
        let _ = std::fs::write(cache_file(), json);
    }
}

/// Cached result only; never starts a scan.
#[tauri::command]
fn analysis_get(state: State<'_, AnCache>, path: String) -> Option<AnalysisView> {
    state.0.lock().unwrap().get(&an_key(&path)).cloned()
}

#[derive(Serialize)]
struct AnStatus {
    root: String,
    analyzed_at: u64,
    took_secs: f32,
}

/// Which drives already have a stored analysis.
#[tauri::command]
fn analysis_status(state: State<'_, AnCache>) -> Vec<AnStatus> {
    let m = state.0.lock().unwrap();
    winenv::fixed_drives()
        .into_iter()
        .filter_map(|d| m.get(&an_key(&d.root)).map(|v| AnStatus { root: d.root.clone(), analyzed_at: v.analyzed_at, took_secs: v.took_secs }))
        .collect()
}

/// One folder. Uses the stored result unless `force` is set.
#[tauri::command]
async fn analyze_path(state: State<'_, AnCache>, path: String, force: bool) -> Result<AnalysisView, String> {
    let cache = state.inner().clone();
    if !force {
        if let Some(v) = cache.0.lock().unwrap().get(&an_key(&path)).cloned() {
            return Ok(v);
        }
    }
    let v = blocking(move || run_analysis(&path)).await??;
    store_analysis(&cache, &v);
    Ok(v)
}

#[derive(Serialize, Clone)]
struct AnEvent {
    root: String,
    status: &'static str,
    view: Option<AnalysisView>,
    error: Option<String>,
}

/// Every drive at once, in parallel. Progress arrives as `analysis-event`; finished drives are stored immediately.
#[tauri::command]
async fn analyze_all(app: AppHandle, state: State<'_, AnCache>, force: bool) -> Result<(), String> {
    let cache = state.inner().clone();
    let roots: Vec<String> = winenv::fixed_drives().into_iter().map(|d| d.root).collect();
    let mut handles = Vec::new();
    for root in roots {
        if !force {
            if let Some(v) = cache.0.lock().unwrap().get(&an_key(&root)).cloned() {
                let _ = app.emit("analysis-event", AnEvent { root, status: "done", view: Some(v), error: None });
                continue;
            }
        }
        let _ = app.emit("analysis-event", AnEvent { root: root.clone(), status: "start", view: None, error: None });
        let (app2, cache2) = (app.clone(), cache.clone());
        handles.push(tauri::async_runtime::spawn_blocking(move || match run_analysis(&root) {
            Ok(v) => {
                store_analysis(&cache2, &v);
                let _ = app2.emit("analysis-event", AnEvent { root, status: "done", view: Some(v), error: None });
            }
            Err(e) => {
                let _ = app2.emit("analysis-event", AnEvent { root, status: "error", view: None, error: Some(e) });
            }
        }));
    }
    for h in handles {
        let _ = h.await;
    }
    Ok(())
}

#[tauri::command]
async fn dev_find(state: State<'_, DevState>, roots: Vec<String>) -> Result<Vec<Junk>, String> {
    let roots: Vec<PathBuf> = roots.into_iter().map(PathBuf::from).filter(|p| p.is_dir()).collect();
    let found = blocking(move || devjunk::find(&roots)).await?;
    *state.0.lock().unwrap() = found.clone();
    Ok(found)
}

#[derive(Serialize)]
struct DevResult {
    freed: u64,
    locked: u64,
    deleted: usize,
}

#[tauri::command]
async fn dev_delete(state: State<'_, DevState>, paths: Vec<String>) -> Result<DevResult, String> {
    // only paths that the last scan produced are eligible
    let wanted: HashSet<String> = paths.into_iter().collect();
    let todo: Vec<Junk> = state.0.lock().unwrap().iter().filter(|j| wanted.contains(&j.path.display().to_string())).cloned().collect();
    let done: (DevResult, Vec<PathBuf>) = blocking(move || {
        let mut res = DevResult { freed: 0, locked: 0, deleted: 0 };
        let mut gone = Vec::new();
        for j in &todo {
            let d = devjunk::delete(j);
            res.freed += d.bytes;
            res.locked += d.locked;
            if !Path::new(&j.path).exists() {
                res.deleted += 1;
                gone.push(j.path.clone());
            }
        }
        (res, gone)
    })
    .await?;
    state.0.lock().unwrap().retain(|j| !done.1.contains(&j.path));
    Ok(done.0)
}

#[tauri::command]
async fn system_details() -> Result<Details, String> {
    blocking(system::details).await
}

#[tauri::command]
async fn history() -> Result<Vec<clean::HistoryEntry>, String> {
    blocking(clean::history).await
}

#[derive(Serialize, Clone)]
struct LivePayload {
    #[serde(flatten)]
    sample: LiveSample,
    battery: Battery,
    /// refreshed every 5 seconds (disk free space changes slowly)
    drives: Option<Vec<DriveInfo>>,
}

/// Samples the machine once per second and pushes it to the UI.
fn start_live(app: AppHandle, exes: LiveExes) {
    std::thread::spawn(move || {
        let mut sampler = Sampler::new();
        std::thread::sleep(std::time::Duration::from_millis(1000));
        let mut tick: u64 = 0;
        loop {
            let started = std::time::Instant::now();
            let sample = sampler.sample();
            *exes.0.lock().unwrap() = sample.groups.iter().filter(|g| !g.exe.is_empty()).map(|g| g.exe.clone()).collect();
            let payload = LivePayload { sample, battery: system::battery(), drives: (tick % 5 == 0).then(winenv::fixed_drives) };
            let _ = app.emit("live", payload);
            tick += 1;
            let spent = started.elapsed();
            std::thread::sleep(std::time::Duration::from_millis(1000).saturating_sub(spent));
        }
    });
}

/// Result of the last "possible leftovers" scan. Only paths in this list (or in a stored analysis) can be recycled.
struct LeftState(Mutex<Vec<Leftover>>);

#[tauri::command]
async fn leftovers_find(state: State<'_, LeftState>) -> Result<Vec<Leftover>, String> {
    let found = blocking(leftovers::find).await?;
    *state.0.lock().unwrap() = found.clone();
    Ok(found)
}

fn same_path(a: &str, b: &str) -> bool {
    fsx::canon_key(a) == fsx::canon_key(b)
}

#[tauri::command]
fn path_reveal(path: String) -> Result<(), String> {
    if !Path::new(&path).exists() {
        return Err("that item no longer exists".into());
    }
    if winenv::reveal(&path) { Ok(()) } else { Err("could not open Explorer".into()) }
}

/// Moves one item to the Recycle Bin. The UI cannot name arbitrary paths: the item must come from a stored
/// analysis or the leftovers scan, and the safety guard must accept it (system folders, drive roots, credentials,
/// Claude data and your Settings exclusions are refused).
#[tauri::command]
async fn path_recycle(an: State<'_, AnCache>, left: State<'_, LeftState>, path: String) -> Result<(), String> {
    let known = an.0.lock().unwrap().values().any(|v| v.items.iter().any(|i| same_path(&i.path, &path)) || v.big_files.iter().any(|f| same_path(&f.path, &path)))
        || left.0.lock().unwrap().iter().any(|l| same_path(&l.path, &path));
    if !known {
        return Err("that item is not part of the current results".into());
    }
    if let Err(why) = fsx::guard_opts(Path::new(&path), true) {
        return Err(format!("refused: {why}"));
    }
    let p2 = path.clone();
    blocking(move || winenv::recycle(&p2)).await??;
    // keep the stored analyses honest: drop the item and its size
    let mut m = an.0.lock().unwrap();
    for v in m.values_mut() {
        if let Some(pos) = v.items.iter().position(|i| same_path(&i.path, &path)) {
            let gone = v.items.remove(pos);
            v.total = v.total.saturating_sub(gone.bytes);
        }
        v.big_files.retain(|f| !same_path(&f.path, &path));
    }
    if let Ok(json) = serde_json::to_string(&*m) {
        let _ = std::fs::write(cache_file(), json);
    }
    drop(m);
    left.0.lock().unwrap().retain(|l| !same_path(&l.path, &path));
    Ok(())
}

#[tauri::command]
fn relaunch_admin(app: AppHandle) -> bool {
    if winenv::relaunch_as_admin(&[]) {
        app.exit(0);
        true
    } else {
        false
    }
}

fn main() {
    settings::init();
    tauri::Builder::default()
        .manage(DevState(Mutex::new(Vec::new())))
        .manage(LeftState(Mutex::new(Vec::new())))
        .manage(AnCache(Arc::new(Mutex::new(load_cache()))))
        .manage(LiveExes(Arc::new(Mutex::new(HashSet::new()))))
        .manage(IconCache(Arc::new(Mutex::new(HashMap::new()))))
        .setup(|app| {
            let exes = app.state::<LiveExes>().inner().clone();
            start_live(app.handle().clone(), exes);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            system_details,
            history,
            boot,
            drives,
            catalog,
            scan_rules,
            run_clean,
            procs_snapshot,
            kill_pids,
            trim_memory,
            services_list,
            service_manual,
            startup_list,
            startup_set,
            process_icons,
            settings_get,
            settings_save,
            leftovers_find,
            path_reveal,
            path_recycle,
            analyze_path,
            analyze_all,
            analysis_get,
            analysis_status,
            dev_find,
            dev_delete,
            relaunch_admin
        ])
        .run(tauri::generate_context!())
        .expect("error while running Sapu Jagat");
}
