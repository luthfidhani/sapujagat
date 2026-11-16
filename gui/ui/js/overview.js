'use strict';
/* Overview: live vitals, storage, what can be reclaimed, what needs attention. */

const CAT_ORDER = ['system', 'gpu', 'browser', 'dev', 'docker', 'apps', 'privacy'];

function ovSubline() {
  const d = S.details, b = S.boot;
  const host = d?.host || b?.sys?.host || '';
  const os = d ? `${d.os_name} ${d.os_display_version}`.trim() : '';
  const up = S.live ? `up ${dur(S.live.uptime)}` : '';
  return [host, os, up].filter(Boolean).map(esc).join(' &nbsp;·&nbsp; ');
}

function ovTile(label, id) {
  return `<div class="card tile"><div class="lab">${label}</div><div class="big num" id="${id}-v">&mdash;</div><div class="sub" id="${id}-s">&nbsp;</div><div class="spark-box" id="${id}-g"></div><div class="spark-cap"><span>60 s ago</span><span id="${id}-c"></span></div></div>`;
}

function ovDrives() {
  const ds = S.boot.drives || [];
  const rows = ds.map((d, i) => {
    const used = d.total - d.free, f = used / d.total;
    return `<div class="drive"><div><b>${esc(d.root)}</b>${i === 0 ? ' <span class="faint">&nbsp;system</span>' : ''}</div><div class="num muted">${human(d.free)} free of ${human(d.total)}</div>${meter(f, `lg ${tone(f)}`)}<div class="faint" style="grid-column:1/-1;font-size:12.5px">${human(used)} used &middot; ${Math.round(f * 100)}%</div></div>`;
  }).join('');
  const tot = ds.reduce((a, d) => a + d.total, 0), free = ds.reduce((a, d) => a + d.free, 0);
  return rows + (ds.length > 1 ? `<div class="drive" style="margin-top:auto;background:var(--surface-2);border-radius:0 0 var(--r-card) var(--r-card)"><div class="muted">All drives</div><div class="num muted">${human(tot - free)} used of ${human(tot)}</div></div>` : '');
}

function ovReclaim() {
  if (!S.rules) return `<div class="pad">${loading('Scanning 40+ locations')}</div>`;
  const { user, adminOnly } = reclaimable();
  const by = {};
  for (const r of S.rules) {
    if (!r.available || (r.admin && !S.boot.admin)) continue;
    by[r.cat] = by[r.cat] || { title: r.cat_title, bytes: 0 };
    by[r.cat].bytes += r.bytes || 0;
  }
  const cats = CAT_ORDER.filter((c) => by[c] && by[c].bytes > 0).sort((a, b) => by[b].bytes - by[a].bytes);
  const total = cats.reduce((a, c) => a + by[c].bytes, 0) || 1;
  return `<div class="pad">
    <div class="lab muted" style="font-size:13px">Can be reclaimed now</div>
    <div class="big" style="margin:6px 0 14px">${human(user)}</div>
    ${cats.length ? `<div class="stack">${cats.map((c, i) => `<i class="c${i}" style="width:${(by[c].bytes / total) * 100}%"></i>`).join('')}</div>
    <div class="legend">${cats.map((c, i) => `<div><span class="sw8 c${i}"></span><span class="n">${esc(by[c].title)}</span><span class="num">${human(by[c].bytes)}</span></div>`).join('')}</div>` : '<div class="muted">Nothing to clean.</div>'}
    ${!S.boot.admin && adminOnly > 0 ? `<div class="muted" style="margin-top:14px;font-size:13px">${ic('lock', 'sm')} ${human(adminOnly)} more is available to an administrator.</div>` : ''}
    <div class="row" style="margin-top:18px"><button class="btn primary" data-go="clean">Review and clean</button></div></div>`;
}

