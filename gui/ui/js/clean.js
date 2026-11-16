'use strict';
/* Clean: choose what to remove, see exactly what it does, run it, read the result. */

const RISK_NOTE = {
  safe: 'Regenerates by itself. No visible side effect.',
  careful: 'Safe for Windows, but has a visible side effect (see Effect).',
  advanced: 'Changes system behaviour or removes data that is not a cache. Always your decision.',
};

const cleanHidden = (r) => !r.special && r.available && r.bytes === 0;
const RISK_ORDER = { safe: 0, careful: 1, advanced: 2 };
/* Rules you can act on always come first; the chosen sort applies within that. */
function cleanCmp(a, b) {
  const act = selectable(b) - selectable(a);
  if (act) return act;
  if (S.cf.sort === 'name') return a.name.localeCompare(b.name);
  if (S.cf.sort === 'risk') return RISK_ORDER[a.risk] - RISK_ORDER[b.risk] || (b.bytes || 0) - (a.bytes || 0);
  return (b.bytes || 0) - (a.bytes || 0);
}
function cleanMatches(r) {
  const { q, mode } = S.cf;
  if (q && !`${r.name} ${r.what} ${r.cat_title}`.toLowerCase().includes(q.toLowerCase())) return false;
  if (mode === 'recommended' && !r.default_on) return false;
  if (mode === 'admin' && !r.admin) return false;
  return true;
}

function ruleRow(r, max) {
  const dis = !selectable(r);
  const size = !r.available ? '<span class="faint">&mdash;</span>' : r.bytes === null ? '<span class="faint">measured on run</span>' : r.bytes === 0 ? '<span class="faint">&mdash;</span>' : `<b class="num">${human(r.bytes)}</b>`;
  const note = r.note ? esc(r.note) : esc(r.what);
  const run = r.running.length ? `<div class="warnline">${esc(r.running.join(', '))} is running. Close it for a complete clean.</div>` : '';
  const req = [r.admin ? 'Needs administrator' : '', r.close.length ? `Close first: ${r.close.join(', ')}` : ''].filter(Boolean).join('  ·  ');
  return `<div class="rule ${dis ? 'dis' : ''}" data-id="${esc(r.id)}" data-hay="${esc(`${r.name} ${r.what} ${r.cat_title}`.toLowerCase())}">
    <label class="chk"><input type="checkbox" data-rule="${esc(r.id)}" ${S.sel.has(r.id) ? 'checked' : ''} ${dis ? 'disabled' : ''} aria-label="Select ${esc(r.name)}"><i></i></label>
    <button class="rname" data-toggle aria-expanded="false"><b>${esc(r.name)} ${ic('chevron', 'sm chev')}</b><span class="d">${note}</span>${run}</button>
    <div class="sumrow">${mark(r.risk, riskLabel[r.risk])}</div>
    <div class="sumrow">${r.admin ? `<span class="mark">${ic('lock', 'sm')}Admin</span>` : ''}</div>
    <div class="rsize">${size}${r.bytes ? meter(r.bytes / max) : ''}</div>
    <div class="detail">
      <div class="kv"><span>What</span><span>${esc(r.what)}</span></div>
      <div class="kv"><span>Effect</span><span>${esc(r.effect)}</span></div>
      <div class="kv"><span>Risk</span><span>${esc(riskLabel[r.risk])}. ${esc(RISK_NOTE[r.risk])}</span></div>
      ${r.items.length ? `<div class="kv"><span>Found</span><span class="mono">${r.items.map(esc).join('<br>')}</span></div>` : ''}
      <div class="kv"><span>Where</span><span class="mono">${r.locations.map(esc).join('<br>')}</span></div>
      ${req ? `<div class="kv"><span>Requires</span><span>${esc(req)}</span></div>` : ''}
    </div></div>`;
}

function cleanSum() {
  const chosen = S.rules.filter((r) => S.sel.has(r.id));
  const bytes = chosen.reduce((a, r) => a + (r.bytes || 0), 0);
  const el = $('#sum');
  if (el) el.innerHTML = chosen.length ? `<b>${chosen.length}</b> selected &nbsp;&middot;&nbsp; about <b>${human(bytes)}</b>` : '<span class="muted">Nothing selected</span>';
  const b = $('#do-clean'); if (b) b.disabled = chosen.length === 0;
  $$('.cat', view).forEach((c) => {
    const boxes = $$('input[data-rule]:not(:disabled)', c).filter((i) => !i.closest('.rule').hidden);
    const all = $('input[data-all]', c); if (!all) return;
    const n = boxes.filter((i) => i.checked).length;
    all.checked = boxes.length > 0 && n === boxes.length;
    all.indeterminate = n > 0 && n < boxes.length;
    all.disabled = boxes.length === 0;
  });
}

