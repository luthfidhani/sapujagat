'use strict';
/* Sapu Jagat front-end core: helpers, icons, theme, live feed, router, modal. Classic scripts share this scope.
   Every dynamic string reaching innerHTML goes through esc(). */

const TAURI = window.__TAURI__;
const invoke = (cmd, args) => TAURI.core.invoke(cmd, args);
const listen = (ev, cb) => TAURI.event.listen(ev, cb);
const $ = (s, el = document) => el.querySelector(s);
const $$ = (s, el = document) => [...el.querySelectorAll(s)];
const view = $('#view');

const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
function human(b) {
  const U = ['B', 'KB', 'MB', 'GB', 'TB'];
  let v = b, i = 0;
  while (v >= 1024 && i < 4) { v /= 1024; i++; }
  if (i === 0) return `${Math.round(b)} B`;
  return `${v >= 100 ? v.toFixed(0) : v >= 10 ? v.toFixed(1) : v.toFixed(2)} ${U[i]}`;
}
const rate = (b) => `${human(b)}/s`;
function dur(s) {
  const d = Math.floor(s / 86400), h = Math.floor((s % 86400) / 3600), m = Math.floor((s % 3600) / 60);
  return d ? `${d}d ${h}h` : h ? `${h}h ${m}m` : `${m}m`;
}
const tone = (f) => (f > 0.9 ? 'bad' : f > 0.78 ? 'warn' : '');

/* ───────── icons: 1.5px line set, no emoji ───────── */
const ICONS = {
  overview: '<path d="M4 16a8 8 0 1 1 16 0"/><path d="M12 16l3.5-4.5"/><path d="M5 19h14"/>',
  clean: '<path d="M5 7h14"/><path d="M9 7V5h6v2"/><path d="M7 7l1 12h8l1-12"/><path d="M10 11v5M14 11v5"/>',
  boost: '<rect x="7" y="7" width="10" height="10" rx="1.5"/><path d="M10 3v4M14 3v4M10 17v4M14 17v4M3 10h4M3 14h4M17 10h4M17 14h4"/>',
  startup: '<path d="M12 4v8"/><path d="M7.2 7.2a7 7 0 1 0 9.6 0"/>',
  storage: '<ellipse cx="12" cy="6.5" rx="7" ry="2.5"/><path d="M5 6.5v11c0 1.4 3.1 2.5 7 2.5s7-1.1 7-2.5v-11"/><path d="M5 12c0 1.4 3.1 2.5 7 2.5s7-1.1 7-2.5"/>',
  developer: '<path d="M9 8l-4 4 4 4"/><path d="M15 8l4 4-4 4"/><path d="M13.5 5.5l-3 13"/>',
  history: '<circle cx="12" cy="12" r="8"/><path d="M12 7.5V12l3 2"/>',
  system: '<rect x="3" y="4.5" width="18" height="12" rx="2"/><path d="M8.5 20h7M12 16.5V20"/>',
  catalog: '<path d="M5 6h14M5 12h14M5 18h9"/>',
  leftovers: '<path d="M3.5 7a2 2 0 0 1 2-2H10l2 2.5h6.5a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"/><path d="M9.5 13.5h5"/>',
  settings: '<path d="M4 7h9M17 7h3M4 17h3M11 17h9"/><circle cx="15" cy="7" r="2"/><circle cx="9" cy="17" r="2"/>',
  reveal: '<path d="M14 5h5v5"/><path d="M19 5l-8 8"/><path d="M17 13v4a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V9a2 2 0 0 1 2-2h4"/>',
  trash: '<path d="M5 7h14"/><path d="M9 7V5h6v2"/><path d="M7 7l1 12h8l1-12"/>',
  plus: '<path d="M12 5v14M5 12h14"/>',
  x: '<path d="M6 6l12 12M18 6L6 18"/>',
  lock: '<rect x="5" y="11" width="14" height="9" rx="2"/><path d="M8 11V8a4 4 0 0 1 8 0v3"/>',
  chevron: '<path d="M9 6l6 6-6 6"/>',
  search: '<circle cx="11" cy="11" r="6"/><path d="M20 20l-4.2-4.2"/>',
  refresh: '<path d="M20 11a8 8 0 1 0-2.3 5.7"/><path d="M20 5v6h-6"/>',
  down: '<path d="M12 5v14"/><path d="M6 13l6 6 6-6"/>',
  up: '<path d="M12 19V5"/><path d="M6 11l6-6 6 6"/>',
  warn: '<path d="M12 4l9 16H3z"/><path d="M12 10v4M12 17.2v.01"/>',
  shield: '<path d="M12 3l7 3v5c0 5-3 8-7 10-4-2-7-5-7-10V6z"/>',
  folder: '<path d="M3.5 7a2 2 0 0 1 2-2H10l2 2.5h6.5a2 2 0 0 1 2 2V17a2 2 0 0 1-2 2h-13a2 2 0 0 1-2-2z"/>',
  file: '<path d="M7 3.5h7l4 4V20a1 1 0 0 1-1 1H7a1 1 0 0 1-1-1V4.5a1 1 0 0 1 1-1z"/><path d="M14 3.5V8h4"/>',
  up_dir: '<path d="M12 19V6"/><path d="M6.5 11.5L12 6l5.5 5.5"/>',
};
const ic = (n, cls = '') => `<svg class="ic ${cls}" viewBox="0 0 24 24" aria-hidden="true">${ICONS[n] || ''}</svg>`;