function ovAttention() {
  const items = [];
  for (const d of S.boot.drives || []) {
    const f = 1 - d.free / d.total;
    if (f >= 0.85) items.push({ sev: f >= 0.93 ? 'bad' : 'warn', t: `${d.root} is ${Math.round(f * 100)}% full`, s: `${human(d.free)} free of ${human(d.total)}`, a: 'Open Storage', go: 'storage' });
  }
  if (S.rules && !S.boot.admin) {
    const { adminOnly } = reclaimable();
    if (adminOnly > 50 * 1024 * 1024) items.push({ sev: 'info', t: `${human(adminOnly)} can only be cleaned as administrator`, s: 'Windows temp, logs, update cache, crash dumps', a: 'Elevate', fn: 'admin' });
  }
  if (S.live) {
    const helpers = S.live.groups.filter((g) => g.class === 'Helper' && g.mem >= 20 * 1024 * 1024);
    if (helpers.length) items.push({ sev: 'info', t: `${helpers.length} vendor helper${helpers.length > 1 ? 's' : ''} running`, s: `${human(helpers.reduce((a, g) => a + g.mem, 0))} of memory`, a: 'Open Boost', go: 'boost' });
    const rf = S.live.ram_used / S.live.ram_total;
    if (rf > 0.85) items.push({ sev: 'warn', t: `Memory is ${Math.round(rf * 100)}% in use`, s: `${human(S.live.ram_used)} of ${human(S.live.ram_total)}`, a: 'Open Boost', go: 'boost' });
  }
  if (S.startup) {
    const n = S.startup.filter((e) => e.enabled && e.advice).length;
    if (n) items.push({ sev: 'info', t: `${n} startup item${n > 1 ? 's' : ''} can be switched off`, s: 'Vendor helpers that launch with Windows', a: 'Open Startup', go: 'startup' });
  }
  if (S.history) {
    const last = S.history[0];
    const days = last ? (Date.now() - new Date(last.when.replace(' ', 'T')).getTime()) / 86400000 : null;
    if (days === null) items.push({ sev: 'info', t: 'No cleaning has been recorded yet', s: 'Run a first clean to see results in History', a: 'Open Clean', go: 'clean' });
    else if (days > 14) items.push({ sev: 'info', t: `Last cleaned ${Math.floor(days)} days ago`, s: last.when, a: 'Open Clean', go: 'clean' });
  }
  if (!items.length) return `<div class="empty">Nothing needs attention.</div>`;
  return `<div class="list">${items.map((i) => `<div class="item"><span class="dot ${i.sev === 'info' ? '' : i.sev}"></span><div class="t"><b>${esc(i.t)}</b><small>${esc(i.s)}</small></div>
    <button class="link" ${i.go ? `data-go="${i.go}"` : `data-fn="${i.fn}"`}>${esc(i.a)}</button></div>`).join('')}</div>`;
}

function ovRunning(p) {
  const st = S.ovrun || (S.ovrun = { k: 'mem', dir: 'desc' });
  const cmp = { name: (a, b) => a.name.localeCompare(b.name), cpu: (a, b) => a.cpu - b.cpu, mem: (a, b) => a.mem - b.mem }[st.k];
  const m = st.dir === 'asc' ? 1 : -1;
  const top = p.groups.slice().sort((a, b) => cmp(a, b) * m).slice(0, 7);
  const th = (k, l, c = '') => `<th class="sortable ${c}" data-os="${k}" aria-sort="${st.k === k ? (st.dir === 'asc' ? 'ascending' : 'descending') : 'none'}" tabindex="0" title="Click to sort">${l}</th>`;
  return `<table class="tbl" style="table-layout:fixed"><colgroup><col><col style="width:72px"><col style="width:100px"></colgroup><thead><tr>${th('name', 'Process')}${th('cpu', 'CPU', 'r')}${th('mem', 'Memory', 'r')}</tr></thead><tbody>
    ${top.map((g) => `<tr><td style="overflow:hidden"><div class="row" style="gap:0;flex-wrap:nowrap">${appIcon(g)}<span class="dot ${g.class === 'Helper' ? 'ok' : g.class === 'UserApp' ? 'warn' : ''}" title="${esc(classLabel[g.class][1])}" style="margin-right:8px"></span><b class="ellip" style="max-width:none">${esc(g.name.replace(/\.exe$/i, ''))}</b>${g.pids.length > 1 ? `<span class="faint">&times;${g.pids.length}</span>` : ''}</div></td><td class="r num">${g.cpu.toFixed(1)}%</td><td class="r num">${human(g.mem)}</td></tr>`).join('')}</tbody></table>
    <div class="pad faint" style="padding-top:0;font-size:12.5px"><span class="dot ok"></span> helper &nbsp; <span class="dot warn"></span> your app &nbsp; <span class="dot"></span> protected</div>`;
}

