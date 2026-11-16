'use strict';
/* Storage: cached space map, all drives analysed in parallel. Developer: regenerable project output.
   An analysis is stored on disk by the engine and reused until a new one is requested. */

function crumbParts(p) {
  const parts = p.split('\\').filter(Boolean);
  const out = [];
  let acc = '';
  parts.forEach((seg, i) => {
    acc = i === 0 ? `${seg}\\` : acc.endsWith('\\') ? acc + seg : `${acc}\\${seg}`;
    out.push([i === 0 ? `${seg}\\` : seg, acc]);
  });
  return out;
}

const tagClass = (t) => (!t ? 't-plain' : t.kind === 'danger' ? 't-danger' : t.kind === 'review' || t.kind === 'warn' ? 't-review' : t.kind === 'cleanable' || t.kind === 'contains' ? 't-cleanable' : 't-plain');
const anRootOf = (path) => (S.boot.drives.find((d) => path.toLowerCase().startsWith(d.root.toLowerCase())) || {}).root || path;

function drawTreemap() {
  const el = $('#tm'), d = S.an.data;
  if (!el || !d) return;
  const W = el.clientWidth, H = el.clientHeight;
  const pos = d.items.filter((i) => i.bytes > 0);
  const top = pos.slice(0, 30), rest = pos.slice(30).reduce((a, i) => a + i.bytes, 0);
  const items = top.map((it) => ({ v: it.bytes, it }));
  if (rest > 0) items.push({ v: rest, other: true, n: pos.length - 30 });
  el.innerHTML = squarify(items, { x: 0, y: 0, w: W, h: H }).map((r) => {
    const big = r.w > 84 && r.h > 40;
    if (r.other) return `<div class="tm-b t-other" style="left:${r.x}px;top:${r.y}px;width:${r.w}px;height:${r.h}px">${big ? `<b>${r.n} smaller items</b><span>${human(r.v)}</span>` : ''}</div>`;
    const it = r.it;
    return `<div class="tm-b ${tagClass(it.tag)} ${it.is_dir ? 'dir' : ''}" ${it.is_dir ? `data-open="${esc(it.path)}"` : ''} title="${esc(`${it.name}  ${human(it.bytes)}${it.tag ? '  ' + it.tag.text : ''}`)}" style="left:${r.x}px;top:${r.y}px;width:${r.w}px;height:${r.h}px">${big ? `<b>${esc(it.name)}</b><span>${human(it.bytes)}</span>` : ''}</div>`;
  }).join('');
}

/* ───────── analysis control ───────── */
async function anStatus() {
  try { const st = await invoke('analysis_status'); S.an.status = {}; st.forEach((s) => (S.an.status[s.root] = s)); } catch {}
}

/* Opens a folder: stored result first, otherwise a fresh scan (never a second scan of a drive that is already running). */
async function openPath(path, force = false) {
  const a = S.an;
  a.path = path; a.err = null;
  const root = anRootOf(path);
  if (!force) {
    const c = await invoke('analysis_get', { path });
    if (c) { a.data = c; a.loading = false; if (route === 'storage') PAGES.storage.render(); return; }
    if (a.running.has(root) && path === root) { a.data = null; a.loading = true; if (route === 'storage') PAGES.storage.render(); return; }
  }
  a.data = null; a.loading = true;
  if (route === 'storage') PAGES.storage.render();
  try { a.data = await invoke('analyze_path', { path, force }); await anStatus(); }
  catch (e) { a.err = String(e); }
  a.loading = false;
  if (route === 'storage') PAGES.storage.render();
}

function analyzeAll(force) {
  invoke('analyze_all', { force }).catch((e) => toast(String(e), 'bad'));
}

