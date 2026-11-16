'use strict';
/* Boost (live process table) and Startup. The process list updates every second from the engine's sampler. */

const B = { tab: 'all', q: '', sort: 'mem', dir: 'desc', memb: '', tick: 0, hold: false, seeded: false, open: new Set() };
const CLASS_ORDER = { Helper: 0, UserApp: 1, Protected: 2 };
const BCOLS = [['name', 'Process', ''], ['class', 'Type', ''], ['cpu', 'CPU', 'r'], ['mem', 'Memory', ''], ['disk', 'Disk', 'r hmd'], ['reason', 'Reason', '']];
const bHead = () => `<tr><th style="width:44px"><label class="chk"><input type="checkbox" id="b-all" aria-label="Select all visible programs"><i></i></label></th>${BCOLS.map(([k, l, c]) => `<th class="sortable ${c}" data-b="${k}" aria-sort="${B.sort === k ? (B.dir === 'asc' ? 'ascending' : 'descending') : 'none'}" tabindex="0" title="Click to sort">${l}</th>`).join('')}</tr>`;
const bkey = (g) => g.name.toLowerCase();

function boostGroups(p) {
  let g = p.groups.slice();
  if (B.tab === 'helper') g = g.filter((x) => x.class === 'Helper');
  else if (B.tab === 'userapp') g = g.filter((x) => x.class === 'UserApp');
  else if (B.tab === 'protected') g = g.filter((x) => x.class === 'Protected');
  if (B.q) g = g.filter((x) => x.name.toLowerCase().includes(B.q.toLowerCase()));
  const cmp = {
    mem: (a, b) => a.mem - b.mem, cpu: (a, b) => a.cpu - b.cpu, disk: (a, b) => a.disk - b.disk,
    name: (a, b) => a.name.localeCompare(b.name), reason: (a, b) => a.reason.localeCompare(b.reason),
    class: (a, b) => CLASS_ORDER[a.class] - CLASS_ORDER[b.class] || b.mem - a.mem,
  }[B.sort];
  const m = B.dir === 'asc' ? 1 : -1;
  return g.sort((a, b) => cmp(a, b) * m).slice(0, 80);
}

function boostRow(g, maxMem) {
  const k = bkey(g), prot = g.class === 'Protected';
  return `<tr data-k="${esc(k)}" class="${prot ? 'dim' : ''}"><td style="width:44px">${prot ? '' : `<label class="chk"><input type="checkbox" data-p="${esc(k)}" ${S.psel.has(k) ? 'checked' : ''} aria-label="Select ${esc(g.name)}"><i></i></label>`}</td>
    <td class="proc"><div class="pcell">${appIcon(g)}<div class="ptext">${g.members.length > 1 ? `<button class="rowtoggle ${B.open.has(k) ? 'open' : ''}" data-tg="${esc(k)}" aria-expanded="${B.open.has(k)}" title="Show the ${g.members.length} processes">${ic('chevron', 'sm')}<b>${esc(g.name)}</b> <span class="faint">&times;${g.members.length}</span></button>` : `<b>${esc(g.name)}</b>`}<div class="sub mono ellip" title="${esc(g.exe)}">${esc(g.exe || '')}</div></div></div></td>
    <td>${mark(...classLabel[g.class])}</td>
    <td class="r num c-cpu">${g.cpu.toFixed(1)}%</td>
    <td class="c-mem" style="min-width:150px"><div class="num">${human(g.mem)}</div>${meter(g.mem / maxMem)}</td>
    <td class="r num c-disk hmd">${g.disk ? rate(g.disk) : '<span class="faint">&mdash;</span>'}</td>
    <td class="reason" title="${esc(g.reason)}">${esc(g.reason)}</td></tr>`;
}

const membersHtml = (g) => `<div class="mlist">${g.members.slice(0, 48).map((m) => `<span>PID ${m.pid} &nbsp;<b class="num">${human(m.mem)}</b> <span class="faint num">${m.cpu.toFixed(1)}%</span></span>`).join('')}${g.members.length > 48 ? `<span class="faint">and ${g.members.length - 48} more</span>` : ''}</div>`;
const memberRow = (g) => `<tr class="members" data-mk="${esc(bkey(g))}"><td></td><td colspan="6">${membersHtml(g)}</td></tr>`;