PAGES.overview = {
  tick: 0,
  render() {
    mount(`${pageHead('Overview', `<span id="ov-sub">${ovSubline()}</span>`, `<button class="btn" id="ov-rescan">${ic('refresh', 'sm')}Rescan</button>`)}
      <div class="grid tiles">${ovTile('Processor', 'ov-cpu')}${ovTile('Memory', 'ov-ram')}${ovTile('Network', 'ov-net')}${ovTile('Disk activity', 'ov-disk')}</div>
      <div class="grid two"><div class="card" style="display:flex;flex-direction:column"><div class="card-h"><h2>Storage</h2><button class="link" data-go="storage">Analyze</button></div><div id="ov-drives" style="margin-top:8px;display:flex;flex-direction:column;flex:1">${ovDrives()}</div></div>
        <div class="card" id="ov-reclaim">${ovReclaim()}</div></div>
      <div class="grid two"><div class="card"><div class="card-h"><h2>Needs attention</h2></div><div id="ov-attn" style="margin-top:8px">${ovAttention()}</div></div>
        <div class="card"><div class="card-h"><h2>Running now</h2><button class="link" data-go="boost">Open Boost</button></div><div id="ov-run" style="margin-top:8px">${S.live ? ovRunning(S.live) : loading('Reading processes')}</div></div></div>`);
    view.onclick = (e) => {
      const hd = e.target.closest('th[data-os]');
      if (hd) { const k = hd.dataset.os, st = S.ovrun || (S.ovrun = { k: 'mem', dir: 'desc' }); st.dir = st.k === k ? (st.dir === 'asc' ? 'desc' : 'asc') : (k === 'name' ? 'asc' : 'desc'); st.k = k; if (S.live) $('#ov-run').innerHTML = ovRunning(S.live); return; }
      const g = e.target.closest('[data-go]'); if (g) return go(g.dataset.go);
      if (e.target.closest('[data-fn="admin"]')) return relaunchAdmin();
      if (e.target.closest('#ov-rescan')) { S.rules = null; $('#ov-reclaim').innerHTML = ovReclaim(); scan(); }
    };
    if (S.live) this.live(S.live);
    loadDetails().then(() => { const el = $('#ov-sub'); if (el && route === 'overview') el.innerHTML = ovSubline(); });
    Promise.all([loadHistory(), loadStartup()]).then(() => { const el = $('#ov-attn'); if (el && route === 'overview') el.innerHTML = ovAttention(); });
    if (!S.rules) scan();
  },
  live(p) {
    const set = (id, html) => { const el = document.getElementById(id); if (el) el.innerHTML = html; };
    const ramF = p.ram_used / p.ram_total;
    set('ov-cpu-v', `${p.cpu_total.toFixed(0)}<small class="pct">%</small>`);
    set('ov-cpu-s', `${p.cpu_cores.length} threads &middot; ${p.proc_count} processes`);
    set('ov-cpu-g', spark(S.hist.cpu, { max: 100 })); set('ov-cpu-c', '0 to 100%');
    set('ov-ram-v', human(p.ram_used));
    set('ov-ram-s', `of ${human(p.ram_total)} &middot; ${Math.round(ramF * 100)}% in use`);
    set('ov-ram-g', spark(S.hist.ram, { max: 100 })); set('ov-ram-c', '0 to 100%');
    const rxMax = Math.max(1048576, ...S.hist.rx), dMax = Math.max(1048576, ...S.hist.disk);
    set('ov-net-v', `${ic('down', 'sm')} ${rate(p.net_rx)}`);
    set('ov-net-s', `${ic('up', 'sm')} ${rate(p.net_tx)} upload`);
    set('ov-net-g', spark(S.hist.rx, { max: rxMax })); set('ov-net-c', `max ${rate(rxMax)}`);
    set('ov-disk-v', rate(p.disk_read + p.disk_write));
    set('ov-disk-s', `read ${rate(p.disk_read)} &middot; write ${rate(p.disk_write)}`);
    set('ov-disk-g', spark(S.hist.disk, { max: dMax })); set('ov-disk-c', `max ${rate(dMax)}`);
    set('ov-sub', ovSubline());
    set('ov-run', ovRunning(p));
    loadIcons(p.groups.slice(0, 30));
    if (p.drives) { set('ov-drives', ovDrives()); set('ov-attn', ovAttention()); }
  },
};
