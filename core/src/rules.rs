//! The catalogue of everything Sapu Jagat knows how to clean.
//!
//! Design: an *allow-list*. If a location is not described here, the cleaner never touches it.
//! Every rule says what it is, what happens when you clean it, and how risky it is.

use serde::Serialize;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, PartialOrd, Ord)]
pub enum Risk {
    /// Regenerates by itself, no visible side effect.
    Safe,
    /// Safe for the system, but has a visible side effect (re-download, re-login, slower first start...).
    Careful,
    /// Changes system behaviour or removes something that is not a cache. Always opt-in.
    Advanced,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, PartialOrd, Ord)]
pub enum Cat {
    System,
    Browser,
    Gpu,
    Dev,
    Docker,
    Apps,
    Privacy,
}

impl Cat {
    pub fn title(self) -> &'static str {
        match self {
            Cat::System => "Windows & System",
            Cat::Browser => "Browsers",
            Cat::Gpu => "GPU shader caches",
            Cat::Dev => "Developer caches",
            Cat::Docker => "Docker",
            Cat::Apps => "App caches",
            Cat::Privacy => "Privacy traces",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Cat::System => "🪟",
            Cat::Browser => "🌐",
            Cat::Gpu => "🎮",
            Cat::Dev => "🛠 ",
            Cat::Docker => "🐳",
            Cat::Apps => "📦",
            Cat::Privacy => "🕵 ",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
pub enum Special {
    RecycleBin,
    DnsCache,
    DockerBuildCache,
    DockerStopped,
    DockerUnusedImages,
    DockerVolumes,
    ComponentStore,
    EventLogs,
    Hibernation,
}

#[derive(Clone, Debug, Serialize)]
pub struct Target {
    pub path: String,
    pub pattern: Option<String>,
    pub age_days: u32,
}

#[derive(Clone, Debug, Serialize)]
pub enum Kind {
    Paths(Vec<Target>),
    Special(Special),
}

#[derive(Clone, Debug, Serialize)]
pub struct Rule {
    pub id: &'static str,
    pub name: &'static str,
    pub cat: Cat,
    pub risk: Risk,
    pub admin: bool,
    pub default_on: bool,
    pub what: &'static str,
    pub effect: &'static str,
    /// Executables that keep these files locked while running.
    pub close: Vec<&'static str>,
    /// Windows services that must be stopped while cleaning (restarted afterwards).
    pub services: Vec<&'static str>,
    pub kind: Kind,
}

fn t(path: &str) -> Target {
    Target { path: path.into(), pattern: None, age_days: 0 }
}
fn ta(path: &str, age: u32) -> Target {
    Target { path: path.into(), pattern: None, age_days: age }
}
fn tp(path: &str, pat: &str, age: u32) -> Target {
    Target { path: path.into(), pattern: Some(pat.into()), age_days: age }
}

struct B(Rule);
impl B {
    fn new(id: &'static str, name: &'static str, cat: Cat, what: &'static str, effect: &'static str, kind: Kind) -> B {
        B(Rule {
            id,
            name,
            cat,
            risk: Risk::Safe,
            admin: false,
            default_on: true,
            what,
            effect,
            close: vec![],
            services: vec![],
            kind,
        })
    }
    fn risk(mut self, r: Risk) -> B {
        self.0.risk = r;
        if r != Risk::Safe {
            self.0.default_on = false;
        }
        self
    }
    fn admin(mut self) -> B {
        self.0.admin = true;
        self
    }
    fn on(mut self) -> B {
        self.0.default_on = true;
        self
    }
    fn close(mut self, v: &[&'static str]) -> B {
        self.0.close = v.to_vec();
        self
    }
    fn svc(mut self, v: &[&'static str]) -> B {
        self.0.services = v.to_vec();
        self
    }
    fn done(self) -> Rule {
        self.0
    }
}

fn paths(v: Vec<Target>) -> Kind {
    Kind::Paths(v)
}

/// Chromium-family browser cache layout under a "User Data" directory.
fn chromium(user_data: &str) -> Vec<Target> {
    let mut v = Vec::new();
    for sub in [
        "*\\Cache",
        "*\\Code Cache",
        "*\\GPUCache",
        "*\\Media Cache",
        "*\\DawnGraphiteCache",
        "*\\DawnWebGPUCache",
        "*\\Service Worker\\CacheStorage",
        "*\\Service Worker\\ScriptCache",
        "GrShaderCache",
        "ShaderCache",
        "GraphiteDawnCache",
    ] {
        v.push(t(&format!("{user_data}\\{sub}")));
    }
    v
}

pub fn catalog() -> Vec<Rule> {
    use Cat::*;
    use Risk::*;
    let mut r: Vec<Rule> = Vec::new();

    // ───────────────────────── Windows & System (user level) ─────────────────────────
    r.push(B::new("temp-user", "User temp files", System,
        "Leftovers apps wrote to %TEMP% (installers, extracted archives, scratch files).",
        "Files older than 2 days only; anything currently in use is skipped.",
        paths(vec![ta("%TEMP%", 2)])).done());
    r.push(B::new("crash-dumps", "Crash dumps (user)", System,
        "Memory dumps apps left after crashing.", "Only useful for debugging; safe to delete.",
        paths(vec![t("%LOCALAPPDATA%\\CrashDumps")])).done());
    r.push(B::new("wer-user", "Error reports (user)", System,
        "Windows Error Reporting queue and archive for your account.", "None.",
        paths(vec![t("%LOCALAPPDATA%\\Microsoft\\Windows\\WER")])).done());
    r.push(B::new("inet-cache", "Internet cache (WinINet)", System,
        "Cache used by Windows/Office components, not by your browser.", "Pages/assets re-download when needed.",
        paths(vec![t("%LOCALAPPDATA%\\Microsoft\\Windows\\INetCache")])).done());
    r.push(B::new("thumb-cache", "Thumbnail & icon cache", System,
        "Explorer's cached previews of images/videos and icons.",
        "Explorer rebuilds it gradually; folders may load previews a little slower once. Locked files are skipped.",
        paths(vec![tp("%LOCALAPPDATA%\\Microsoft\\Windows\\Explorer", "thumbcache_*.db|iconcache_*.db", 0)])).done());
    r.push(B::new("dns-cache", "DNS resolver cache", System,
        "Windows' cache of domain-name lookups.", "Next visit to each site does one fresh DNS lookup.",
        Kind::Special(Special::DnsCache)).admin().done());
    r.push(B::new("recycle-bin", "Recycle Bin (all drives)", System,
        "Files you already deleted but that still occupy disk space.",
        "PERMANENT: items currently in the Recycle Bin can no longer be restored.",
        Kind::Special(Special::RecycleBin)).risk(Careful).on().done());

    // ───────────────────────── Windows & System (admin) ─────────────────────────
    r.push(B::new("temp-windows", "Windows temp folder", System,
        "System-wide scratch space (C:\\Windows\\Temp).", "Files older than 2 days only; in-use files are skipped.",
        paths(vec![ta("C:\\Windows\\Temp", 2)])).admin().done());
    r.push(B::new("temp-other-users", "Temp files of other Windows accounts", System,
        "%TEMP% of every other user account on this PC (installers and scratch files they left).",
        "Files older than 2 days only; in-use files are skipped. Your own account is covered by the user rule.",
        paths(vec![ta("C:\\Users\\*\\AppData\\Local\\Temp", 2)])).admin().done());
    r.push(B::new("win-logs", "Windows servicing logs", System,
        "CBS / DISM / Windows Update logs and old log cabinets.", "Only matters if you are debugging a failed update. Files older than 7 days.",
        paths(vec![
            tp("C:\\Windows\\Logs\\CBS", "*.log|*.cab", 7),
            tp("C:\\Windows\\Logs\\DISM", "*.log", 7),
            tp("C:\\Windows\\Logs\\WindowsUpdate", "*.etl", 3),
            ta("C:\\ProgramData\\USOShared\\Logs", 7),
        ])).admin().done());
    r.push(B::new("wu-cache", "Windows Update download cache", System,
        "Update packages that were already installed (SoftwareDistribution\\Download).",
        "Windows Update stops briefly during cleaning; anything still needed is re-downloaded.",
        paths(vec![t("C:\\Windows\\SoftwareDistribution\\Download")])).admin().svc(&["wuauserv", "bits"]).done());
    r.push(B::new("delivery-opt", "Delivery Optimization cache", System,
        "Update chunks Windows keeps to share with other PCs.", "Re-downloaded only if needed again.",
        paths(vec![
            t("C:\\Windows\\ServiceProfiles\\NetworkService\\AppData\\Local\\Microsoft\\Windows\\DeliveryOptimization\\Cache"),
            t("C:\\ProgramData\\Microsoft\\Windows\\DeliveryOptimization\\Cache"),
        ])).admin().svc(&["dosvc"]).done());
    r.push(B::new("dumps-system", "System crash dumps", System,
        "MEMORY.DMP, Minidump and LiveKernelReports from blue screens / driver hangs.",
        "Only needed to diagnose a crash. Delete if the system is stable.",
        paths(vec![
            t("C:\\Windows\\MEMORY.DMP"),
            tp("C:\\Windows\\Minidump", "*.dmp", 0),
            tp("C:\\Windows\\LiveKernelReports", "*.dmp", 0),
        ])).admin().done());
    r.push(B::new("wer-system", "Error reports (system)", System,
        "System-wide Windows Error Reporting archive and queue.", "None.",
        paths(vec![
            t("C:\\ProgramData\\Microsoft\\Windows\\WER\\ReportArchive"),
            t("C:\\ProgramData\\Microsoft\\Windows\\WER\\ReportQueue"),
            t("C:\\ProgramData\\Microsoft\\Windows\\WER\\Temp"),
        ])).admin().done());
    r.push(B::new("msi-leftover", "Interrupted installer leftovers", System,
        "C:\\Config.Msi — rollback data from an installer that did not finish.",
        "Safe when no installation is running. Windows recreates it on the next install.",
        paths(vec![t("C:\\Config.Msi")])).admin().risk(Careful).done());
    r.push(B::new("component-store", "Windows component store cleanup", System,
        "Runs DISM StartComponentCleanup: removes superseded Windows update files from WinSxS.",
        "Safe and supported by Microsoft (no /ResetBase). Takes a minute or two; you can still uninstall the latest updates.",
        Kind::Special(Special::ComponentStore)).admin().done());
    r.push(B::new("event-logs", "Windows event logs", System,
        "Clears the Event Viewer logs (Application, System, Security...).",
        "Erases troubleshooting history. Windows keeps logging afterwards.",
        Kind::Special(Special::EventLogs)).admin().risk(Careful).done());
    r.push(B::new("hibernation", "Hibernation file (hiberfil.sys)", System,
        "Turns off hibernation; frees a file about 40% of your RAM size.",
        "BEHAVIOUR CHANGE: no hibernate and no Fast Startup. Re-enable any time with `powercfg /h on`.",
        Kind::Special(Special::Hibernation)).admin().risk(Advanced).done());

    // ───────────────────────── GPU shader caches ─────────────────────────
    r.push(B::new("shader-nvidia", "NVIDIA shader cache", Gpu,
        "Compiled shaders NVIDIA's driver cached for games and apps.",
        "Games recompile shaders: expect a brief stutter on the first launch of each game.",
        paths(vec![
            t("%LOCALAPPDATA%\\NVIDIA\\DXCache"),
            t("%LOCALAPPDATA%\\NVIDIA\\GLCache"),
            t("%LOCALAPPDATA%\\NVIDIA\\OptixCache"),
            t("%USERPROFILE%\\AppData\\LocalLow\\NVIDIA\\PerDriverVersion\\DXCache"),
        ])).done());
    r.push(B::new("shader-other", "DirectX / AMD / Intel shader cache", Gpu,
        "Shader caches from DirectX, AMD and Intel drivers.", "Same as NVIDIA: first launch of games recompiles shaders.",
        paths(vec![
            t("%LOCALAPPDATA%\\D3DSCache"),
            t("%LOCALAPPDATA%\\AMD\\DxCache"),
            t("%LOCALAPPDATA%\\AMD\\DxcCache"),
            t("%LOCALAPPDATA%\\AMD\\VkCache"),
            t("%LOCALAPPDATA%\\Intel\\ShaderCache"),
        ])).done());

    // ───────────────────────── Browsers ─────────────────────────
    r.push(B::new("br-brave", "Brave cache", Browser,
        "Page cache, compiled JS and GPU caches. Cookies, history, passwords and sessions are NOT touched.",
        "Sites load a bit slower the first time. Close Brave for a full clean.",
        paths(chromium("%LOCALAPPDATA%\\BraveSoftware\\Brave-Browser\\User Data"))).close(&["brave.exe"]).done());
    r.push(B::new("br-edge", "Microsoft Edge cache", Browser,
        "Page cache, compiled JS and GPU caches. Logins, history and cookies are NOT touched.",
        "Sites load a bit slower the first time. Close Edge for a full clean.",
        paths(chromium("%LOCALAPPDATA%\\Microsoft\\Edge\\User Data"))).close(&["msedge.exe"]).done());
    r.push(B::new("br-chrome", "Google Chrome cache", Browser,
        "Page cache, compiled JS and GPU caches. Logins, history and cookies are NOT touched.",
        "Sites load a bit slower the first time. Close Chrome for a full clean.",
        paths(chromium("%LOCALAPPDATA%\\Google\\Chrome\\User Data"))).close(&["chrome.exe"]).done());
    r.push(B::new("br-avast", "Avast Secure Browser cache", Browser,
        "Cache of the Avast browser (not touched: logins, history).", "Sites load a bit slower the first time.",
        paths(chromium("%LOCALAPPDATA%\\AVAST Software\\Browser\\User Data"))).close(&["asb.exe", "avastbrowser.exe"]).done());
    r.push(B::new("br-firefox", "Firefox cache", Browser,
        "Firefox's disk cache and startup cache. Logins, history and cookies are NOT touched.",
        "Sites load a bit slower the first time. Close Firefox for a full clean.",
        paths(vec![
            t("%LOCALAPPDATA%\\Mozilla\\Firefox\\Profiles\\*\\cache2"),
            t("%LOCALAPPDATA%\\Mozilla\\Firefox\\Profiles\\*\\startupCache"),
        ])).close(&["firefox.exe"]).done());

    // ───────────────────────── Developer caches ─────────────────────────
    r.push(B::new("dev-pip", "pip cache", Dev, "Downloaded Python wheels.", "Packages are re-downloaded on next install.",
        paths(vec![t("%LOCALAPPDATA%\\pip\\Cache")])).done());
    r.push(B::new("dev-npm", "npm / yarn / pnpm caches", Dev, "Package tarballs cached by Node package managers.",
        "Packages are re-downloaded on next install.",
        paths(vec![
            t("%LOCALAPPDATA%\\npm-cache"),
            t("%APPDATA%\\npm-cache"),
            t("%LOCALAPPDATA%\\Yarn\\Cache"),
            t("%LOCALAPPDATA%\\pnpm-cache"),
        ])).done());
    r.push(B::new("dev-nuget", "NuGet HTTP & scratch caches", Dev, "Cached NuGet feed responses and temp restore data.",
        "Re-fetched on next restore. (The global packages folder is a separate, opt-in rule.)",
        paths(vec![
            t("%LOCALAPPDATA%\\NuGet\\v3-cache"),
            t("%LOCALAPPDATA%\\NuGet\\plugins-cache"),
            t("%LOCALAPPDATA%\\NuGet\\Cache"),
        ])).done());
    r.push(B::new("dev-go", "Go build cache", Dev, "Compiled package cache of the Go toolchain.", "First build afterwards is slower.",
        paths(vec![t("%LOCALAPPDATA%\\go-build")])).done());
    r.push(B::new("dev-rustup", "rustup downloads & temp", Dev, "Toolchain archives and temp files left by rustup.", "None.",
        paths(vec![t("%USERPROFILE%\\.rustup\\downloads"), t("%USERPROFILE%\\.rustup\\tmp")])).done());
    r.push(B::new("dev-cargo", "Cargo registry cache", Dev,
        "Downloaded crate archives and extracted sources in ~/.cargo/registry.",
        "Crates are re-downloaded on next build; offline builds will fail until then.",
        paths(vec![
            t("%USERPROFILE%\\.cargo\\registry\\cache"),
            t("%USERPROFILE%\\.cargo\\registry\\src"),
            t("%USERPROFILE%\\.cargo\\git\\checkouts"),
        ])).risk(Careful).done());
    r.push(B::new("dev-nuget-global", "NuGet global packages", Dev,
        "~/.nuget/packages — every NuGet package version you ever restored.",
        "Next restore re-downloads what each project needs; offline builds fail until then.",
        paths(vec![t("%USERPROFILE%\\.nuget\\packages")])).risk(Careful).done());
    r.push(B::new("dev-jvm", "Gradle & Maven caches", Dev, "Downloaded JVM dependencies.",
        "Re-downloaded on next build (can be large).",
        paths(vec![t("%USERPROFILE%\\.gradle\\caches"), t("%USERPROFILE%\\.m2\\repository")])).risk(Careful).done());

    // ───────────────────────── Docker ─────────────────────────
    r.push(B::new("docker-build", "Docker build cache", Docker, "Layers cached from past `docker build` runs.",
        "Next build of each image starts from scratch.", Kind::Special(Special::DockerBuildCache)).done());
    r.push(B::new("docker-stopped", "Docker stopped containers & dangling images", Docker,
        "Containers that exited and image layers with no tag.", "Stopped containers lose their writable layer (the data inside them).",
        Kind::Special(Special::DockerStopped)).risk(Careful).on().done());
    r.push(B::new("docker-images", "Docker unused images", Docker,
        "Images not used by any container.", "They must be pulled/built again when you need them.",
        Kind::Special(Special::DockerUnusedImages)).risk(Careful).done());
    r.push(B::new("docker-volumes", "Docker unused volumes", Docker,
        "Volumes not attached to any container (databases, uploads...).",
        "DATA LOSS RISK: anything stored only in those volumes is gone for good.",
        Kind::Special(Special::DockerVolumes)).risk(Advanced).done());

    // ───────────────────────── App caches ─────────────────────────
    r.push(B::new("app-steam", "Steam web & shader cache", Apps, "Steam's embedded browser cache and compiled shaders.",
        "Steam store pages load slower once; game shaders recompile.",
        paths(vec![
            t("%LOCALAPPDATA%\\Steam\\htmlcache"),
            t("%ProgramFiles(x86)%\\Steam\\appcache\\httpcache"),
            t("%ProgramFiles(x86)%\\Steam\\steamapps\\shadercache"),
        ])).close(&["steam.exe", "steamwebhelper.exe"]).done());
    r.push(B::new("app-telegram", "Telegram media cache", Apps, "Cached photos/videos you viewed in Telegram.",
        "Media re-downloads from the cloud when you open it again (chats are not affected).",
        paths(vec![
            t("%APPDATA%\\Telegram Desktop\\tdata\\user_data\\cache"),
            t("%APPDATA%\\Telegram Desktop\\tdata\\user_data\\media_cache"),
        ])).close(&["telegram.exe"]).done());
    r.push(B::new("app-electron", "Discord / Slack / VS Code / Spotify caches", Apps,
        "Chromium-style caches of popular Electron apps (settings and logins are NOT touched).",
        "Apps rebuild their caches on next start. Close the apps for a full clean.",
        paths(vec![
            t("%APPDATA%\\discord\\Cache"), t("%APPDATA%\\discord\\Code Cache"), t("%APPDATA%\\discord\\GPUCache"),
            t("%APPDATA%\\Slack\\Cache"), t("%APPDATA%\\Slack\\Code Cache"), t("%APPDATA%\\Slack\\GPUCache"),
            t("%APPDATA%\\Code\\Cache"), t("%APPDATA%\\Code\\CachedData"), t("%APPDATA%\\Code\\Code Cache"), t("%APPDATA%\\Code\\GPUCache"),
            t("%LOCALAPPDATA%\\Spotify\\Data"),
        ])).close(&["discord.exe", "slack.exe", "code.exe", "spotify.exe"]).done());

    // ───────────────────────── Privacy traces ─────────────────────────
    r.push(B::new("priv-recent", "Recent files & jump lists", Privacy,
        "Shortcuts to recently opened files and the taskbar jump lists.",
        "Explorer's 'Recent' and taskbar jump lists start empty. No real file is deleted.",
        paths(vec![
            tp("%APPDATA%\\Microsoft\\Windows\\Recent", "*.lnk", 0),
            t("%APPDATA%\\Microsoft\\Windows\\Recent\\AutomaticDestinations"),
            t("%APPDATA%\\Microsoft\\Windows\\Recent\\CustomDestinations"),
        ])).risk(Careful).done());

    r
}

pub fn find<'a>(rules: &'a [Rule], id: &str) -> Option<&'a Rule> {
    rules.iter().find(|r| r.id == id)
}