function boostSelCount() {
  const n = $('#b-n'); if (n) n.textContent = S.psel.size;
  const k = $('#b-kill'); if (k) k.disabled = S.psel.size === 0;
  const all = $('#b-all');
  if (all && S.live) {
    const keys = boostGroups(S.live).filter((g) => g.class !== 'Protected').map(bkey), c = keys.filter((x) => S.psel.has(x)).length;
    all.checked = keys.length > 0 && c === keys.length;
    all.indeterminate = c > 0 && c < keys.length;
    all.disabled = keys.length === 0;
  }
}

function boostStats(p) {
  const set = (id, h) => { const el = document.getElementById(id); if (el) el.innerHTML = h; };
  const helpers = p.groups.filter((g) => g.class === 'Helper');
  const bucket = (g) => (g.class === 'Helper' ? 'helper' : g.class === 'UserApp' ? 'user' : g.reason.startsWith('Shared web') ? 'web' : g.reason.startsWith('Runs this session') ? 'self' : g.reason === 'Security engine' ? 'sec' : 'win');
  const defs = [['win', 'Windows and drivers', 'c3'], ['web', 'Web runtime (WebView2)', 'c4'], ['self', 'Terminal, Claude, this app', 'c5'], ['sec', 'Security software', 'c6'], ['helper', 'Vendor helpers', 'c2'], ['user', 'Your apps', 'c0']];
  const sums = { win: 0, web: 0, self: 0, sec: 0, helper: 0, user: 0 };
  p.groups.forEach((g) => (sums[bucket(g)] += g.mem));
  const owned = Object.values(sums).reduce((a, b) => a + b, 0);
  const other = Math.max(0, p.ram_used - owned), den = Math.max(p.ram_used, owned) || 1;
  set('b-proc-v', `${p.proc_count}<small>processes</small>`);
  set('b-proc-s', `${p.groups.length} programs, ${helpers.length} vendor helpers`);
  set('b-proc-g', `<div class="stack">${defs.map(([k, , c]) => `<i class="${c}" style="width:${(sums[k] / den) * 100}%"></i>`).join('')}<i class="cx" style="width:${(other / den) * 100}%"></i></div>
    <div class="legend" style="margin-top:10px;gap:4px;font-size:12.5px">${defs.map(([k, l, c]) => `<div><span class="sw8 ${c}"></span><span class="n">${l}</span><span class="num">${human(sums[k])}</span></div>`).join('')}<div><span class="sw8 cx"></span><span class="n" title="Kernel, drivers and file cache belong to no process">Kernel, drivers, cache</span><span class="num">${human(other)}</span></div></div>`);
  set('b-ram-v', human(p.ram_used));
  set('b-ram-s', `of ${human(p.ram_total)} &middot; ${Math.round((p.ram_used / p.ram_total) * 100)}% in use`);
  set('b-ram-g', spark(S.hist.ram, { max: 100 }));
  set('b-cpu-v', `${p.cpu_total.toFixed(0)}<small class="pct">%</small>`);
  set('b-cpu-s', `${p.cpu_cores.length} threads`);
  set('b-cpu-g', spark(S.hist.cpu, { max: 100 }));
}

function boostTable(p) {
  const rows = boostGroups(p), maxMem = Math.max(1, ...rows.map((g) => g.mem));
  const body = $('#b-body'); if (!body) return;
  body.innerHTML = rows.length ? rows.map((g) => boostRow(g, maxMem) + (B.open.has(bkey(g)) ? memberRow(g) : '')).join('') : `<tr><td colspan="7"><div class="empty">No processes match.</div></td></tr>`;
  B.memb = rows.map(bkey).sort().join('|');
  boostSelCount();
  loadIcons(rows);
}