function cleanFilter() {
  let shown = 0;
  $$('.rule', view).forEach((row) => {
    const r = S.rules.find((x) => x.id === row.dataset.id);
    const vis = cleanMatches(r);
    row.hidden = !vis;
    if (vis) shown++;
  });
  $$('.cat', view).forEach((c) => { c.hidden = !$$('.rule', c).some((x) => !x.hidden); });
  const none = $('#clean-none'); if (none) none.hidden = shown > 0;
  cleanSum();
}

PAGES.clean = {
  render() {
    if (!S.rules) { mount(loading('Scanning your system', 'Measuring 40+ cleaning locations')); scan(); return; }
    const admin = S.boot.admin, { user, adminOnly } = reclaimable();
    const byCat = {}; let hidden = 0;
    for (const r of S.rules) { if (cleanHidden(r)) { hidden++; continue; } (byCat[r.cat] ||= []).push(r); }
    const max = Math.max(1, ...S.rules.map((r) => r.bytes || 0));
    const cats = CAT_ORDER.filter((c) => byCat[c]);
    mount(`${pageHead('Clean', `${human(user)} can be reclaimed now${hidden ? `. ${hidden} locations are already clean.` : '.'}`, `<button class="btn" id="rescan2">${ic('refresh', 'sm')}Rescan</button>`)}
      ${!admin && adminOnly > 0 ? `<div class="callout">${ic('lock')}<div class="grow"><b>${human(adminOnly)}</b> of system files can only be cleaned as administrator.<div class="muted" style="font-size:13px">Windows temp and logs, update cache, crash dumps, component store.</div></div><button class="btn sm" id="go-admin2">Run as administrator</button></div>` : ''}
      <div class="toolbar"><span class="search">${ic('search', 'sm')}<input class="input" id="cq" placeholder="Search locations" value="${esc(S.cf.q)}" autocomplete="off"></span>
        <div class="seg" id="cmode">${[['all', 'All'], ['recommended', 'Recommended'], ['admin', 'Needs admin']].map(([k, l]) => `<button data-m="${k}" class="${S.cf.mode === k ? 'on' : ''}">${l}</button>`).join('')}</div>
        <span class="grow"></span><span class="muted">Sort</span><div class="seg" id="csort">${[['size', 'Size'], ['name', 'Name'], ['risk', 'Risk']].map(([k, l]) => `<button data-sort="${k}" class="${S.cf.sort === k ? 'on' : ''}">${l}</button>`).join('')}</div>
        <button class="btn sm" id="sel-all">Select all</button><button class="btn sm" id="sel-rec">Select recommended</button><button class="btn sm quiet" id="sel-none">Clear</button></div>
      ${cats.map((c) => `<section class="card cat"><div class="cat-h"><label class="chk"><input type="checkbox" data-all aria-label="Select category"><i></i></label><h2>${esc(byCat[c][0].cat_title)}</h2><span class="faint num">${human(byCat[c].reduce((a, r) => a + (r.bytes || 0), 0))}</span></div>
        ${byCat[c].sort(cleanCmp).map((r) => ruleRow(r, max)).join('')}</section>`).join('')}
      <div class="empty" id="clean-none" hidden>No locations match.</div>
      <div class="actionbar"><div class="inner"><div id="sum"></div><button class="btn primary" id="do-clean">Clean selected</button></div></div>`);
    cleanFilter();
    view.oninput = (e) => { if (e.target.id === 'cq') { S.cf.q = e.target.value; cleanFilter(); } };
    view.onclick = (e) => {
      if (e.target.closest('#sel-all')) { S.rules.filter((r) => selectable(r) && cleanMatches(r)).forEach((r) => S.sel.add(r.id)); $$('input[data-rule]').forEach((i) => (i.checked = S.sel.has(i.dataset.rule))); cleanSum(); }
      else if (e.target.closest('#sel-rec')) { S.sel = new Set(S.rules.filter((r) => r.default_on && selectable(r)).map((r) => r.id)); $$('input[data-rule]').forEach((i) => (i.checked = S.sel.has(i.dataset.rule))); cleanSum(); }
      else if (e.target.closest('#sel-none')) { S.sel.clear(); $$('input[data-rule]').forEach((i) => (i.checked = false)); cleanSum(); }
      else if (e.target.closest('#rescan2')) { S.rules = null; PAGES.clean.render(); }
      else if (e.target.closest('#go-admin2')) relaunchAdmin();
      else if (e.target.closest('#do-clean')) startClean();
      else if (e.target.closest('#csort [data-sort]')) { S.cf.sort = e.target.closest('[data-sort]').dataset.sort; PAGES.clean.render(); }
      else if (e.target.closest('#cmode [data-m]')) { S.cf.mode = e.target.closest('[data-m]').dataset.m; $$('#cmode button').forEach((b) => b.classList.toggle('on', b.dataset.m === S.cf.mode)); cleanFilter(); }
      else { const t = e.target.closest('[data-toggle]'); if (t) { const row = t.closest('.rule'); row.classList.toggle('open'); t.setAttribute('aria-expanded', row.classList.contains('open')); } }
    };
    view.onchange = (e) => {
      const cb = e.target.closest('input[data-rule]');
      if (cb) { cb.checked ? S.sel.add(cb.dataset.rule) : S.sel.delete(cb.dataset.rule); return cleanSum(); }
      const all = e.target.closest('input[data-all]');
      if (all) { $$('input[data-rule]:not(:disabled)', all.closest('.cat')).filter((i) => !i.closest('.rule').hidden).forEach((i) => { i.checked = all.checked; all.checked ? S.sel.add(i.dataset.rule) : S.sel.delete(i.dataset.rule); }); cleanSum(); }
    };
  },
};