/* ───────── state ───────── */
const S = {
  boot: null, rules: null, sel: new Set(), selInit: false, scanning: null, busy: false, showingResult: false,
  live: null, hist: { cpu: [], ram: [], rx: [], tx: [], disk: [] },
  details: null, history: null, startup: null, svcs: null, catalog: null,
  psel: new Set(),
  cf: { q: '', mode: 'all', sort: 'size' },
  sort: {},
  an: { drive: null, path: null, data: null, loading: false, err: null, mode: 'map', running: new Set(), status: {}, init: false },
  dev: { roots: new Set(), found: null, sel: new Set(), loading: false },
};
const PAGES = {};
let route = 'overview';

const NAV = [
  ['Maintain', [['overview', 'Overview'], ['clean', 'Clean'], ['boost', 'Boost'], ['startup', 'Startup']]],
  ['Inspect', [['storage', 'Storage'], ['leftovers', 'Leftovers'], ['developer', 'Developer'], ['history', 'History'], ['system', 'System']]],
  ['Reference', [['catalog', 'Catalog'], ['settings', 'Settings']]],
];
const TITLES = {};
NAV.forEach(([, items]) => items.forEach(([id, t]) => (TITLES[id] = t)));

/* ───────── theme ───────── */
function getTheme() { try { return localStorage.getItem('sj-theme') || 'auto'; } catch { return 'auto'; } }
function setTheme(p) {
  try { localStorage.setItem('sj-theme', p); } catch {}
  if (p === 'auto') document.documentElement.removeAttribute('data-theme');
  else document.documentElement.setAttribute('data-theme', p);
  renderThemeSeg();
  if (PAGES[route]?.themed) PAGES[route].themed();
}
function renderThemeSeg() {
  const cur = getTheme();
  $('#themeseg').innerHTML = [['light', 'Light'], ['dark', 'Dark'], ['auto', 'Auto']].map(([k, l]) => `<button data-theme="${k}" class="${cur === k ? 'on' : ''}" aria-pressed="${cur === k}">${l}</button>`).join('');
}

/* ───────── small UI kit ───────── */
function toast(msg, kind = '') {
  const t = document.createElement('div');
  t.className = 'toast';
  t.innerHTML = `<span class="dot ${kind}"></span><span>${esc(msg)}</span>`;
  $('#toasts').appendChild(t);
  setTimeout(() => t.remove(), 5200);
}