function boostPatch(p) {
  const rows = boostGroups(p), maxMem = Math.max(1, ...rows.map((g) => g.mem));
  for (const g of rows) {
    const tr = document.querySelector(`#b-body tr[data-k="${CSS.escape(bkey(g))}"]`); if (!tr) continue;
    tr.querySelector('.c-cpu').textContent = `${g.cpu.toFixed(1)}%`;
    tr.querySelector('.c-mem').innerHTML = `<div class="num">${human(g.mem)}</div>${meter(g.mem / maxMem)}`;
    tr.querySelector('.c-disk').innerHTML = g.disk ? rate(g.disk) : '<span class="faint">&mdash;</span>';
    if (B.open.has(bkey(g))) { const m = document.querySelector(`#b-body tr.members[data-mk="${CSS.escape(bkey(g))}"] td:last-child`); if (m) m.innerHTML = membersHtml(g); }
  }
}

PAGES.boost = {
  render() {
    const counts = { all: 0, helper: 0, userapp: 0, protected: 0 };
    (S.live?.groups || []).forEach((g) => { counts.all++; counts[g.class === 'Helper' ? 'helper' : g.class === 'UserApp' ? 'userapp' : 'protected']++; });
    mount(`${pageHead('Boost', 'Live view of everything running, updated every second. Windows core, drivers, the security engine and your terminal are protected and cannot be closed from here.',
      `<button class="btn" id="b-trim">Trim memory</button><button class="btn primary" id="b-kill" disabled><span>Close selected</span><span class="count" id="b-n">0</span></button>`)}
      <div class="grid" style="grid-template-columns:repeat(3,minmax(0,1fr))">
        <div class="card tile"><div class="lab">Where memory goes</div><div class="big num" id="b-proc-v">&mdash;</div><div class="sub" id="b-proc-s">&nbsp;</div><div class="spark-box" id="b-proc-g"></div></div>
        <div class="card tile"><div class="lab">Memory</div><div class="big num" id="b-ram-v">&mdash;</div><div class="sub" id="b-ram-s">&nbsp;</div><div class="spark-box" id="b-ram-g"></div><div class="spark-cap"><span>60 s ago</span><span>0 to 100%</span></div></div>
        <div class="card tile"><div class="lab">Processor</div><div class="big num" id="b-cpu-v">&mdash;</div><div class="sub" id="b-cpu-s">&nbsp;</div><div class="spark-box" id="b-cpu-g"></div><div class="spark-cap"><span>60 s ago</span><span>0 to 100%</span></div></div></div>
      <div class="toolbar" style="margin-top:24px"><div class="seg" id="b-tabs">${[['all', 'All'], ['helper', 'Helpers'], ['userapp', 'Your apps'], ['protected', 'Protected']].map(([k, l]) => `<button data-t="${k}" class="${B.tab === k ? 'on' : ''}">${l} <span class="faint">${counts[k] || ''}</span></button>`).join('')}</div>
        <span class="search">${ic('search', 'sm')}<input class="input" id="b-q" placeholder="Search processes" value="${esc(B.q)}" autocomplete="off"></span>
        <span class="grow"></span><span class="faint">Click a column header to sort</span></div>
      <div class="card scroll-x"><table class="tbl"><thead>${bHead()}</thead><tbody id="b-body"><tr><td colspan="7">${loading('Waiting for the first sample')}</td></tr></tbody></table></div>
      <p class="faint" style="margin-top:12px;font-size:12.5px;max-width:90ch">Memory is each program's working set, which includes pages shared with other programs, so a group like svchost or WebView2 looks larger than the private memory it owns (Task Manager's default column is private). Windows also keeps RAM that belongs to no process: the kernel, drivers and the file cache, shown above as Kernel, drivers, cache.</p>
      <div class="section-title">Vendor services</div><div class="card" id="b-svc">${loading('Reading services')}</div>`);
    view.onclick = (e) => {
      const t = e.target.closest('#b-tabs [data-t]');
      if (t) { B.tab = t.dataset.t; $$('#b-tabs button').forEach((b) => b.classList.toggle('on', b === t)); if (S.live) boostTable(S.live); return; }
      const hd = e.target.closest('th[data-b]');
      if (hd) {
        const k = hd.dataset.b;
        B.dir = B.sort === k ? (B.dir === 'asc' ? 'desc' : 'asc') : (['name', 'class', 'reason'].includes(k) ? 'asc' : 'desc');
        B.sort = k;
        $('thead', view.querySelector('#b-body').closest('table')).innerHTML = bHead();
        if (S.live) boostTable(S.live);
        return;
      }
      const tg = e.target.closest('[data-tg]');
      if (tg) { const k = tg.dataset.tg; B.open.has(k) ? B.open.delete(k) : B.open.add(k); if (S.live) boostTable(S.live); return; }
      if (e.target.closest('#b-kill')) return killSelected();
      if (e.target.closest('#b-trim')) return trimMemory();
      const sv = e.target.closest('[data-svc]'); if (sv) stopService(S.svcs[+sv.dataset.svc]);
    };
    view.oninput = (e) => { if (e.target.id === 'b-q') { B.q = e.target.value; if (S.live) boostTable(S.live); } };
    view.onchange = (e) => {
      if (e.target.id === 'b-all') {
        const keys = boostGroups(S.live).filter((g) => g.class !== 'Protected').map(bkey);
        keys.forEach((k) => (e.target.checked ? S.psel.add(k) : S.psel.delete(k)));
        $$('#b-body input[data-p]').forEach((i) => (i.checked = S.psel.has(i.dataset.p)));
        return boostSelCount();
      }
      const cb = e.target.closest('input[data-p]'); if (!cb) return;
      cb.checked ? S.psel.add(cb.dataset.p) : S.psel.delete(cb.dataset.p);
      boostSelCount();
    };
    view.onkeydown = (e) => { if ((e.key === 'Enter' || e.key === ' ') && e.target.matches('th[data-b]')) { e.preventDefault(); e.target.click(); } };
    view.onmousedown = () => { B.hold = true; };
    window.onmouseup = () => { B.hold = false; };
    if (S.live) this.live(S.live);
    this.loadSvcs();
  },
  async loadSvcs() {
    try { S.svcs = await invoke('services_list'); } catch { S.svcs = []; }
    const el = $('#b-svc'); if (!el || route !== 'boost') return;
    const admin = S.boot.admin;
    el.innerHTML = S.svcs.length ? `<table class="tbl"><tbody>${S.svcs.map((s, i) => `<tr><td><b>${esc(s.label)}</b><div class="sub">${esc(s.why)}</div></td><td>${s.running ? mark('warn', 'Running') : mark('protected', 'Stopped')}</td>
      <td class="r">${s.running ? `<button class="btn sm" data-svc="${i}" ${admin ? '' : 'disabled'}>${admin ? 'Stop and set to Manual' : 'Needs administrator'}</button>` : ''}</td></tr>`).join('')}</tbody></table>
      <div class="pad faint" style="font-size:12.5px;padding-top:0">These services restart on their own if you only end the process. Setting them to Manual is reversible.</div>` : '<div class="empty">No known vendor services found.</div>';
  },
  live(p) {
    if (!S.psel.size && !B.seeded && p.groups.length) {
      S.psel = new Set(p.groups.filter((g) => g.class === 'Helper' && g.mem >= 20 * 1024 * 1024).map(bkey));
      B.seeded = true;
    }
    boostStats(p);
    const body = $('#b-body'); if (!body) return;
    const interacting = B.hold || document.activeElement?.closest?.('#b-body');
    const memb = boostGroups(p).map(bkey).sort().join('|');
    B.tick++;
    if (!body.children.length || body.querySelector('.center') || ((memb !== B.memb || B.tick % 5 === 0) && !interacting)) boostTable(p);
    else boostPatch(p);
    boostSelCount();
  },
  leave() { window.onmouseup = null; },
};