function onAnalysisEvent(p) {
  const a = S.an;
  if (p.status === 'start') a.running.add(p.root);
  else {
    a.running.delete(p.root);
    if (p.status === 'done' && p.view) {
      a.status[p.root] = { root: p.root, analyzed_at: p.view.analyzed_at, took_secs: p.view.took_secs };
      if (a.drive === p.root && (!a.path || a.path === p.root)) { a.data = p.view; a.path = p.root; a.loading = false; }
    } else if (p.status === 'error' && a.drive === p.root) { a.err = p.error; a.loading = false; }
  }
  if (route === 'storage') PAGES.storage.render();
}

PAGES.storage = {
  render() {
    const drives = S.boot.drives, a = S.an;
    if (!a.drive) a.drive = drives[0].root;
    if (!a.init) {
      a.init = true;
      a.loading = true; a.path = a.drive;
      anStatus().then(() => analyzeAll(false)); // stored drives answer instantly; missing ones are scanned in parallel
    }
    if (!S.history && !a.histLoading) { a.histLoading = true; loadHistory().then(() => { a.histLoading = false; if (route === 'storage') PAGES.storage.render(); }); }
    const cur = drives.find((d) => d.root === a.drive) || drives[0];
    const running = a.running.size, hasAny = Object.keys(a.status).length > 0;
    const cards = drives.map((d) => {
      const st = a.status[d.root], run = a.running.has(d.root), f = 1 - d.free / d.total;
      return `<button class="dcard ${d.root === a.drive ? 'on' : ''}" data-d="${esc(d.root)}" aria-pressed="${d.root === a.drive}">
        <div class="spread"><b>${esc(d.root)}</b><span class="num muted">${human(d.total - d.free)} of ${human(d.total)}</span></div>${meter(f, tone(f))}
        <div class="st">${run ? '<span class="spin"></span>Analyzing' : st ? `Analyzed ${esc(ago(st.analyzed_at))} &middot; ${st.took_secs.toFixed(0)} s` : 'Not analyzed'}</div></button>`;
    }).join('');

    let area = '';
    if (a.loading || (a.running.has(cur.root) && !a.data)) {
      area = `<div class="card">${loading(`Measuring ${a.path || cur.root}`, running > 1 ? `Analyzing ${[...a.running].join('  ')} in parallel. A whole drive can take about a minute.` : 'A whole drive can take about a minute.')}</div>`;
    } else if (a.err) area = `<div class="callout">${ic('warn')}<div class="grow">${esc(a.err)}</div></div>`;
    else if (!a.data) area = `<div class="card center"><div><b>${esc(cur.root)} has not been analyzed</b><div class="muted" style="margin:6px 0 16px">Nothing is deleted here. The map shows what is big and what is safe to clean.</div><button class="btn primary" data-open="${esc(cur.root)}">Analyze ${esc(cur.root)}</button></div></div>`;
    else {
      const d = a.data, max = Math.max(1, d.items[0]?.bytes || 1);
      const lastClean = S.history && S.history[0];
      const stale = lastClean && new Date(lastClean.when.replace(' ', 'T')).getTime() / 1000 > d.analyzed_at;
      const crumbs = crumbParts(d.root).map(([l, p], i) => `${i ? '<span class="sep">/</span>' : ''}<button data-open="${esc(p)}">${esc(l)}</button>`).join('');
      area = `<div class="spread" style="margin-bottom:10px"><div class="crumbs">${crumbs}</div>
          <div class="row">${d.parent ? `<button class="btn sm" data-open="${esc(d.parent)}">${ic('up_dir', 'sm')}Up</button>` : ''}<button class="btn sm" id="an-re" title="Scan this folder again">${ic('refresh', 'sm')}Re-analyze</button><div class="seg" id="an-mode"><button data-m="map" class="${a.mode === 'map' ? 'on' : ''}">Map</button><button data-m="list" class="${a.mode === 'list' ? 'on' : ''}">List</button></div></div></div>
        <div class="muted" style="margin-bottom:12px"><b class="num" style="color:var(--text)">${human(d.total)}</b> in this folder &nbsp;&middot;&nbsp; analyzed ${esc(ago(d.analyzed_at))}, took ${d.took_secs.toFixed(0)} s. Saved until you analyze again.${d.unreadable ? ` ${d.unreadable} items could not be read, run as administrator for the full picture.` : ''}${stale ? ' <span style="color:var(--warn)">A clean ran after this analysis, so sizes may be out of date. Re-analyze to refresh.</span>' : ''}</div>
        ${a.mode === 'map' ? `<div class="card pad" style="padding:12px"><div class="tm" id="tm"></div>
          <div class="row" style="gap:18px;margin:12px 4px 2px;font-size:12.5px;color:var(--text-2)"><span><i class="swatch" style="background:color-mix(in srgb,var(--ok) 40%,var(--surface))"></i>Cleanable</span><span><i class="swatch" style="background:color-mix(in srgb,var(--warn) 40%,var(--surface))"></i>Your files or Windows-managed</span><span><i class="swatch" style="background:color-mix(in srgb,var(--text) 14%,var(--surface))"></i>Windows, do not touch</span><span><i class="swatch" style="background:color-mix(in srgb,var(--accent) 30%,var(--surface))"></i>Everything else</span><span class="faint">Click a folder to open it</span></div></div>`
        : `<div class="card scroll-x"><table class="tbl" data-tk="stlist"><thead><tr><th data-s="s">Name</th><th data-s="n">Size</th><th class="r" data-s="n">Share</th><th data-s="s">Note</th><th></th></tr></thead><tbody>${d.items.slice(0, 60).map((it) => `<tr class="${it.is_dir && it.bytes ? 'click' : ''}" ${it.is_dir && it.bytes ? `data-open="${esc(it.path)}"` : ''}>
          <td data-v="${esc(it.name)}"><span class="row" style="gap:10px;flex-wrap:nowrap">${ic(it.is_dir ? 'folder' : 'file', 'sm')}<b>${esc(it.name)}</b></span></td><td data-v="${it.bytes}" style="min-width:170px"><div class="num">${human(it.bytes)}</div>${meter(it.bytes / max, tone(d.total ? it.bytes / d.total : 0))}</td>
          <td class="r num" data-v="${d.total ? (it.bytes * 100) / d.total : 0}">${d.total ? ((it.bytes * 100) / d.total).toFixed(1) : '0.0'}%</td><td data-v="${esc(it.tag ? it.tag.text : '')}">${it.tag ? `<span class="tagtxt ${it.tag.kind}">${esc(it.tag.text)}</span>` : ''}</td><td class="r">${actBtns(it.path)}</td></tr>`).join('')}</tbody></table></div>`}
        ${d.big_files.length ? `<div class="section-title">Largest single files</div><div class="card scroll-x"><table class="tbl" data-tk="stbig"><thead><tr><th class="r" data-s="n">Size</th><th data-s="s">File</th><th data-s="s">Note</th><th></th></tr></thead><tbody>${d.big_files.map((x) => `<tr><td class="r num" data-v="${x.bytes}" style="width:100px"><b>${human(x.bytes)}</b></td><td class="mono ellip" data-v="${esc(x.path)}" style="max-width:620px" title="${esc(x.path)}">${esc(x.path)}</td><td data-v="${esc(x.tag ? x.tag.text : '')}">${x.tag ? `<span class="tagtxt ${x.tag.kind}">${esc(x.tag.text)}</span>` : ''}</td><td class="r">${actBtns(x.path)}</td></tr>`).join('')}</tbody></table></div>` : ''}`;
    }
    mount(`${pageHead('Storage', 'Where your space went, and what is safe to do about it. All drives are analyzed in parallel and the results are saved until you analyze again. This page only reads; it never deletes.',
      `<button class="btn ${hasAny ? '' : 'primary'}" id="an-all" ${running ? 'disabled' : ''}>${ic('refresh', 'sm')}${running ? `Analyzing ${running} drive${running > 1 ? 's' : ''}` : hasAny ? 'Re-analyze all drives' : 'Analyze all drives'}</button>`)}
      <div class="dcards">${cards}</div>${area}`);
    if (a.data && a.mode === 'map' && !a.loading) drawTreemap();
    view.onclick = async (e) => {
      const rv = e.target.closest('[data-reveal]');
      if (rv) return revealPath(rv.dataset.reveal);
      const rc = e.target.closest('[data-recycle]');
      if (rc) {
        const p = rc.dataset.recycle, it = a.data && (a.data.items.find((i) => i.path === p) || a.data.big_files.find((f) => f.path === p));
        if ((await recyclePaths([{ path: p, bytes: it ? it.bytes : 0 }])).length) { const c = await invoke('analysis_get', { path: a.path }); if (c) a.data = c; if (route === 'storage') this.render(); }
        return;
      }
      if (e.target.closest('#an-all')) return analyzeAll(true);
      const d = e.target.closest('.dcard[data-d]');
      if (d) { a.drive = d.dataset.d; a.data = null; a.err = null; return openPath(a.drive); }
      if (e.target.closest('#an-re')) return openPath(a.path || a.drive, true);
      const m = e.target.closest('#an-mode [data-m]');
      if (m) { a.mode = m.dataset.m; return this.render(); }
      const o = e.target.closest('[data-open]');
      if (o) openPath(o.dataset.open);
    };
    window.onresize = () => { if (route === 'storage' && S.an.data && S.an.mode === 'map') drawTreemap(); };
  },
  leave() { window.onresize = null; },
};