async function startClean() {
  const chosen = S.rules.filter((r) => S.sel.has(r.id));
  if (!chosen.length) return;
  const notable = chosen.filter((r) => r.risk !== 'safe' || r.admin);
  const running = [...new Set(chosen.flatMap((r) => r.running))];
  const adv = chosen.some((r) => r.risk === 'advanced');
  const bytes = chosen.reduce((a, r) => a + (r.bytes || 0), 0);
  const body = (notable.length ? notable.map((r) => `<div class="mi"><div class="row" style="gap:12px"><b>${esc(r.name)}</b>${mark(r.risk, riskLabel[r.risk])}${r.admin ? `<span class="mark">${ic('lock', 'sm')}Admin</span>` : ''}</div><small>${esc(r.effect)}</small></div>`).join('') : '<div class="muted">Only safe caches are selected. Everything here rebuilds by itself.</div>')
    + (running.length ? `<label class="ack" style="margin-top:6px"><span class="chk"><input type="checkbox" data-key="close"><i></i></span><span>Close ${esc(running.join(', '))} first for a complete clean. Unsaved work in them is lost.</span></label>` : '')
    + (S.boot.admin && notable.length ? '<div class="muted" style="margin-top:8px">A System Restore point is created before system changes.</div>' : '');
  const r = await confirmModal({
    title: `Clean ${chosen.length} item${chosen.length > 1 ? 's' : ''}`,
    intro: `About ${human(bytes)} will be reclaimed.${notable.length ? ' Review these first.' : ''}`,
    body, ok: 'Clean', danger: adv, ack: adv ? 'I understand the advanced items change system behaviour or remove data.' : null,
  });
  if (!r.ok) return;

  S.busy = true;
  view.onclick = view.onchange = view.oninput = null;
  mount(`${pageHead('Cleaning', '<span id="p-stage">Preparing</span>')}
    <div class="card pad"><div class="spread"><b id="p-name">Starting</b><span class="muted num" id="p-count"></span></div>
    <div class="progress"><i id="p-bar"></i></div><div class="loglist" id="p-log"></div></div>`);
  const un = await listen('clean-event', (e) => onCleanEvent(e.payload));
  try {
    const report = await invoke('run_clean', { ids: chosen.map((x) => x.id), closeApps: !!r.values.close });
    S.busy = false;
    renderResult(report);
    S.history = null;
    S.rules = null; scan();
  } catch (err) {
    S.busy = false;
    toast(`Clean failed: ${err}`, 'bad');
    PAGES.clean.render();
  } finally { un(); }
}