async function killSelected() {
  const p = S.live; if (!p) return;
  const chosen = p.groups.filter((g) => g.class !== 'Protected' && S.psel.has(bkey(g)));
  if (!chosen.length) return toast('Nothing selected.', 'warn');
  const mine = chosen.filter((g) => g.class === 'UserApp');
  const r = await confirmModal({
    title: `Close ${chosen.length} program${chosen.length > 1 ? 's' : ''}`,
    intro: mine.length ? 'You selected your own apps. Unsaved work in them will be lost.' : 'These are background helpers. Some start again at the next sign-in.',
    body: chosen.map((g) => `<div class="mi"><div class="row" style="gap:12px"><b>${esc(g.name)}</b>${g.pids.length > 1 ? `<span class="faint">&times;${g.pids.length}</span>` : ''}${mark(...classLabel[g.class])}<span class="grow"></span><span class="num muted">${human(g.mem)}</span></div><small>${esc(g.reason)}</small></div>`).join(''),
    ok: 'Close', danger: mine.length > 0,
  });
  if (!r.ok) return;
  const before = p.ram_used;
  const res = await invoke('kill_pids', { pids: chosen.flatMap((g) => g.pids) });
  S.psel = new Set([...S.psel].filter((k) => !chosen.some((g) => bkey(g) === k)));
  setTimeout(() => { const after = S.live ? S.live.ram_used : before; toast(`Closed ${res.killed} of ${res.requested} processes. Memory in use changed by ${human(Math.abs(before - after))} ${after <= before ? 'lower' : 'higher'}.`, res.killed ? 'ok' : 'warn'); }, 1600);
  if (res.killed < res.requested) toast(`${res.requested - res.killed} could not be closed. Windows services need administrator rights.`, 'warn');
}

