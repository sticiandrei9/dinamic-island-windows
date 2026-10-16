import { ICONS } from './icons.js';
import { t } from './lib/i18n.js';
import { sortNotes, openCount } from './lib/notes.js';

const el = (tag, cls, text) => Object.assign(document.createElement(tag), { className: cls ?? '', textContent: text ?? '' });
const STRIKE_MS = 380; // cât se vede tăierea înainte ca notița bifată să coboare

// Fila Notițe: câmp de scris sus, lista dedesubt (nebifatele întâi). Lista o ține Rust (`notes.json`) și o
// trimite tuturor insulelor prin `notes-changed`. Câmpul are nevoie de focus de tastatură, iar insula nu e
// activabilă: `notes_focus` o activează cât scrii și redă focusul aplicației de dinainte.
export function init(island) {
  const { invoke, tauri } = island;
  const pane = document.querySelector('.pane[data-pane="notes"]');
  if (!pane) return;

  const input = el('input', 'nt-input');
  Object.assign(input, { type: 'text', maxLength: 500, autocomplete: 'off', spellcheck: false });
  const list = el('div', 'nt-list');
  const empty = el('div', 'nt-empty');
  const clear = el('button', 'pill-btn');
  clear.style.padding = '3px 10px';
  const foot = el('div', 'nt-foot');
  foot.append(clear);
  pane.append(input, list, empty, foot);

  let items = [];
  let shown = false;
  let seen = new Set(); // rândurile deja afișate nu mai intră cu animație la fiecare reîmprospătare
  let holdUntil = 0; // o notiță tocmai bifată: rândul coboară abia după ce se vede tăierea
  let renderTimer = 0;
  let badge = -1;
  let hotkeyOpen = false; // deschisă cu scurtătura (cursorul poate fi în altă parte)
  let hideTimer = 0;
  let adding = false;

  function texts() {
    input.placeholder = t('notes.placeholder');
    empty.textContent = t('notes.empty');
    clear.textContent = t('notes.clearDone');
  }

  // fără Rust (demo în browser) aceleași operații se fac local, doar ca să se vadă designul
  function call(cmd, args = {}) {
    if (tauri) return invoke(cmd, args);
    if (cmd === 'notes_add') items = [...items, { id: Math.max(0, ...items.map((n) => n.id)) + 1, text: args.text, done: false, at: Math.floor(Date.now() / 1000) }];
    else if (cmd === 'notes_toggle') items = items.map((n) => (n.id === args.id ? { ...n, done: !n.done } : n));
    else if (cmd === 'notes_remove') items = items.filter((n) => n.id !== args.id);
    else if (cmd === 'notes_clear_done') items = items.filter((n) => !n.done);
    else return Promise.resolve();
    apply(items);
    return Promise.resolve();
  }

  function row(n, i) {
    const r = el('div', `nt-item${n.done ? ' done' : ''}${seen.has(n.id) ? '' : ' in'}`);
    r.style.setProperty('--i', i);
    const check = el('button', 'nt-check');
    check.title = t('notes.check');
    check.addEventListener('click', (e) => {
      e.stopPropagation();
      if (!n.done) { r.classList.add('done'); holdUntil = Date.now() + STRIKE_MS; }
      call('notes_toggle', { id: n.id }).catch(() => {});
    });
    const del = el('button', 'nt-del', '×');
    del.title = t('notes.delete');
    del.addEventListener('click', (e) => { e.stopPropagation(); call('notes_remove', { id: n.id }).catch(() => {}); });
    const body = el('div', 'nt-body');
    body.append(el('span', 'nt-text', n.text));
    r.append(check, body, del);
    return r;
  }

  function render() {
    const sorted = sortNotes(items);
    empty.style.display = sorted.length ? 'none' : '';
    clear.hidden = !sorted.some((n) => n.done);
    list.replaceChildren(...sorted.map(row));
    seen = new Set(sorted.map((n) => n.id));
    island.refresh();
  }

  function apply(next) {
    items = next ?? [];
    const open = openCount(items);
    if (open !== badge) { badge = open; island.setBadge('notes', open ? String(open) : ''); }
    if (!shown) return;
    clearTimeout(renderTimer);
    const wait = holdUntil - Date.now();
    if (wait > 0) renderTimer = setTimeout(render, wait);
    else render();
  }

  // ---------- focus ----------
  const grab = () => invoke('notes_focus', { on: true }).catch(() => {});
  function release() {
    invoke('notes_focus', { on: false }).catch(() => {});
    // deschisă cu scurtătura și cu mouse-ul în altă parte, insula nu mai are cine s-o strângă
    clearTimeout(hideTimer);
    if (hotkeyOpen && !island.hovering) hideTimer = setTimeout(() => { if (!island.hovering) island.collapse(); }, 1500);
  }
  input.addEventListener('focus', grab);
  // fereastra nu e activă (NOACTIVATE), deci `focus` nu vine la click: activăm la apăsare (al doilea grab e inofensiv)
  input.addEventListener('pointerdown', grab);
  input.addEventListener('blur', release);
  input.addEventListener('keydown', (e) => {
    e.stopPropagation();
    if (e.key === 'Enter' && !e.isComposing) {
      e.preventDefault();
      const text = input.value.trim();
      if (!text) { input.blur(); return; }
      if (adding) return; // al doilea Enter cât salvarea nu s-a încheiat n-are voie să dubleze notița
      adding = true;
      // textul rămâne în câmp până la succes; la eroare (salvare eșuată) rămâne pentru o nouă încercare
      call('notes_add', { text })
        .then(() => { input.value = ''; input.blur(); })
        .catch(() => {})
        .finally(() => { adding = false; });
    } else if (e.key === 'Escape') {
      e.preventDefault();
      input.blur();
      if (hotkeyOpen) island.collapse();
    }
  });
  clear.addEventListener('click', (e) => { e.stopPropagation(); call('notes_clear_done').catch(() => {}); });

  function onShow() {
    shown = true;
    seen = new Set(); // la deschiderea filei toate rândurile intră pe rând
    render();
    // o fereastră creată târziu (monitor nou) n-a primit evenimentele de până acum
    if (tauri) invoke('notes_list').then((l) => { if (Array.isArray(l)) apply(l); }).catch(() => {});
  }
  function onHide() {
    shown = false;
    hotkeyOpen = false;
    clearTimeout(hideTimer);
    clearTimeout(renderTimer);
    input.blur();
    invoke('notes_focus', { on: false }).catch(() => {}); // și când câmpul nu mai avea focusul DOM
  }

  texts();
  island.registerPane('notes', { module: 'notes', icon: ICONS.notes, title: 'tab.notes', onShow, onHide });
  window.addEventListener('island-settings', texts);
  window.addEventListener('notes-demo', (e) => apply(e.detail)); // ?demo&notes: notițe inventate

  if (!tauri) return;
  tauri.event.listen('notes-changed', (e) => apply(e.payload));
  invoke('notes_list').then(apply).catch(() => {});
  // scurtătura globală: insula se deschide pe Notițe, cu câmpul gata de scris
  tauri.webviewWindow.getCurrentWebviewWindow().listen('island-notes', () => {
    if (island.settings?.modules?.notes === false) return;
    hotkeyOpen = true;
    clearTimeout(hideTimer);
    island.expandTo('notes');
    input.focus();
    grab(); // evenimentul `focus` nu vine cât fereastra nu e activă
  });
}
