import { ICONS } from './icons.js';
import { t } from './lib/i18n.js';
import { clipPreview } from './lib/format.js';

const el = (tag, cls, text) => Object.assign(document.createElement(tag), { className: cls ?? '', textContent: text ?? '' });

// Fila Clipboard: ultimele 5 texte copiate (le ține Rust, doar în memorie), click = copiază înapoi.
// La fiecare copiere venită din afara insulei apare o tranzitorie scurtă „Copiat” cu începutul textului.
export function init(island) {
  const { invoke, tauri } = island;
  const pane = document.querySelector('.pane[data-pane="clipboard"]');
  if (!pane) return;

  const head = el('div', 'cb-head');
  const clear = el('button', 'pill-btn', t('clip.clear'));
  clear.dataset.i18n = 'clip.clear';
  clear.style.padding = '3px 10px';
  clear.addEventListener('click', () => invoke('clipboard_clear'));
  head.append(clear);
  const list = el('div', 'cb-list');
  const empty = el('div', 'cb-empty', t('clip.empty'));
  empty.dataset.i18n = 'clip.empty';
  pane.append(head, list, empty);

  let items = [];
  let shown = false;
  let seen = new Set(); // rândurile deja afișate nu mai intră cu animație la fiecare reîmprospătare
  let flash = null; // textul pe care tocmai s-a dat click: rândul lui arată „Copiat” o secundă
  let flashTimer = 0;

  function render() {
    clear.style.visibility = items.length ? 'visible' : 'hidden';
    empty.style.display = items.length ? 'none' : '';
    list.replaceChildren(...items.map((text, i) => {
      const row = el('div', `cb-item${seen.has(text) ? '' : ' in'}${text === flash ? ' done' : ''}`);
      row.style.setProperty('--i', i);
      row.append(el('div', 'cb-text', text), el('span', 'cb-ok', t('clip.copied')));
      row.addEventListener('click', () => copy(i, text));
      return row;
    }));
    seen = new Set(items);
    island.refresh();
  }

  function copy(index, text) {
    flash = text;
    clearTimeout(flashTimer);
    flashTimer = setTimeout(() => { flash = null; }, 1000);
    // în aplicație, lista reordonată sosește prin evenimentul `clipboard`; în demo nu există Rust, deci redesenăm direct
    invoke('clipboard_copy', { index }).then(() => { if (!tauri) render(); }).catch(() => { flash = null; });
  }

  function apply(p) {
    items = p.items ?? [];
    if (p.new && items[0]) {
      island.dropTransients('clip'); // copieri rapide: doar ultima, nu o coadă de câte 2 s
      island.pushTransient({ kind: 'clip', icon: 'copy', title: t('clip.copied'), detail: clipPreview(items[0]), duration: 2000 });
    }
    if (shown) render();
  }

  tauri?.event.listen('clipboard', (e) => apply(e.payload));
  window.addEventListener('clipboard-demo', (e) => apply(e.detail)); // ?demo&clipboard: copieri inventate

  function onShow() {
    shown = true;
    seen = new Set(); // la deschiderea filei toate rândurile intră pe rând
    render();
    // o fereastră creată târziu (monitor nou) n-a primit evenimentele de până acum
    invoke('clipboard_list').then((l) => { if (Array.isArray(l) && l.join('\0') !== items.join('\0')) { items = l; render(); } }).catch(() => {});
  }

  island.registerPane('clipboard', { module: 'clipboard', icon: ICONS.copy, title: 'tab.clipboard', onShow, onHide: () => { shown = false; } });
}