/* ───────── developer ───────── */
PAGES.developer = {
  render() {
    const dv = S.dev, drives = S.boot.drives;
    if (!dv.roots.size && !dv.found) drives.forEach((d) => dv.roots.add(d.root));
    const head = pageHead('Developer', 'Build output of projects you may have stopped touching: node_modules, Rust target, virtual environments, bytecode. A folder is listed only when a real project file (package.json, Cargo.toml) sits next to it. Everything here is rebuilt by your tools.');
    const picker = `<div class="toolbar"><div class="seg" id="dv-roots">${drives.map((d) => `<button data-r="${esc(d.root)}" class="${dv.roots.has(d.root) ? 'on' : ''}">${esc(d.root)}</button>`).join('')}</div><button class="btn primary" id="dv-scan" ${dv.loading ? 'disabled' : ''}>${dv.loading ? 'Searching' : 'Search'}</button><span class="faint">System folders, AppData and Program Files are always skipped.</span></div>`;
    if (dv.loading) { mount(head + picker + `<div class="card">${loading('Searching for build output')}</div>`); this.bind(); return; }
    if (!dv.found) { mount(head + picker + `<div class="card empty">Choose the drives to search, then press Search.</div>`); this.bind(); return; }
    if (!dv.found.length) { mount(head + picker + `<div class="card empty">No build output worth cleaning was found.</div>`); this.bind(); return; }
    const kinds = {}; dv.found.forEach((j) => (kinds[j.kind] = (kinds[j.kind] || 0) + j.bytes));
    const kk = Object.keys(kinds).sort((a, b) => kinds[b] - kinds[a]), total = kk.reduce((a, k) => a + kinds[k], 0) || 1;
    const sel = dv.found.filter((j) => dv.sel.has(j.path)), selBytes = sel.reduce((a, j) => a + j.bytes, 0);
    mount(head + picker + `<div class="card pad" style="margin-bottom:18px"><div class="spread"><div><span class="big num">${human(total)}</span><span class="unit">in ${dv.found.length} folders</span></div><div class="muted num">${dv.found.filter((j) => j.age_days >= 30).length} idle for 30+ days</div></div>
        <div class="stack" style="margin-top:14px">${kk.map((k, i) => `<i class="c${i}" style="width:${(kinds[k] / total) * 100}%"></i>`).join('')}</div>
        <div class="legend">${kk.map((k, i) => `<div><span class="sw8 c${i}"></span><span class="n">${esc(k)}</span><span class="num">${human(kinds[k])}</span></div>`).join('')}</div></div>
      <div class="card scroll-x"><table class="tbl" data-tk="dev"><thead><tr><th style="width:44px"><label class="chk"><input type="checkbox" id="dv-all" aria-label="Select all folders"><i></i></label></th><th data-s="s">Project</th><th data-s="s">Kind</th><th class="r" data-s="n">Idle</th><th class="r" data-s="n">Size</th></tr></thead><tbody>
      ${dv.found.map((j, i) => `<tr><td style="width:44px"><label class="chk"><input type="checkbox" data-j="${i}" ${dv.sel.has(j.path) ? 'checked' : ''} aria-label="Select ${esc(j.path)}"><i></i></label></td>
        <td data-v="${esc(j.project.split('\\').pop() || j.project)}"><b>${esc(j.project.split('\\').pop() || j.project)}</b><div class="sub mono ellip" title="${esc(j.path)}">${esc(j.path)}</div></td><td data-v="${esc(j.kind)}">${esc(j.kind)}<div class="sub">rebuild with ${esc(j.rebuild_hint)}</div></td>
        <td class="r num" data-v="${j.age_days}">${j.age_days} d</td><td class="r num" data-v="${j.bytes}"><b>${human(j.bytes)}</b></td></tr>`).join('')}</tbody></table></div>
      <div class="actionbar"><div class="inner"><div><b>${sel.length}</b> selected &nbsp;&middot;&nbsp; <b>${human(selBytes)}</b></div><button class="btn danger" id="dv-del" ${sel.length ? '' : 'disabled'}>Delete selected</button></div></div>`);
    const all = $('#dv-all');
    if (all) { all.checked = sel.length === dv.found.length; all.indeterminate = sel.length > 0 && sel.length < dv.found.length; }
    this.bind();
  },
  bind() {
    const dv = S.dev;
    view.onclick = async (e) => {
      const r = e.target.closest('#dv-roots [data-r]');
      if (r) { const k = r.dataset.r; dv.roots.has(k) ? dv.roots.delete(k) : dv.roots.add(k); return this.render(); }
      if (e.target.closest('#dv-scan')) {
        if (!dv.roots.size) return toast('Pick at least one drive.', 'warn');
        dv.loading = true; this.render();
        try { dv.found = await invoke('dev_find', { roots: [...dv.roots] }); dv.sel = new Set(dv.found.filter((j) => j.age_days >= 30).map((j) => j.path)); } catch (err) { toast(String(err), 'bad'); }
        dv.loading = false; if (route === 'developer') this.render();
      } else if (e.target.closest('#dv-del')) {
        const chosen = dv.found.filter((j) => dv.sel.has(j.path)), sum = chosen.reduce((a, j) => a + j.bytes, 0);
        const ok = await confirmModal({ title: `Delete ${chosen.length} folder${chosen.length > 1 ? 's' : ''}`, intro: `${human(sum)} of build output. Links and junctions inside are unlinked, never followed.`, body: chosen.slice(0, 8).map((j) => `<div class="mi mono">${esc(j.path)}</div>`).join('') + (chosen.length > 8 ? `<div class="mi muted">and ${chosen.length - 8} more</div>` : ''), ok: 'Delete', danger: true });
        if (!ok.ok) return;
        const res = await invoke('dev_delete', { paths: chosen.map((j) => j.path) });
        toast(`Freed ${human(res.freed)} from ${res.deleted} folder${res.deleted === 1 ? '' : 's'}.${res.locked ? ` ${res.locked} files in use were skipped.` : ''}`, 'ok');
        dv.sel.clear();
        try { dv.found = await invoke('dev_find', { roots: [...dv.roots] }); } catch {}
        this.render();
      }
    };
    view.onchange = (e) => {
      if (e.target.id === 'dv-all') { dv.sel = e.target.checked ? new Set(dv.found.map((j) => j.path)) : new Set(); return this.render(); }
      const cb = e.target.closest('input[data-j]'); if (!cb) return;
      const p = dv.found[+cb.dataset.j].path;
      cb.checked ? dv.sel.add(p) : dv.sel.delete(p);
      this.render();
    };
  },
};
