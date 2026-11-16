'use strict';
/* Boot sequence: shell, engine handshake, live feed, first route. */
(async function init() {
  buildNav();
  window.addEventListener('hashchange', () => { if (!S.busy) setRoute(); });
  try { S.boot = await invoke('boot'); }
  catch (e) { mount(loading('Could not start the engine', String(e))); return; }
  renderRail();
  await listen('live', (e) => onLive(e.payload));
  await listen('analysis-event', (e) => onAnalysisEvent(e.payload));
  setRoute();
  scan();
})();