function confirmModal({ title, intro = '', body = '', ok = 'Confirm', danger = false, ack = null, cancel = 'Cancel' }) {
  return new Promise((resolve) => {
    const scrim = document.createElement('div');
    scrim.className = 'scrim';
    scrim.innerHTML = `<div class="modal" role="dialog" aria-modal="true" aria-label="${esc(title)}">
      <h2>${esc(title)}</h2>${intro ? `<div class="intro">${intro}</div>` : ''}
      <div class="mb">${body}</div>
      ${ack ? `<label class="ack"><span class="chk"><input type="checkbox" id="m-ack"><i></i></span><span>${esc(ack)}</span></label>` : ''}
      <div class="btns">${cancel ? `<button class="btn" data-x="cancel">${esc(cancel)}</button>` : ''}
      <button class="btn ${danger ? 'danger' : 'primary'}" data-x="ok" ${ack ? 'disabled' : ''}>${esc(ok)}</button></div></div>`;
    document.body.appendChild(scrim);
    const done = (okv) => {
      const values = {};
      $$('input[data-key]', scrim).forEach((i) => (values[i.dataset.key] = i.checked));
      scrim.remove();
      document.removeEventListener('keydown', onKey);
      resolve({ ok: okv, values });
    };
    const onKey = (e) => { if (e.key === 'Escape') done(false); };
    document.addEventListener('keydown', onKey);
    scrim.addEventListener('click', (e) => {
      if (e.target === scrim) done(false);
      const x = e.target.closest('[data-x]');
      if (x) done(x.dataset.x === 'ok');
    });
    const a = $('#m-ack', scrim);
    if (a) a.addEventListener('change', () => ($('[data-x="ok"]', scrim).disabled = !a.checked));
  });
}

const loading = (msg, sub = '') => `<div class="center"><div><div class="spin"></div><div><b>${esc(msg)}</b></div>${sub ? `<div class="muted" style="margin-top:4px">${esc(sub)}</div>` : ''}</div></div>`;
const mark = (cls, txt) => `<span class="mark ${cls}"><i></i>${esc(txt)}</span>`;
const riskLabel = { safe: 'Safe', careful: 'Careful', advanced: 'Advanced' };
const classLabel = { Protected: ['protected', 'Protected'], Helper: ['helper', 'Helper'], UserApp: ['userapp', 'Your app'] };
const meter = (f, cls = '') => `<div class="meter ${cls}"><i style="width:${Math.max(0, Math.min(1, f)) * 100}%"></i></div>`;
const pageHead = (title, sub = '', actions = '') => `<div class="head"><div><h1>${esc(title)}</h1>${sub ? `<p>${sub}</p>` : ''}</div><div class="row">${actions}</div></div>`;
const mount = (html) => { view.innerHTML = html; initSort(); };

/* File actions shared by Storage and Leftovers. The engine only accepts paths from its own current results. */
const actBtns = (path) => `<span class="acts"><button class="iconbtn" data-reveal="${esc(path)}" title="Show in Explorer" aria-label="Show in Explorer">${ic('reveal', 'sm')}</button><button class="iconbtn" data-recycle="${esc(path)}" title="Move to Recycle Bin" aria-label="Move to Recycle Bin">${ic('trash', 'sm')}</button></span>`;
async function revealPath(p) { try { await invoke('path_reveal', { path: p }); } catch (e) { toast(String(e), 'bad'); } }
/* items: [{ path, bytes }]. Returns how many were moved. Recoverable from the Recycle Bin. */
async function recyclePaths(items) {
  const sum = items.reduce((a, i) => a + (i.bytes || 0), 0);
  const r = await confirmModal({
    title: `Move ${items.length} item${items.length > 1 ? 's' : ''} to the Recycle Bin`,
    intro: `${human(sum)} in total. You can restore them from the Recycle Bin. The space only comes back after you empty it.`,
    body: items.slice(0, 8).map((i) => `<div class="mi mono">${esc(i.path)}</div>`).join('') + (items.length > 8 ? `<div class="mi muted">and ${items.length - 8} more</div>` : ''),
    ok: 'Move to Recycle Bin', danger: true,
  });
  if (!r.ok) return [];
  const ok = [], errs = [];
  for (const i of items) { try { await invoke('path_recycle', { path: i.path }); ok.push(i.path); } catch (e) { errs.push(String(e)); } }
  if (ok.length) toast(`Moved ${ok.length} item${ok.length > 1 ? 's' : ''} to the Recycle Bin.`, 'ok');
  if (errs.length) toast(errs[0] + (errs.length > 1 ? ` (and ${errs.length - 1} more problems)` : ''), 'bad');
  return ok;
}

