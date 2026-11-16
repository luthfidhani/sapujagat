'use strict';
/* Settings: excluded folders, rules on/off, what is recommended, restore point. Saved immediately, shared with the CLI. */

PAGES.settings = {
  async render() {
    mount(loading('Reading settings'));
    try { S.settings = await invoke('settings_get'); } catch (e) { mount(pageHead('Settings') + `<div class="card empty">${esc(String(e))}</div>`); return; }
    if (route === 'settings') this.draw();
  },
  draw() {
    const { settings: s, rules } = S.settings;
    mount(`${pageHead('Settings', 'These apply to this app and to the command line version. Changes are saved immediately.')}
      <section class="card" style="margin-bottom:16px"><div class="card-h"><h2>Safety</h2></div><div style="margin-top:6px">
        <div class="setrow"><div><b>Create a restore point before system changes</b><p>Recommended. Made once per run, only when the run is elevated and includes system-level or non-safe items. Windows may limit how often it keeps them.</p></div>
        <label class="sw"><input type="checkbox" id="st-rp" ${s.restore_point ? 'checked' : ''} aria-label="Create a restore point"><i></i></label></div></div></section>
      <section class="card" style="margin-bottom:16px"><div class="card-h"><h2>Excluded folders</h2></div>
        <p class="muted" style="padding:6px 20px 12px">Nothing inside these folders is ever cleaned, measured for deletion or removed, even when a rule points at it. Use full folder paths.</p>
        ${s.exclusions.length ? s.exclusions.map((p, i) => `<div class="exrow"><span class="mono">${esc(p)}</span><button class="iconbtn" data-rm="${i}" title="Remove this exclusion" aria-label="Remove exclusion">${ic('x', 'sm')}</button></div>`).join('') : '<div class="empty" style="border-top:1px solid var(--line)">No folders are excluded.</div>'}
        <div class="form-row"><input class="input mono" id="st-path" placeholder="D:\\Projects\\keep-this-folder" autocomplete="off" aria-label="Folder to exclude"><button class="btn" id="st-add">${ic('plus', 'sm')}Add folder</button></div></section>
      <section class="card scroll-x"><div class="card-h"><h2>Cleaning rules</h2><button class="btn sm" id="st-reset">Reset rules to defaults</button></div>
        <p class="muted" style="padding:6px 20px 12px">Turn a rule off to remove it from scans and cleaning entirely. Recommended decides whether it is ticked by default on the Clean page.</p>
        <table class="tbl" data-tk="strules"><thead><tr><th data-s="s">Rule</th><th data-s="s">Category</th><th data-s="s">Risk</th><th class="r" data-s="n">Enabled</th><th class="r" data-s="n">Recommended</th></tr></thead><tbody>
        ${rules.map((r) => `<tr><td data-v="${esc(r.name)}"><b>${esc(r.name)}</b>${r.admin ? ` <span class="mark">${ic('lock', 'sm')}Admin</span>` : ''}</td><td class="muted" data-v="${esc(r.cat_title)}">${esc(r.cat_title)}</td><td data-v="${r.risk}">${mark(r.risk, riskLabel[r.risk])}</td>
          <td class="r" data-v="${r.enabled ? 1 : 0}"><label class="sw"><input type="checkbox" data-en="${esc(r.id)}" ${r.enabled ? 'checked' : ''} aria-label="Enable ${esc(r.name)}"><i></i></label></td>
          <td class="r" data-v="${r.enabled && r.recommended ? 1 : 0}"><label class="sw"><input type="checkbox" data-rec="${esc(r.id)}" ${r.recommended ? 'checked' : ''} ${r.enabled ? '' : 'disabled'} aria-label="Recommended: ${esc(r.name)}"><i></i></label></td></tr>`).join('')}</tbody></table></section>`);
    this.bind();
  },
  async save(next, msg = 'Saved.') {
    try {
      S.settings = await invoke('settings_save', { new: next });
      S.rules = null; S.selInit = false; S.catalog = null; // scans must reflect the new rules
      toast(msg, 'ok');
    } catch (e) { toast(String(e), 'bad'); }
    if (route === 'settings') this.draw();
  },
  bind() {
    const cur = () => JSON.parse(JSON.stringify(S.settings.settings));
    const add = () => {
      const input = $('#st-path'), v = input.value.trim().replace(/^"|"$/g, '');
      if (!v) return toast('Type a full folder path first.', 'warn');
      const n = cur(); n.exclusions.push(v);
      this.save(n, 'Folder excluded.');
    };
    view.onclick = (e) => {
      const rm = e.target.closest('[data-rm]');
      if (rm) { const n = cur(); n.exclusions.splice(+rm.dataset.rm, 1); return this.save(n, 'Exclusion removed.'); }
      if (e.target.closest('#st-add')) return add();
      if (e.target.closest('#st-reset')) { const n = cur(); n.disabled_rules = []; n.recommended = {}; this.save(n, 'Rules reset to their defaults.'); }
    };
    const inp = $('#st-path');
    if (inp) inp.onkeydown = (e) => { if (e.key === 'Enter') add(); };
    view.onchange = (e) => {
      if (e.target.id === 'st-rp') { const n = cur(); n.restore_point = e.target.checked; return this.save(n); }
      const en = e.target.closest('input[data-en]');
      if (en) { const n = cur(), id = en.dataset.en; n.disabled_rules = n.disabled_rules.filter((x) => x !== id); if (!en.checked) n.disabled_rules.push(id); return this.save(n); }
      const rc = e.target.closest('input[data-rec]');
      if (rc) {
        const n = cur(), id = rc.dataset.rec, base = S.settings.rules.find((r) => r.id === id).base_recommended;
        if (rc.checked === base) delete n.recommended[id]; else n.recommended[id] = rc.checked;
        return this.save(n);
      }
    };
  },
};
