import { t } from './lib/i18n.js';

// GitHub Actions (Rust emite `ci` = rulările în curs și `ci-done` = o rulare terminată):
// pastilă cu inel care se rotește cât rulează ceva, la final o tranzitorie verde / roșie; click = rularea în browser.
export function init(island) {
  const { invoke, tauri } = island;
  const $ = (id) => document.getElementById(id);
  let enabled = false;
  let list = [];

  function renderPill() {
    if (list.length) {
      $('c-ci-text').textContent = list.length === 1 ? `${list[0].project} · ${list[0].name}` : t('ci.runs', { n: list.length });
    }
    island.setActivity('ci', list.length ? { kind: 'ci' } : null);
  }

  function apply(next) {
    list = next;
    renderPill();
  }

  function done(r) {
    const ok = r.state === 'ok';
    island.pushTransient({
      kind: 'ci', url: r.url, icon: ok ? 'check' : 'cross', color: ok ? 'var(--green)' : 'var(--red)',
      title: r.project, detail: r.name, duration: 5000, interactive: true,
    });
  }

  // click pe tranzitorie: rularea în browser; opresc propagarea ca să nu se extindă și insula
  $('transient').addEventListener('click', (e) => {
    const s = island.shown;
    if (s?.kind !== 'ci') return;
    e.stopPropagation();
    invoke('open_url', { url: s.url }).catch((err) => invoke('log_js', { msg: `ci: ${err}` }));
    island.dropTransients('ci');
  });

  // demo din browser (?demo&ci), fără Rust
  window.addEventListener('ci-demo', (e) => { enabled = true; apply(e.detail); });
  window.addEventListener('ci-done-demo', (e) => { enabled = true; done(e.detail); });
  if (!tauri) return;

  tauri.event.listen('ci', (e) => enabled && apply(e.payload));
  tauri.event.listen('ci-done', (e) => enabled && done(e.payload));
  window.addEventListener('island-settings', (e) => {
    const on = e.detail.modules.ci === true; // implicit oprit
    renderPill(); // aceeași stare: doar textele (limba)
    if (on === enabled) return;
    enabled = on;
    if (!on) { apply([]); island.dropTransients('ci'); }
  });
}