function onCleanEvent(p) {
  const set = (id, v) => { const el = $(id); if (el) el.textContent = v; };
  if (p.kind === 'Stage') set('#p-stage', p.message);
  else if (p.kind === 'RestorePoint') set('#p-stage', p.ok ? 'Restore point created' : 'Restore point could not be created. Continuing with non-system changes.');
  else if (p.kind === 'RuleStart') { set('#p-name', p.name); set('#p-count', `${p.index + 1} of ${p.total}`); set('#p-stage', 'Cleaning'); const b = $('#p-bar'); if (b) b.style.width = `${(p.index / p.total) * 100}%`; }
  else if (p.kind === 'RuleDone') {
    const b = $('#p-bar'); if (b) b.style.width = `${((p.index + 1) / p.total) * 100}%`;
    const log = $('#p-log');
    if (log && (p.result.freed > 0 || !p.result.ok)) log.insertAdjacentHTML('afterbegin', `<div><span>${esc(p.result.name)}</span><b class="num" style="color:${p.result.ok ? 'var(--text)' : 'var(--bad)'}">${p.result.ok ? human(p.result.freed) : esc(p.result.note)}</b></div>`);
  }
}

function countUp(el, to, fmt = human) {
  const t0 = performance.now(), d = 800;
  const tick = (t) => { const k = Math.max(0, Math.min(1, (t - t0) / d)), e = 1 - Math.pow(1 - k, 3); el.textContent = fmt(Math.round(to * e)); if (k < 1) requestAnimationFrame(tick); };
  requestAnimationFrame(tick);
}

function renderResult(rep) {
  S.showingResult = true;
  const rows = [...rep.results].sort((a, b) => b.freed - a.freed);
  mount(`${pageHead('Clean complete', 'Everything below was measured by the engine. The log is saved on this PC.', `<button class="btn primary" data-go="overview">Back to overview</button>`)}
    <div class="card pad"><div class="lab muted">Reclaimed</div><div class="result-n num" id="r-big" style="margin-top:8px">0 B</div>
      <div class="muted" style="margin-top:10px">${rep.restore_point === true ? 'A restore point was created first. ' : ''}${rep.killed_apps ? `${rep.killed_apps} app process${rep.killed_apps > 1 ? 'es were' : ' was'} closed. ` : ''}${rows.filter((r) => r.freed > 0).length} locations had something to remove.</div></div>
    <div class="section-title">Drives</div>
    <div class="card">${rep.after.map((a, i) => { const b = rep.before[i], gain = a.free - b.free, f = 1 - a.free / a.total;
      return `<div class="drive"><div><b>${esc(a.root)}</b></div><div class="num muted">${human(a.free)} free of ${human(a.total)} &nbsp;&middot;&nbsp; <span style="color:${gain > 0 ? 'var(--ok)' : 'var(--text-3)'}">${gain > 0 ? '+' : gain < 0 ? '-' : ''}${human(Math.abs(gain))}</span></div>${meter(f, `lg ${tone(f)}`)}</div>`; }).join('')}</div>
    <div class="section-title">By location</div>
    <div class="card scroll-x"><table class="tbl" data-tk="cres"><thead><tr><th data-s="s">Location</th><th class="r" data-s="n">Freed</th><th class="r" data-s="n">Items</th><th data-s="s">Status</th></tr></thead><tbody>
    ${rows.map((r) => `<tr class="${r.freed ? '' : 'dim'}"><td data-v="${esc(r.name)}">${esc(r.name)}</td><td class="r num" data-v="${r.freed}">${r.freed ? `<b>${human(r.freed)}</b>` : '&mdash;'}</td><td class="r num" data-v="${r.files}">${r.files}</td>
      <td data-v="${r.ok ? (r.locked ? 'partial' : 'done') : 'failed'}">${!r.ok ? mark('bad', r.note || 'Failed') : r.locked ? mark('warn', `Partial. ${r.note}`) : r.note ? mark('ok', r.note) : mark('ok', 'Done')}</td></tr>`).join('')}</tbody></table></div>`);
  countUp($('#r-big'), rep.total_freed);
  view.onclick = (e) => { const g = e.target.closest('[data-go]'); if (g) go(g.dataset.go); };
}
