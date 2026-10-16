import { ICONS } from './icons.js';
import { t } from './lib/i18n.js';
import { formatBytes, formatRate } from './lib/format.js';

// Fișierele care rulează cod cer o a doua apăsare pe „Deschide”
const RUNS_CODE = /\.(exe|msi|bat|cmd|ps1)$/i;
const SURE_MS = 3000;

// Descărcările din browser (Rust emite `downloads` și `download-done`): pastilă cu inel care se rotește,
// iar la final o tranzitorie interactivă cu fișierul (Deschide, Arată în folder, tragere afară).
export function init(island) {
  const { invoke, tauri } = island;
  const $ = (id) => document.getElementById(id);
  const log = (msg) => invoke('log_js', { msg: `descărcări: ${msg}` }).catch(() => {});
  $('d-file').innerHTML = ICONS.file;
  let enabled = false;
  let list = [];

  function renderPill() {
    if (list.length) {
      const one = list.length === 1 ? list[0] : null;
      const speed = list.reduce((s, d) => s + d.speed, 0);
      $('c-download-name').textContent = one ? one.name || t('download.unnamed') : t('download.count', { n: list.length });
      $('c-download-info').textContent = one ? `${formatBytes(one.bytes)} · ${formatRate(speed)}` : formatRate(speed);
    }
    island.setActivity('download', list.length ? { kind: 'download' } : null);
  }

  function apply(next) {
    list = next;
    renderPill();
  }

  function done(d) {
    island.dropTransients('download'); // o descărcare nouă o înlocuiește pe cea veche
    island.pushTransient({ kind: 'download', interactive: true, ...d, duration: 6000 });
  }

  // ---- tranzitoria: se umple când ajunge la rând (island.js trimite `download-show`)
  let armTimer = 0;
  const open = $('d-open');
  function disarm() {
    clearTimeout(armTimer);
    armTimer = 0;
    open.textContent = t('download.open');
  }

  window.addEventListener('download-show', (e) => {
    disarm();
    $('d-name').textContent = e.detail.name;
    $('d-size').textContent = formatBytes(e.detail.bytes);
  });

  const current = () => (island.shown?.kind === 'download' ? island.shown : null);
  const run = (command) => (e) => {
    e.stopPropagation();
    const d = current();
    if (!d) return;
    invoke(command, { path: d.path }).catch((err) => log(`${command}: ${err}`));
    island.dropTransients('download');
  };

  open.addEventListener('click', (e) => {
    const d = current();
    if (d && RUNS_CODE.test(d.name) && !armTimer) {
      e.stopPropagation();
      open.textContent = t('download.sure');
      armTimer = setTimeout(disarm, SURE_MS);
      return;
    }
    run('shelf_open')(e);
  });
  $('d-reveal').addEventListener('click', run('shelf_reveal'));
  $('d-close').addEventListener('click', (e) => { e.stopPropagation(); island.dropTransients('download'); });

  // tragere afară din insulă: după câțiva pixeli de mișcare pornește drag-ul nativ (ca la raft)
  const file = $('d-file');
  let start = null;
  file.addEventListener('pointerdown', (e) => { if (e.button === 0) start = { x: e.clientX, y: e.clientY }; });
  file.addEventListener('pointermove', (e) => {
    if (start && !(e.buttons & 1)) { start = null; return; } // butonul a fost eliberat în afara elementului
    const d = current();
    if (!start || !d || Math.hypot(e.clientX - start.x, e.clientY - start.y) < 5) return;
    start = null;
    invoke('shelf_drag_out', { path: d.path }).catch((err) => log(`tragerea a eșuat: ${err}`));
  });
  file.addEventListener('pointerup', () => { start = null; });
  file.addEventListener('dragstart', (e) => e.preventDefault());

  // demo din browser (?demo&download), fără Rust
  window.addEventListener('downloads-demo', (e) => { enabled = true; apply(e.detail); });
  window.addEventListener('download-done-demo', (e) => { enabled = true; done(e.detail); });
  if (!tauri) return;

  tauri.event.listen('downloads', (e) => enabled && apply(e.payload));
  tauri.event.listen('download-done', (e) => enabled && done(e.payload));
  window.addEventListener('island-settings', (e) => {
    const on = e.detail.modules.downloads !== false;
    renderPill(); // aceeași stare: doar textele (limba)
    if (on === enabled) return;
    enabled = on;
    if (!on) { apply([]); island.dropTransients('download'); }
  });
}
