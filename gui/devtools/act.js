/* Dev-only: scripted interactions so headless screenshots can capture modals / result screens. */
(function () {
  const act = new URLSearchParams(location.search).get('act');
  const wait = (ms) => new Promise((r) => setTimeout(r, ms));
  const click = (sel) => { const el = document.querySelector(sel); if (el) el.click(); };
  (async () => {
    await wait(700);
    if (act === 'analyzer') { click('[data-open]'); }
    if (act === 'devscan') { click('#dv-scan'); }
    if (act === 'sorthist') { click('table[data-tk="hist"] th:nth-child(2)'); click('table[data-tk="hist"] th:nth-child(2)'); click('table[data-tk="hist"] th:nth-child(1)'); }
    if (act === 'sortboost') { click('th[data-b="cpu"]'); }
    if (act === 'boostopen') { await wait(600); click('.rowtoggle'); await wait(300); click('#b-all'); }
    if (act === 'storelist') { click('#an-mode [data-m="list"]'); await wait(300); click('table[data-tk="stlist"] th:nth-child(1)'); }
    if (act === 'cleanmodal' || act === 'result') {
      click('#do-clean');
      await wait(300);
      if (act === 'result') { click('[data-x="ok"]'); await wait(1700); }
    }
    if (act === 'cleanrun') { click('#do-clean'); await wait(300); click('[data-x="ok"]'); }
  })();
})();
