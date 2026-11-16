/* Applies the saved theme before first paint. 'auto' leaves data-theme unset so the OS decides. */
(function () {
  var pref = 'auto';
  try { pref = localStorage.getItem('sj-theme') || 'auto'; } catch (e) {}
  var q = new URLSearchParams(location.search).get('theme');
  if (q === 'light' || q === 'dark') pref = q;
  if (pref === 'light' || pref === 'dark') document.documentElement.setAttribute('data-theme', pref);
})();
