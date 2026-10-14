(function () {
  var $ = function (s, r) { return (r || document).querySelector(s); };
  var $$ = function (s, r) { return Array.prototype.slice.call((r || document).querySelectorAll(s)); };
  var body = document.body;
  var root = document.documentElement;
  var store = {
    get: function (k) { try { return localStorage.getItem(k); } catch (e) { return null; } },
    set: function (k, v) { try { localStorage.setItem(k, v); } catch (e) {} }
  };

  /* theme toggle: follows the system until the reader picks one */
  var saved = store.get('am-theme');
  if (saved === 'light' || saved === 'dark') root.setAttribute('data-theme', saved);
  $$('.theme').forEach(function (b) { b.addEventListener('click', function () {
    var cur = root.getAttribute('data-theme') ||
      (window.matchMedia && matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light');
    var next = cur === 'dark' ? 'light' : 'dark';
    root.setAttribute('data-theme', next);
    store.set('am-theme', next);
  }); });

  /* mobile drawer */
  var menu = $('#menu');
  function setNav(open) {
    body.classList.toggle('nav-open', open);
    menu.setAttribute('aria-expanded', open ? 'true' : 'false');
  }
  menu.addEventListener('click', function () { setNav(!body.classList.contains('nav-open')); });
  $('#scrim').addEventListener('click', function () { setNav(false); });
  document.addEventListener('keydown', function (e) { if (e.key === 'Escape') setNav(false); });
  $('#nav').addEventListener('click', function (e) {
    var a = e.target.closest && e.target.closest('a');
    if (a) setNav(false);
  });

  /* nav groups */
  var groups = $$('.ng');
  function openGroup(g, open) {
    g.classList.toggle('open', open);
    $('.ng-toggle', g).setAttribute('aria-expanded', open ? 'true' : 'false');
  }
  groups.forEach(function (g) {
    $('.ng-toggle', g).addEventListener('click', function () { openGroup(g, !g.classList.contains('open')); });
  });

  /* heading filter */
  var q = $('#q'), none = $('#none');
  var lastOpen = null;
  function filter() {
    var t = q.value.trim().toLowerCase();
    var any = false;
    if (t && lastOpen === null) lastOpen = groups.map(function (g) { return g.classList.contains('open'); });
    groups.forEach(function (g, gi) {
      var hits = 0;
      $$('li', g).forEach(function (li) {
        var m = !t || $('a', li).getAttribute('data-text').indexOf(t) !== -1;
        li.classList.toggle('hide', !m);
        if (m) hits++;
      });
      var titleHit = t && $('.ng-link', g).textContent.toLowerCase().indexOf(t) !== -1;
      var show = !t || hits > 0 || titleHit;
      g.classList.toggle('hide', !show);
      if (show) any = true;
      if (t) openGroup(g, hits > 0);
      else if (lastOpen) openGroup(g, lastOpen[gi]);
    });
    if (!t) lastOpen = null;
    none.hidden = any;
  }
  q.addEventListener('input', filter);
  q.addEventListener('keydown', function (e) {
    if (e.key === 'Enter') {
      var first = $('.ng:not(.hide) li:not(.hide) a');
      if (first) { first.click(); }
    }
    if (e.key === 'Escape' && q.value) { q.value = ''; filter(); }
  });

  /* scrollspy */
  var heads = $$('.prose h1, .prose h2, .prose h3');
  var links = {};
  $$('#nav a').forEach(function (a) { links[a.getAttribute('href').slice(1)] = a; });
  var current = null;
  function setCurrent(id) {
    if (id === current) return;
    current = id;
    $$('#nav [aria-current]').forEach(function (a) { a.removeAttribute('aria-current'); });
    var a = links[id];
    if (!a) return;
    a.setAttribute('aria-current', 'true');
    var g = a.closest('.ng');
    if (g && !q.value) {
      groups.forEach(function (x) { openGroup(x, x === g); });
    }
    var side = $('.side-in');
    var r = a.getBoundingClientRect(), s = side.getBoundingClientRect();
    if (r.top < s.top + 60 || r.bottom > s.bottom - 20) side.scrollTop += r.top - s.top - s.height / 3;
    store.set('am-last', id);
  }
  function spy() {
    var y = 90, best = heads[0];
    for (var i = 0; i < heads.length; i++) {
      if (heads[i].getBoundingClientRect().top <= y) best = heads[i]; else break;
    }
    if (best) setCurrent(best.id);
  }
  var ticking = false;
  window.addEventListener('scroll', function () {
    if (ticking) return;
    ticking = true;
    requestAnimationFrame(function () { ticking = false; spy(); });
  }, { passive: true });

  /* restore last position when opened without a deep link */
  if (!location.hash) {
    var last = store.get('am-last');
    var el = last && document.getElementById(last);
    if (el && last !== 'overview') el.scrollIntoView();
  }
  spy();
  if (!current) openGroup(groups[0], true);

  /* copy buttons */
  function fallbackCopy(code, btn) {
    try {
      var sel = window.getSelection(), range = document.createRange();
      range.selectNodeContents(code);
      sel.removeAllRanges(); sel.addRange(range);
      var ok = document.execCommand && document.execCommand('copy');
      flash(btn, ok ? 'Copied' : 'Selected');
    } catch (e) { flash(btn, 'Selected'); }
  }
  function flash(btn, msg) {
    btn.textContent = msg; btn.classList.add('done');
    setTimeout(function () { btn.textContent = 'Copy'; btn.classList.remove('done'); }, 1600);
  }
  document.addEventListener('click', function (e) {
    var btn = e.target.closest && e.target.closest('.copy');
    if (!btn) return;
    var code = $('code', btn.closest('.code'));
    var text = code.textContent;
    if (navigator.clipboard && navigator.clipboard.writeText) {
      navigator.clipboard.writeText(text).then(function () { flash(btn, 'Copied'); }, function () { fallbackCopy(code, btn); });
    } else fallbackCopy(code, btn);
  });
})();
