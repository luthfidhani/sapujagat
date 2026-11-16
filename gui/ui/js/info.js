'use strict';
/* System (specs + live), History (past cleans), Catalog (everything the engine can clean). */

const dlr = (label, html, id = '') => `<div><dt>${esc(label)}</dt><dd ${id ? `id="${id}"` : ''}>${html}</dd></div>`;

function sysBattery(b) {
  if (!b || !b.present) return `No battery${b && b.ac != null ? ` &middot; ${b.ac ? 'on AC power' : 'on battery'}` : ''}`;
  const state = b.charging ? 'charging' : b.ac ? 'plugged in' : 'on battery';
  const left = !b.ac && b.minutes_left != null ? ` &middot; ${dur(b.minutes_left * 60)} left` : '';
  return `${b.percent ?? '?'}% &middot; ${state}${left}`;
}

PAGES.system = {
  async render() {
    mount(loading('Reading system details'));
    const d = await loadDetails();
    if (route !== 'system') return;
    if (!d) { mount(pageHead('System') + '<div class="card empty">System details are not available.</div>'); return; }
    const gpus = d.gpus.length ? d.gpus.map((g) => dlr('Adapter', `${esc(g.name)}${g.vram ? ` <span class="muted">&middot; ${human(g.vram)}</span>` : ''}`)).join('') : dlr('Adapter', 'Not reported');
    mount(`${pageHead('System', 'Hardware and Windows details for this PC. Values marked live update every second.')}
      <div class="specs">
        <section class="card"><div class="spec-h">Device</div><dl class="dl">${dlr('Name', esc(d.host))}${dlr('Windows', esc(`${d.os_name} ${d.os_display_version}`.trim()))}${dlr('Build', esc(d.os_build))}${dlr('Architecture', esc(d.arch))}${dlr('Uptime', '&mdash;', 'sy-up')}</dl></section>
        <section class="card"><div class="spec-h">Processor</div><dl class="dl">${dlr('Model', esc(d.cpu_brand))}${dlr('Cores', `${d.physical_cores ?? '?'} physical &middot; ${d.logical_cores} logical`)}${dlr('Base clock', `${(d.cpu_mhz / 1000).toFixed(2)} GHz`)}${dlr('Usage (live)', '&mdash;', 'sy-cpu')}</dl>
          <div class="cores" id="sy-cores">${Array.from({ length: d.logical_cores }, (_, i) => `<div class="core"><div class="col"><i style="height:0%"></i></div>${i + 1}</div>`).join('')}</div></section>
        <section class="card"><div class="spec-h">Memory</div><dl class="dl">${dlr('Installed', human(d.ram_total))}${dlr('In use (live)', '&mdash;', 'sy-ram')}${dlr('Available (live)', '&mdash;', 'sy-avail')}${dlr('Swap (live)', '&mdash;', 'sy-swap')}</dl></section>
        <section class="card"><div class="spec-h">Graphics and power</div><dl class="dl">${gpus}${dlr('Power plan', esc(d.power_plan || 'Unknown'))}${dlr('Battery (live)', sysBattery(d.battery), 'sy-bat')}</dl></section>
        <section class="card"><div class="spec-h">Storage</div><dl class="dl" id="sy-drives"></dl></section>
        <section class="card"><div class="spec-h">Sapu Jagat</div><dl class="dl">${dlr('Version', esc(d.engine_version))}${dlr('Access', S.boot.admin ? 'Administrator' : 'Standard user')}${dlr('Log folder', `<span class="mono">${esc(d.log_dir)}</span>`)}${dlr('Safety', 'Allow-list of locations. A restore point is created before system-level changes.')}</dl></section>
      </div>`);
    if (S.live) this.live(S.live);
    this.drives();
  },
  drives() {
    const el = $('#sy-drives'); if (!el) return;
    el.innerHTML = (S.boot.drives || []).map((d) => { const f = 1 - d.free / d.total; return `<div><dt>${esc(d.root)}</dt><dd>${human(d.free)} free of ${human(d.total)}<div style="margin-top:6px">${meter(f, tone(f))}</div></dd></div>`; }).join('');
  },
  live(p) {
    const set = (id, h) => { const el = document.getElementById(id); if (el) el.innerHTML = h; };
    set('sy-up', dur(p.uptime));
    set('sy-cpu', `${p.cpu_total.toFixed(0)}%`);
    set('sy-ram', `${human(p.ram_used)} &middot; ${Math.round((p.ram_used / p.ram_total) * 100)}%`);
    set('sy-avail', human(p.ram_total - p.ram_used));
    set('sy-swap', p.swap_total ? `${human(p.swap_used)} of ${human(p.swap_total)}` : 'Off');
    if (p.battery) set('sy-bat', sysBattery(p.battery));
    $$('#sy-cores .core i').forEach((el, i) => { el.style.height = `${Math.max(2, p.cpu_cores[i] ?? 0)}%`; });
    if (p.drives) this.drives();
  },
};

