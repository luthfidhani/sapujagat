/* Dev-only: fake Tauri bridge so the UI can be previewed in a plain browser. Not shipped.
   Data mirrors a real scan of this PC; the live feed is a random walk emitted once per second. */
(function () {
  const GB = 1024 ** 3, MB = 1024 ** 2;
  const R = (id, name, cat, cat_title, risk, admin, def, what, effect, bytes, extra = {}) => ({ id, name, cat, cat_title, risk, admin, default_on: def, what, effect, close: [], special: false, locations: ['%LOCALAPPDATA%\\' + name.replace(/\s/g, '')], bytes, files: 10, items: [], available: true, note: null, running: [], ...extra });
  const rules = [
    R('temp-user', 'User temp files', 'system', 'Windows & System', 'safe', false, true, 'Leftovers apps wrote to %TEMP%.', 'Files older than 2 days only.', 24.7 * MB, { locations: ['%TEMP%   older than 2 days'] }),
    R('crash-dumps', 'Crash dumps (user)', 'system', 'Windows & System', 'safe', false, true, 'Memory dumps apps left after crashing.', 'Only useful for debugging.', 274 * MB),
    R('thumb-cache', 'Thumbnail & icon cache', 'system', 'Windows & System', 'safe', false, true, "Explorer's cached previews.", 'Explorer rebuilds it gradually.', 84.1 * MB),
    R('recycle-bin', 'Recycle Bin (all drives)', 'system', 'Windows & System', 'careful', false, true, 'Files you already deleted.', 'Permanent: items in the Recycle Bin can no longer be restored.', 0, { special: true, locations: ['Built-in system action (no folder is deleted by path)'] }),
    R('temp-other-users', 'Temp files of other Windows accounts', 'system', 'Windows & System', 'safe', true, true, '%TEMP% of every other user account.', 'Files older than 2 days only.', 1.99 * GB, { locations: ['C:\\Users\\*\\AppData\\Local\\Temp   older than 2 days'] }),
    R('win-logs', 'Windows servicing logs', 'system', 'Windows & System', 'safe', true, true, 'CBS, DISM and Windows Update logs.', 'Only matters if debugging a failed update.', 62.7 * MB),
    R('dumps-system', 'System crash dumps', 'system', 'Windows & System', 'safe', true, true, 'MEMORY.DMP, Minidump from blue screens.', 'Only needed to diagnose a crash.', 1.25 * GB),
    R('component-store', 'Windows component store cleanup', 'system', 'Windows & System', 'safe', true, true, 'Runs DISM StartComponentCleanup.', 'Safe and supported by Microsoft.', null, { special: true, note: 'size known after run' }),
    R('event-logs', 'Windows event logs', 'system', 'Windows & System', 'careful', true, false, 'Clears the Event Viewer logs.', 'Erases troubleshooting history.', 384 * MB, { special: true }),
    R('hibernation', 'Hibernation file (hiberfil.sys)', 'system', 'Windows & System', 'advanced', true, false, 'Turns off hibernation.', 'Behaviour change: no hibernate and no Fast Startup.', null, { special: true }),
    R('old-drivers', 'Old driver versions (DriverStore)', 'system', 'Windows & System', 'careful', true, false, 'Older versions of driver packages that Windows keeps after installing a newer one.', 'You can no longer roll those drivers back to the old version. Drivers in use, boot-critical or built into Windows are never removed. A restore point is created first.', 1.34 * GB, { special: true, files: 6, items: ['oem42.inf  NVIDIA  Display 31.0.15.2802  (612 MB), newer 31.0.15.5222 stays', 'oem17.inf  Intel  Display 30.0.101.1994  (402 MB), newer 31.0.101.5333 stays', 'oem9.inf  Realtek  Media 6.0.9500.1  (88 MB), newer 6.0.9650.1 stays'] }),
    R('shader-nvidia', 'NVIDIA shader cache', 'gpu', 'GPU shader caches', 'safe', false, true, "Compiled shaders NVIDIA's driver cached.", 'Games recompile shaders: brief stutter on first launch.', 4.31 * GB, { locations: ['%LOCALAPPDATA%\\NVIDIA\\DXCache', '%LOCALAPPDATA%\\NVIDIA\\GLCache'] }),
    R('br-brave', 'Brave cache', 'browser', 'Browsers', 'safe', false, true, 'Page cache and GPU caches. Cookies, history and passwords are not touched.', 'Sites load slower the first time.', 791 * MB, { running: ['brave.exe'], close: ['brave.exe'] }),
    R('br-edge', 'Microsoft Edge cache', 'browser', 'Browsers', 'safe', false, true, 'Page cache and GPU caches.', 'Sites load slower the first time.', 43.5 * MB),
    R('dev-pip', 'pip cache', 'dev', 'Developer caches', 'safe', false, true, 'Downloaded Python wheels.', 'Re-downloaded on next install.', 90.2 * MB),
    R('dev-cargo', 'Cargo registry cache', 'dev', 'Developer caches', 'careful', false, false, 'Downloaded crate archives.', 'Offline builds fail until re-downloaded.', 309 * MB),
    R('docker-build', 'Docker build cache', 'docker', 'Docker', 'safe', false, true, 'Layers cached from past builds.', 'Next build starts from scratch.', null, { special: true, available: false, note: 'Docker is not running. Start Docker Desktop to clean this.' }),
    R('docker-volumes', 'Docker unused volumes', 'docker', 'Docker', 'advanced', false, false, 'Volumes not attached to any container.', 'Data loss risk: anything only in those volumes is gone.', null, { special: true, available: false, note: 'Docker is not running' }),
    R('docker-compact', "Shrink Docker's virtual disk", 'docker', 'Docker', 'advanced', true, false, "Docker keeps its images and volumes in one virtual disk file (VHDX) that never gets smaller by itself, even after pruning.", 'Stops Docker Desktop and every running container first, then compacts the disk file. This can take several minutes. Docker stays off afterwards until you start it. Nothing inside the disk is deleted.', null, { special: true, files: 1, items: ['C:\\ProgramData\\DockerDesktop\\vm-data\\DockerDesktop.vhdx  (5.73 GB)'], note: 'Disk file is 5.73 GB. How much shrinks is known after compacting.' }),
    R('app-steam', 'Steam web & shader cache', 'apps', 'App caches', 'safe', false, true, "Steam's embedded browser cache.", 'Store pages load slower once.', 356 * MB),
    R('priv-recent', 'Recent files & jump lists', 'privacy', 'Privacy traces', 'careful', false, false, 'Shortcuts to recently opened files.', 'Recent lists start empty.', 1.8 * MB),
  ];
  const q = new URLSearchParams(location.search);
  const drives = [{ root: 'C:\\', total: 219 * GB, free: 116 * GB }, { root: 'D:\\', total: 384 * GB, free: 115 * GB }, { root: 'E:\\', total: 350 * GB, free: 43 * GB }];
  const base = [
    ['brave.exe', 14, 1.72 * GB, 2.1, 'UserApp', 'Application you launched', 'C:\\Program Files\\BraveSoftware\\Brave-Browser\\Application\\brave.exe'],
    ['svchost.exe', 89, 1.54 * GB, 0.4, 'Protected', 'Core Windows / driver / your terminal', 'C:\\Windows\\System32\\svchost.exe'],
    ['msedgewebview2.exe', 12, 818 * MB, 1.7, 'Protected', 'Shared web runtime used by other apps (incl. Claude, Widgets)', 'C:\\Program Files (x86)\\Microsoft\\EdgeWebView\\Application\\msedgewebview2.exe'],
    ['explorer.exe', 1, 521 * MB, 0.2, 'Protected', 'This tool / its parent terminal', 'C:\\Windows\\explorer.exe'],
    ['claude.exe', 6, 438 * MB, 0.6, 'Protected', 'Core Windows / driver / your terminal', 'C:\\Program Files\\WindowsApps\\Claude\\claude.exe'],
    ['aswEngSrv.exe', 1, 259 * MB, 0.1, 'Protected', 'Security engine', 'C:\\Program Files\\Avast Software\\Avast\\aswEngSrv.exe'],
    ['Dell.TechHub.Instrumentation.SubAgent.exe', 1, 244 * MB, 0, 'Helper', 'Dell background helper (telemetry / update agent)', 'C:\\Program Files\\Dell\\TechHub\\Dell.TechHub.Instrumentation.SubAgent.exe'],
    ['AvastUI.exe', 1, 242 * MB, 0.2, 'Helper', 'Avast tray UI (the protection engine keeps running)', 'C:\\Program Files\\Avast Software\\Avast\\AvastUI.exe'],
    ['Killer.exe', 1, 190 * MB, 0.4, 'Helper', 'Killer network suite UI (networking works without it)', 'C:\\Program Files\\Killer Networking\\Killer.exe'],
    ['Telegram.exe', 1, 160 * MB, 0, 'UserApp', 'Application you launched', 'C:\\Users\\Luthfi\\AppData\\Roaming\\Telegram Desktop\\Telegram.exe'],
    ['Dell.D3.WinSvc.exe', 1, 147 * MB, 0, 'Helper', 'Dell background helper (telemetry / update agent)', 'C:\\Program Files\\Dell\\D3\\Dell.D3.WinSvc.exe'],
    ['UninstallMonitor.exe', 1, 128 * MB, 0.2, 'Helper', 'IObit uninstall monitor', 'C:\\Program Files (x86)\\IObit\\IObit Uninstaller\\UninstallMonitor.exe'],
    ['Docker Desktop.exe', 4, 96 * MB, 0, 'UserApp', 'Application you launched', 'C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe'],
  ];
  let t = 0, cpu = 22, rx = 600 * 1024, tx = 60 * 1024;
  const walk = (v, lo, hi, step) => Math.max(lo, Math.min(hi, v + (Math.random() - 0.5) * step));
  const sample = () => {
    t++; cpu = walk(cpu, 4, 70, 14); rx = walk(rx, 0, 4 * MB, 700 * 1024); tx = walk(tx, 0, 600 * 1024, 80 * 1024);
    return {
      cpu_total: cpu, cpu_cores: Array.from({ length: 16 }, (_, i) => Math.max(1, Math.min(100, cpu + (Math.sin(t / 3 + i) * 22)))),
      ram_used: 12.7 * GB + Math.sin(t / 5) * 90 * MB, ram_total: 31.7 * GB, swap_used: 1.1 * GB, swap_total: 4.5 * GB, proc_count: 312,
      net_rx: Math.round(rx), net_tx: Math.round(tx), disk_read: Math.round(Math.max(0, walk(2 * MB, 0, 18 * MB, 9 * MB))), disk_write: Math.round(Math.max(0, walk(900 * 1024, 0, 9 * MB, 4 * MB))), uptime: 3 * 86400 + 4 * 3600 + t,
      groups: base.map(([name, n, mem, c, cls, reason, exe]) => ({ name, exe, pids: Array.from({ length: n }, (_, i) => 1000 + i), members: Array.from({ length: n }, (_, i) => ({ pid: 1000 + i, mem: mem / n, cpu: c / n })), mem: mem + Math.sin(t / 4 + n) * 4 * MB, cpu: Math.max(0, c + (Math.random() - 0.5) * 1.4), disk: Math.random() < 0.5 ? Math.round(Math.random() * 600 * 1024) : 0, class: cls, reason })),
      battery: { present: true, percent: 78, charging: false, ac: false, minutes_left: 214 },
      drives: t % 5 === 1 ? drives : null,
    };
  };
  const startup = [
    { name: 'AutoDarkMode', command: '"C:\\Users\\Luthfi\\AppData\\Local\\Programs\\AutoDarkMode\\adm-app\\core\\AutoDarkModeSvc.exe"', scope: 'User (registry)', enabled: true, needs_admin: false, advice: null },
    { name: 'AvastUI.exe', command: '"C:\\Program Files\\Avast Software\\Avast\\AvLaunch.exe" /gui', scope: 'Machine (registry)', enabled: true, needs_admin: true, advice: 'Avast tray UI (the protection engine keeps running)' },
    { name: 'RtkAudUService', command: '"C:\\Windows\\System32\\DriverStore\\FileRepository\\realtekservice.inf_amd64\\RtkAudUService64.exe" -background', scope: 'Machine (registry)', enabled: true, needs_admin: true, advice: null },
    { name: 'Docker Desktop', command: 'C:\\Program Files\\Docker\\Docker\\Docker Desktop.exe', scope: 'User (registry)', enabled: false, needs_admin: false, advice: null },
    { name: 'Steam', command: '"C:\\Program Files (x86)\\Steam\\steam.exe" -silent', scope: 'User (registry)', enabled: false, needs_admin: false, advice: null },
  ];
  const tag = (text, kind) => ({ text, kind });
  const items = [['ProgramData', 39.4, 1, tag('Shared app data', 'info')], ['Windows', 37.7, 1, tag('Windows, do not touch', 'danger')], ['Users', 33.6, 1, tag('Contains cleanable: User temp files', 'contains')], ['Program Files', 14.5, 1, tag('Installed apps', 'info')], ['Program Files (x86)', 10.4, 1, tag('Contains cleanable: Steam web & shader cache', 'contains')], ['ysl_rom', 3.41, 1, tag('Your folder, review manually', 'review')], ['pagefile.sys', 1, 0, tag('Windows virtual memory file', 'warn')], ['SamFwTool', 0.3, 1, tag('Your folder, review manually', 'review')], ['temp', 0.06, 1, tag('Your folder, review manually', 'review')], ['Recovery', 0.01, 1, null]]
    .map(([name, gb, d, tg]) => ({ name, path: 'C:\\' + name, bytes: gb * GB, is_dir: !!d, tag: tg }));
  const analysis = { root: 'C:\\', parent: null, items, total: 140 * GB, unreadable: 331, big_files: [
    { path: 'C:\\Users\\Luthfi\\AppData\\Local\\Packages\\Claude_pzs8sxrjxfjjc\\LocalCache\\Roaming\\Claude\\vm_bundles\\rootfs.vhdx', bytes: 7.85 * GB, tag: tag('Claude app data, protected', 'protected') },
    { path: 'C:\\Users\\Luthfi\\Desktop\\Stronghold.Crusader.Definitive.Edition.iso', bytes: 5.78 * GB, tag: null },
    { path: 'C:\\ProgramData\\DockerDesktop\\vm-data\\DockerDesktop.vhdx', bytes: 5.73 * GB, tag: tag('Shared app data', 'info') }] };
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const mkAn = (root, scale, took) => ({ ...analysis, root, parent: root.length <= 3 ? null : 'C:\\', analyzed_at: Math.floor(Date.now() / 1000) - 3 * 3600, took_secs: took, total: analysis.total * scale, items: analysis.items.map((i) => ({ ...i, path: root + i.name, bytes: i.bytes * scale })) });
  const store = {};
  if (!q.get('fresh')) { store['C:\\'] = mkAn('C:\\', 1, 71); store['D:\\'] = mkAn('D:\\', 0.6, 38); }
  const dev = [
    { path: 'D:\\work\\shop\\node_modules', kind: 'node_modules', bytes: 412 * MB, age_days: 140, project: 'D:\\work\\shop', rebuild_hint: 'npm install' },
    { path: 'D:\\work\\api\\target', kind: 'Rust target', bytes: 1.4 * GB, age_days: 62, project: 'D:\\work\\api', rebuild_hint: 'cargo build' },
    { path: 'E:\\old\\ml\\.venv', kind: 'Python venv', bytes: 880 * MB, age_days: 9, project: 'E:\\old\\ml', rebuild_hint: 'recreate and pip install' }];
  const history = [
    { when: '2026-11-16 13:02:11', total: 5.94 * GB, items: 11, top: [['NVIDIA shader cache', 4.31 * GB], ['Brave cache', 791 * MB], ['Steam web & shader cache', 356 * MB]] },
    { when: '2026-11-16 12:05:19', total: 3.33 * GB, items: 7, top: [['Temp files of other Windows accounts', 1.99 * GB], ['System crash dumps', 1.25 * GB]] },
    { when: '2026-11-16 11:58:50', total: 1.39 * GB, items: 4, top: [['User temp files', 1.39 * GB]] }];
  const lf = (name, place, mb, age, root) => ({ path: root + '\\' + name, name, place, bytes: mb * MB, age_days: age, why: `No installed program, Store app or running process matches "${name}". Last changed ${age} days ago.` });
  const leftovers = [lf('Adobe Genuine Service', 'ProgramData', 612, 410, 'C:\\ProgramData'), lf('OldVPNClient', 'AppData (Roaming)', 340, 530, 'C:\\Users\\Luthfi\\AppData\\Roaming'), lf('FlashPlayerCache', 'AppData (Local)', 118, 960, 'C:\\Users\\Luthfi\\AppData\\Local'), lf('Wondershare Helper Compact', 'ProgramData', 64, 188, 'C:\\ProgramData'), lf('tmpinstaller_7z', 'AppData (Local)', 22, 75, 'C:\\Users\\Luthfi\\AppData\\Local')];
  let mockSettings = { exclusions: ['D:\\Projects\\keep-this-folder'], disabled_rules: [], recommended: {}, restore_point: true };
  const settingsView = () => ({ settings: mockSettings, rules: rules.map((r) => ({ id: r.id, name: r.name, cat: r.cat, cat_title: r.cat_title, risk: r.risk, admin: r.admin, enabled: !mockSettings.disabled_rules.includes(r.id), recommended: mockSettings.recommended[r.id] ?? r.default_on, base_recommended: r.default_on })) });
  const handlers = {
    boot: () => ({ admin: q.get('admin') === '1', sys: { cpu: 'Intel Core i7-11800H', cores: 16, ram_used: 12.7 * GB, ram_total: 31.7 * GB, host: 'LUTHFI' }, drives, version: '0.1.0' }),
    drives: () => drives, catalog: () => rules.map((r) => ({ ...r, bytes: null })), scan_rules: () => rules,
    system_details: () => ({ host: 'LUTHFI', os_name: 'Windows 11 Pro', os_display_version: '25H2', os_build: '26300.9974', kernel: '26300', uptime_secs: 3 * 86400, cpu_brand: '11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz', physical_cores: 8, logical_cores: 16, cpu_mhz: 2304, ram_total: 31.7 * GB, swap_total: 4.5 * GB, gpus: [{ name: 'NVIDIA GeForce RTX 3050 Ti Laptop GPU', vram: 4 * GB }, { name: 'Intel(R) UHD Graphics', vram: null }], power_plan: 'Balanced', battery: { present: true, percent: 78, charging: false, ac: false, minutes_left: 214 }, arch: 'x86_64', engine_version: '0.1.0', log_dir: 'C:\\Users\\Luthfi\\AppData\\Local\\sapujagat\\logs' }),
    history: () => (q.get('empty') ? [] : history),
    services_list: () => [{ name: 'DellTechHub', label: 'Dell TechHub', why: 'Dell hub and telemetry agent', running: true }, { name: 'DellClientManagementService', label: 'Dell Client Management', why: 'Dell remote-management agent', running: true }],
    startup_list: () => startup, dev_find: () => dev,
    analysis_status: () => Object.keys(store).map((root) => ({ root, analyzed_at: store[root].analyzed_at, took_secs: store[root].took_secs })),
    analysis_get: ({ path }) => store[path] || null,
    analyze_path: async ({ path, force }) => { if (!force && store[path]) return store[path]; await sleep(500); store[path] = mkAn(path, path.length <= 3 ? 1 : 0.4, 12); return store[path]; },
    analyze_all: async ({ force }) => {
      const jobs = drives.map((d, i) => (async () => {
        if (!force && store[d.root]) return emit('analysis-event', { root: d.root, status: 'done', view: store[d.root] });
        emit('analysis-event', { root: d.root, status: 'start' });
        await sleep(1200 + i * 900);
        store[d.root] = mkAn(d.root, [1, 0.6, 0.45][i] || 0.5, [71, 38, 52][i] || 40);
        emit('analysis-event', { root: d.root, status: 'done', view: store[d.root] });
      })());
      await Promise.all(jobs);
    },
    settings_get: () => settingsView(),
    settings_save: ({ new: n }) => { mockSettings = JSON.parse(JSON.stringify(n)); return settingsView(); },
    leftovers_find: async () => { await sleep(700); return leftovers.slice(); },
    path_reveal: () => null,
    path_recycle: ({ path }) => { const i = leftovers.findIndex((l) => l.path === path); if (i >= 0) leftovers.splice(i, 1); return null; },
    process_icons: ({ paths }) => Object.fromEntries(paths.filter((p) => !/svchost|msedgewebview|explorer/i.test(p)).map((p) => { const c = ['#3e6ae1', '#c4352c', '#1d8a5b', '#a96a0c', '#6d4bd6'][p.length % 5]; const l = (p.split('\\').pop() || '?')[0].toUpperCase(); return [p, 'data:image/svg+xml;utf8,' + encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" rx="7" fill="${c}"/><text x="16" y="22" font-size="17" font-family="Arial" font-weight="700" text-anchor="middle" fill="#fff">${l}</text></svg>`)]; })),
    trim_memory: () =>({ trimmed: 18, used_before: 12.7 * GB, used_after: 11.9 * GB }), kill_pids: () => ({ killed: 3, requested: 3 }),
    run_clean: async () => {
      const done = rules.filter((r) => r.default_on && r.available && r.bytes);
      for (let i = 0; i < done.length; i++) { cb({ payload: { kind: 'RuleStart', index: i, total: done.length, id: done[i].id, name: done[i].name } }); await new Promise((r) => setTimeout(r, 120)); cb({ payload: { kind: 'RuleDone', index: i, total: done.length, result: { name: done[i].name, freed: done[i].bytes, ok: true, note: '' } } }); }
      return { before: drives, after: drives.map((d, i) => ({ ...d, free: d.free + (i === 0 ? 8.9 * GB : 0) })), restore_point: true, killed_apps: 0, total_freed: 8.9 * GB, results: done.map((r) => ({ id: r.id, name: r.name, freed: r.bytes, files: 120, locked: r.id === 'br-brave' ? 1 : 0, ok: true, note: r.id === 'br-brave' ? '1 file in use was skipped' : '' })) };
    },
  };
  const listeners = {};
  const emit = (name, payload) => (listeners[name] || []).forEach((f) => f({ payload }));
  const cb = (e) => emit('clean-event', e.payload);
  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => { if (!handlers[cmd]) throw new Error('mock: ' + cmd); return handlers[cmd](args); } },
    event: { listen: async (name, f) => { (listeners[name] ||= []).push(f); if (name === 'live') { setTimeout(() => f({ payload: sample() }), 50); setInterval(() => f({ payload: sample() }), 1000); } return () => {}; } },
  };
  window.__MOCK__ = true;
})();