async function trimMemory() {
  toast('Trimming memory');
  const t = await invoke('trim_memory');
  toast(`Trimmed ${t.trimmed} processes. In use: ${human(t.used_before)} to ${human(t.used_after)}. Windows refills it with cache as you work.`, 'ok');
}

async function stopService(s) {
  try {
    const undo = await invoke('service_manual', { name: s.name });
    await confirmModal({ title: `${s.label} stopped`, intro: 'Start type is now Manual. To undo, run this in an elevated terminal:', body: `<div class="mi mono">${esc(undo)}</div>`, ok: 'Close', cancel: null });
    PAGES.boost.loadSvcs();
  } catch (e) { toast(`${s.label}: ${e}`, 'bad'); }
}

/* ───────── startup ───────── */
PAGES.startup = {
  async render() {
    mount(loading('Reading startup entries'));
    const list = await loadStartup(true);
    if (route !== 'startup') return;
    const admin = S.boot.admin, on = list.filter((e) => e.enabled).length, flagged = list.filter((e) => e.enabled && e.advice).length;
    mount(`${pageHead('Startup', 'Programs that start with Windows. Switching one off is reversible and uses the same flag as Task Manager. Nothing is deleted.')}
      <div class="row" style="margin-bottom:18px;gap:28px"><span><b class="num">${on}</b> <span class="muted">of ${list.length} enabled</span></span><span><b class="num">${flagged}</b> <span class="muted">flagged as unnecessary</span></span></div>
      <div class="card scroll-x"><table class="tbl" data-tk="startup"><thead><tr><th data-s="s">Name</th><th data-s="s">Source</th><th data-s="s">Note</th><th class="r" data-s="n">Starts with Windows</th></tr></thead><tbody>
      ${list.map((e, i) => `<tr><td data-v="${esc(e.name)}"><b>${esc(e.name)}</b><div class="sub mono ellip" title="${esc(e.command)}">${esc(e.command)}</div></td><td class="muted num">${esc(e.scope)}</td>
        <td data-v="${esc(e.advice || '')}">${e.advice ? mark('warn', e.advice) : ''}</td>
        <td class="r" data-v="${e.enabled ? 1 : 0}"><label class="sw"><input type="checkbox" data-s="${i}" ${e.enabled ? 'checked' : ''} ${e.needs_admin && !admin ? 'disabled' : ''} aria-label="Start ${esc(e.name)} with Windows"><i></i></label>${e.needs_admin && !admin ? '<div class="sub">Needs administrator</div>' : ''}</td></tr>`).join('')}</tbody></table></div>`);
    view.onchange = async (ev) => {
      const cb = ev.target.closest('input[data-s]'); if (!cb) return;
      const e = S.startup[+cb.dataset.s], want = cb.checked;
      try { await invoke('startup_set', { name: e.name, scope: e.scope, enabled: want }); e.enabled = want; cb.closest('td').dataset.v = want ? 1 : 0; toast(`${e.name} will ${want ? '' : 'not '}start with Windows.`, 'ok'); }
      catch (err) { cb.checked = !want; toast(`${e.name}: ${err}`, 'bad'); }
    };
  },
};