/* App icons: the engine extracts them from each program's .exe. Until one arrives (or if there is none) a letter tile shows. */
const APP_ICONS = {};
const iconPending = new Set();
function appIcon(g) {
  const u = g.exe && APP_ICONS[g.exe];
  return `<span class="appicon" data-exe="${esc(g.exe || '')}">${u ? `<img alt="" src="${u}">` : `<i>${esc((g.name || '?').charAt(0).toUpperCase())}</i>`}</span>`;
}
function loadIcons(groups) {
  const need = [...new Set(groups.map((g) => g.exe).filter((e) => e && APP_ICONS[e] === undefined && !iconPending.has(e)))].slice(0, 60);
  if (!need.length) return;
  need.forEach((e) => iconPending.add(e));
  invoke('process_icons', { paths: need })
    .then((map) => {
      need.forEach((e) => { APP_ICONS[e] = map[e] || false; iconPending.delete(e); });
      $$('.appicon[data-exe]').forEach((el) => { const u = APP_ICONS[el.dataset.exe]; if (u && !el.querySelector('img')) el.innerHTML = `<img alt="" src="${u}">`; });
    })
    .catch(() => need.forEach((e) => iconPending.delete(e)));
}

function ago(unix) {
  const s = Math.max(0, Date.now() / 1000 - unix);
  if (s < 60) return 'just now';
  if (s < 3600) return `${Math.floor(s / 60)} min ago`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ago`;
  return `${Math.floor(s / 86400)} d ago`;
}

/* Sortable tables: <table class="tbl" data-tk="key"> with <th data-s="n|s">. Cells may carry data-v for the sort value.
   The chosen column and direction are remembered per table key for the session. */
function initSort(root = view) {
  $$('table.tbl[data-tk]', root).forEach((t) => {
    const key = t.dataset.tk, ths = $$('thead th', t), tb = t.tBodies[0];
    if (!tb) return;
    const apply = () => {
      const st = S.sort[key];
      ths.forEach((th, i) => { if (th.dataset.s) th.setAttribute('aria-sort', st && st.col === i ? (st.dir === 'asc' ? 'ascending' : 'descending') : 'none'); });
      if (!st) return;
      const num = ths[st.col].dataset.s === 'n', m = st.dir === 'asc' ? 1 : -1;
      const val = (r) => { const td = r.cells[st.col]; const v = td?.dataset.v ?? td?.textContent.trim() ?? ''; return num ? parseFloat(v) || 0 : String(v).toLowerCase(); };
      const rows = [...tb.rows].map((r, idx) => ({ r, idx, v: val(r) }));
      rows.sort((a, b) => (a.v < b.v ? -m : a.v > b.v ? m : a.idx - b.idx));
      rows.forEach((x) => tb.appendChild(x.r));
    };
    ths.forEach((th) => { if (th.dataset.s) { th.classList.add('sortable'); th.tabIndex = 0; th.title = 'Click to sort'; } });
    const toggle = (th) => {
      const i = ths.indexOf(th), cur = S.sort[key];
      S.sort[key] = { col: i, dir: cur && cur.col === i ? (cur.dir === 'asc' ? 'desc' : 'asc') : (th.dataset.s === 'n' ? 'desc' : 'asc') };
      apply();
    };
    t.tHead.onclick = (e) => { const th = e.target.closest('th[data-s]'); if (th) toggle(th); };
    t.tHead.onkeydown = (e) => { if (e.key === 'Enter' || e.key === ' ') { const th = e.target.closest('th[data-s]'); if (th) { e.preventDefault(); toggle(th); } } };
    apply();
  });
}

/* sparkline: fixed 60-sample window, right-aligned; max is explicit so the scale is honest */
function spark(vals, { max = 100, grid = true } = {}) {
  const W = 240, H = 44, N = 60, step = W / (N - 1);
  const y = (v) => H - 2 - (Math.max(0, Math.min(v, max)) / max) * (H - 6);
  let body = grid ? `<line class="g" x1="0" x2="${W}" y1="${y(max)}" y2="${y(max)}"/><line class="g" x1="0" x2="${W}" y1="${y(max / 2)}" y2="${y(max / 2)}"/>` : '';
  if (vals.length > 1) {
    const pts = vals.map((v, i) => [(N - vals.length + i) * step, y(v)]);
    const line = 'M' + pts.map((p) => `${p[0].toFixed(1)} ${p[1].toFixed(1)}`).join(' L');
    body += `<path class="a" d="${line} L${pts.at(-1)[0].toFixed(1)} ${H} L${pts[0][0].toFixed(1)} ${H} Z"/><path class="l" vector-effect="non-scaling-stroke" d="${line}"/>`;
  }
  return `<svg class="spark" viewBox="0 0 ${W} ${H}" preserveAspectRatio="none" role="img" aria-label="Last 60 seconds">${body}</svg>`;
}

/* squarified treemap */
function squarify(items, rect) {
  const res = [];
  const list = items.filter((i) => i.v > 0).map((i) => ({ ...i }));
  const total = list.reduce((a, b) => a + b.v, 0);
  if (!total) return res;
  const scale = (rect.w * rect.h) / total;
  list.forEach((i) => (i.a = i.v * scale));
  let { x, y, w, h } = rect;
  const worst = (row, side) => {
    const s = row.reduce((a, i) => a + i.a, 0), mx = Math.max(...row.map((i) => i.a)), mn = Math.min(...row.map((i) => i.a));
    return Math.max((side * side * mx) / (s * s), (s * s) / (side * side * mn));
  };
  while (list.length) {
    const side = Math.min(w, h);
    const row = [list.shift()];
    while (list.length && worst([...row, list[0]], side) <= worst(row, side)) row.push(list.shift());
    const s = row.reduce((a, i) => a + i.a, 0);
    if (w >= h) { const rw = s / h; let cy = y; row.forEach((i) => { const ih = i.a / rw; res.push({ ...i, x, y: cy, w: rw, h: ih }); cy += ih; }); x += rw; w -= rw; }
    else { const rh = s / w; let cx = x; row.forEach((i) => { const iw = i.a / rh; res.push({ ...i, x: cx, y, w: iw, h: rh }); cx += iw; }); y += rh; h -= rh; }
  }
  return res;
}

/* ───────── live feed (1 s) ───────── */
function pushHist(k, v) { const a = S.hist[k]; a.push(v); if (a.length > 60) a.shift(); }
function onLive(p) {
  S.live = p;
  pushHist('cpu', p.cpu_total);
  pushHist('ram', (p.ram_used / p.ram_total) * 100);
  pushHist('rx', p.net_rx); pushHist('tx', p.net_tx); pushHist('disk', p.disk_read + p.disk_write);
  if (p.drives && S.boot) S.boot.drives = p.drives;
  if (!S.busy || route === 'overview') PAGES[route]?.live?.(p);
}

/* ───────── data loaders shared by pages ───────── */
const selectable = (r) => r.available && (S.boot.admin || !r.admin) && (r.special || r.bytes !== 0);
function reclaimable() {
  let user = 0, adminOnly = 0;
  for (const r of S.rules || []) {
    if (!r.available) continue;
    const b = r.bytes || 0;
    if (r.admin && !S.boot.admin) adminOnly += b; else user += b;
  }
  return { user, adminOnly };
}
async function scan() {
  if (S.scanning) return S.scanning;
  S.scanning = (async () => {
    try {
      S.rules = await invoke('scan_rules');
      if (!S.selInit) { S.sel = new Set(S.rules.filter((r) => r.default_on && selectable(r)).map((r) => r.id)); S.selInit = true; }
      else S.sel = new Set([...S.sel].filter((id) => selectable(S.rules.find((r) => r.id === id) || {})));
    } catch (e) { toast(`Scan failed: ${e}`, 'bad'); }
    S.scanning = null;
    if ((route === 'overview' || route === 'clean') && !S.showingResult) PAGES[route].render();
  })();
  return S.scanning;
}
async function loadDetails() { if (!S.details) { try { S.details = await invoke('system_details'); } catch {} } return S.details; }
async function loadHistory(force = false) { if (!S.history || force) { try { S.history = await invoke('history'); } catch { S.history = []; } } return S.history; }
async function loadStartup(force = false) { if (!S.startup || force) { try { S.startup = await invoke('startup_list'); } catch { S.startup = []; } } return S.startup; }

async function relaunchAdmin() {
  const r = await confirmModal({ title: 'Run as administrator', intro: 'Windows will ask for permission. This window closes and reopens with full access to system caches, logs and the component store.', ok: 'Continue' });
  if (!r.ok) return;
  if (!(await invoke('relaunch_admin'))) toast('Administrator access was not granted.', 'warn');
}

/* ───────── shell: nav, router ───────── */
function buildNav() {
  $('#nav').innerHTML = NAV.map(([g, items]) => `<div class="navgroup"><small>${g}</small><div class="nav">${items.map(([id, label]) => `<button data-route="${id}">${ic(id)}<span>${label}</span></button>`).join('')}</div></div>`).join('');
  $('#nav').addEventListener('click', (e) => {
    const b = e.target.closest('[data-route]');
    if (!b) return;
    if (S.busy) return toast('A job is running. Wait for it to finish.', 'warn');
    if (location.hash.slice(1) === b.dataset.route) setRoute(); else location.hash = b.dataset.route;
  });
  $('#themeseg').addEventListener('click', (e) => { const b = e.target.closest('[data-theme]'); if (b) setTheme(b.dataset.theme); });
  renderThemeSeg();
}
function renderRail() {
  const a = S.boot?.admin;
  $('#priv').innerHTML = `<span class="dot ${a ? 'ok' : 'warn'}"></span><span>${a ? 'Administrator' : 'Standard user'}</span>${a ? '' : '<button class="link" id="go-admin" style="margin-left:auto">Elevate</button>'}`;
  const b = $('#go-admin'); if (b) b.addEventListener('click', relaunchAdmin);
  if (S.boot) $('#ver').textContent = `v${S.boot.version}`;
}
function setRoute() {
  S.showingResult = false;
  PAGES[route]?.leave?.();
  route = (location.hash || '#overview').slice(1);
  if (!PAGES[route]) route = 'overview';
  $$('#nav [data-route]').forEach((b) => b.classList.toggle('on', b.dataset.route === route));
  render();
}
function render() { $('#main').scrollTop = 0; view.onclick = view.onchange = view.oninput = view.onkeydown = null; PAGES[route].render(); }
const go = (r) => { if (S.busy) return; if (location.hash.slice(1) === r) setRoute(); else location.hash = r; };
