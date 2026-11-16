'use strict';
/* Leftovers: possible remains of uninstalled apps. Candidates only; the engine never deletes them by itself. */

const LF = { found: null, sel: new Set(), loading: false };

PAGES.leftovers = {
  render() {
    const head = pageHead('Leftovers', 'Folders in AppData and ProgramData that no installed program, Store app or running process matches. These are candidates, not verdicts: some belong to portable tools. Nothing is removed automatically. You choose, and items go to the Recycle Bin.',
      `<button class="btn primary" id="lf-scan" ${LF.loading ? 'disabled' : ''}>${LF.loading ? 'Scanning' : LF.found ? 'Scan again' : 'Scan'}</button>`);
    if (LF.loading) { mount(head + `<div class="card">${loading('Looking for leftovers', 'Reading the installed programs and measuring folders')}</div>`); this.bind(); return; }
    if (!LF.found) { mount(head + '<div class="card empty">Press Scan to look for leftovers. It takes a few seconds.</div>'); this.bind(); return; }
    if (!LF.found.length) { mount(head + '<div class="card empty">No leftovers found. Every large folder in AppData and ProgramData matches something installed.</div>'); this.bind(); return; }
    const total = LF.found.reduce((a, x) => a + x.bytes, 0), sel = LF.found.filter((x) => LF.sel.has(x.path)), selBytes = sel.reduce((a, x) => a + x.bytes, 0);
    mount(head + `<div class="card pad" style="margin-bottom:18px"><span class="big num">${human(total)}</span><span class="unit">in ${LF.found.length} candidate folders</span></div>
      <div class="card scroll-x"><table class="tbl" data-tk="left"><thead><tr><th style="width:44px"><label class="chk"><input type="checkbox" id="lf-all" aria-label="Select all candidates"><i></i></label></th><th data-s="s">Folder</th><th data-s="s">Location</th><th class="r" data-s="n">Size</th><th class="r" data-s="n">Last changed</th><th></th></tr></thead><tbody>
      ${LF.found.map((x, i) => `<tr><td><label class="chk"><input type="checkbox" data-l="${i}" ${LF.sel.has(x.path) ? 'checked' : ''} aria-label="Select ${esc(x.name)}"><i></i></label></td>
        <td data-v="${esc(x.name)}"><b>${esc(x.name)}</b><div class="sub mono ellip" title="${esc(x.path)}">${esc(x.path)}</div><div class="sub" style="white-space:normal;max-width:520px">${esc(x.why)}</div></td>
        <td data-v="${esc(x.place)}" class="muted">${esc(x.place)}</td><td class="r num" data-v="${x.bytes}"><b>${human(x.bytes)}</b></td><td class="r num" data-v="${x.age_days}">${x.age_days} d</td><td class="r">${actBtns(x.path)}</td></tr>`).join('')}</tbody></table></div>
      <div class="actionbar"><div class="inner"><div><b>${sel.length}</b> selected &nbsp;&middot;&nbsp; <b>${human(selBytes)}</b></div><button class="btn danger" id="lf-del" ${sel.length ? '' : 'disabled'}>Move selected to Recycle Bin</button></div></div>`);
    const all = $('#lf-all');
    if (all) { all.checked = sel.length === LF.found.length; all.indeterminate = sel.length > 0 && sel.length < LF.found.length; }
    this.bind();
  },
  bind() {
    view.onclick = async (e) => {
      const rv = e.target.closest('[data-reveal]'); if (rv) return revealPath(rv.dataset.reveal);
      const rc = e.target.closest('[data-recycle]');
      if (rc) { const x = LF.found.find((y) => y.path === rc.dataset.recycle); const done = await recyclePaths([{ path: rc.dataset.recycle, bytes: x ? x.bytes : 0 }]); LF.found = LF.found.filter((y) => !done.includes(y.path)); LF.sel.delete(rc.dataset.recycle); return this.render(); }
      if (e.target.closest('#lf-scan')) {
        LF.loading = true; this.render();
        try { LF.found = await invoke('leftovers_find'); LF.sel = new Set(); } catch (err) { toast(String(err), 'bad'); }
        LF.loading = false; if (route === 'leftovers') this.render();
      } else if (e.target.closest('#lf-del')) {
        const chosen = LF.found.filter((x) => LF.sel.has(x.path));
        const done = await recyclePaths(chosen.map((x) => ({ path: x.path, bytes: x.bytes })));
        LF.found = LF.found.filter((y) => !done.includes(y.path)); done.forEach((p) => LF.sel.delete(p)); this.render();
      }
    };
    view.onchange = (e) => {
      if (e.target.id === 'lf-all') { LF.sel = e.target.checked ? new Set(LF.found.map((x) => x.path)) : new Set(); return this.render(); }
      const cb = e.target.closest('input[data-l]'); if (!cb) return;
      const p = LF.found[+cb.dataset.l].path;
      cb.checked ? LF.sel.add(p) : LF.sel.delete(p);
      this.render();
    };
  },
};