PAGES.history = {
  async render() {
    mount(loading('Reading history'));
    const h = await loadHistory(true);
    if (route !== 'history') return;
    const total = h.reduce((a, r) => a + r.total, 0);
    const recent = h.slice(0, 14).reverse(), max = Math.max(1, ...recent.map((r) => r.total));
    mount(`${pageHead('History', 'Every clean is written to a log on this PC. This page reads those logs back.')}
      <div class="grid tiles" style="margin-bottom:22px">
        <div class="card tile" style="min-height:0"><div class="lab">Total reclaimed</div><div class="big num">${human(total)}</div></div>
        <div class="card tile" style="min-height:0"><div class="lab">Cleaning runs</div><div class="big num">${h.length}</div></div>
        <div class="card tile" style="min-height:0"><div class="lab">Largest run</div><div class="big num">${human(Math.max(0, ...h.map((r) => r.total)))}</div></div>
        <div class="card tile" style="min-height:0"><div class="lab">Last run</div><div class="big num" style="font-size:20px;padding-top:8px">${h[0] ? esc(h[0].when.slice(0, 10)) : 'Never'}</div><div class="sub">${h[0] ? esc(h[0].when.slice(11)) : ''}</div></div></div>
      ${h.length ? `<div class="card" style="margin-bottom:22px"><div class="card-h"><h2>Reclaimed per run</h2><span class="faint">Last ${recent.length} runs, oldest to newest. Tallest bar: ${human(max)}</span></div><div class="bars" style="margin-top:16px">${recent.map((r) => `<div title="${esc(r.when)}  ${human(r.total)}"><i style="height:${Math.max(2, (r.total / max) * 100)}%"></i></div>`).join('')}</div><div style="height:16px"></div></div>
      <div class="card scroll-x"><table class="tbl" data-tk="hist"><thead><tr><th data-s="s">When</th><th class="r" data-s="n">Reclaimed</th><th class="r" data-s="n">Locations</th><th data-s="s">Largest items</th></tr></thead><tbody>
      ${h.map((r) => `<tr><td class="num" data-v="${esc(r.when)}">${esc(r.when)}</td><td class="r num" data-v="${r.total}"><b>${human(r.total)}</b></td><td class="r num" data-v="${r.items}">${r.items}</td><td class="muted" data-v="${esc(r.top[0] ? r.top[0][0] : '')}">${r.top.slice(0, 3).map(([n, b]) => `${esc(n)} ${human(b)}`).join(', ')}</td></tr>`).join('')}</tbody></table></div>`
      : '<div class="card empty">No cleaning has been recorded yet.</div>'}`);
  },
};

PAGES.catalog = {
  async render() {
    if (!S.catalog) { mount(loading('Loading catalog')); S.catalog = await invoke('catalog'); if (route !== 'catalog') return; }
    const by = {};
    S.catalog.forEach((r) => (by[r.cat] ||= []).push(r));
    mount(`${pageHead('Catalog', 'Everything Sapu Jagat can clean, and why. It works from an allow-list: a location that is not here is never touched. Cookies, passwords, documents and the registry are never part of it.')}
      <div class="toolbar"><span class="search">${ic('search', 'sm')}<input class="input" id="kq" placeholder="Search the catalog" autocomplete="off"></span><span class="faint">${S.catalog.length} entries</span></div>
      ${CAT_ORDER.filter((c) => by[c]).map((c) => `<section class="card cat"><div class="cat-h"><h2>${esc(by[c][0].cat_title)}</h2><span class="faint">${by[c].length}</span></div>
        ${by[c].map((r) => `<div class="catitem" data-hay="${esc(`${r.name} ${r.what} ${r.effect}`.toLowerCase())}"><div class="row" style="gap:12px"><b>${esc(r.name)}</b>${mark(r.risk, riskLabel[r.risk])}${r.admin ? `<span class="mark">${ic('lock', 'sm')}Admin</span>` : ''}</div>
          <p>${esc(r.what)}</p><p><em>Effect</em>${esc(r.effect)}</p><p class="mono" style="color:var(--text-3)">${r.locations.map(esc).join('<br>')}</p></div>`).join('')}</section>`).join('')}`);
    view.oninput = (e) => {
      if (e.target.id !== 'kq') return;
      const q = e.target.value.toLowerCase();
      $$('.catitem', view).forEach((i) => (i.hidden = q && !i.dataset.hay.includes(q)));
      $$('.cat', view).forEach((c) => (c.hidden = !$$('.catitem', c).some((i) => !i.hidden)));
    };
  },
};
