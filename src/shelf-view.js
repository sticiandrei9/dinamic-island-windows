import { ICONS } from './icons.js';

const el = (tag, cls, text) => Object.assign(document.createElement(tag), { className: cls ?? '', textContent: text ?? '' });

// Raftul de fișiere (ca „Tray” la NotchNook): tragi fișiere pe insulă, apoi le tragi de aici în orice aplicație.
export function init(island) {
  const { tauri, invoke } = island;
  if (!tauri) return;
  const self = tauri.webviewWindow.getCurrentWebviewWindow();
  let enabled = true;
  let items = [];

  const style = document.createElement('style');
  style.textContent = `
    .sh-head { display: flex; align-items: center; justify-content: space-between; margin-bottom: 8px; font-size: 12px; color: var(--fg2); }
    .sh-grid { display: flex; gap: 8px; overflow-x: auto; padding: 2px 2px 6px; scrollbar-width: thin; }
    .sh-card { position: relative; flex: none; width: 70px; display: flex; flex-direction: column; align-items: center; gap: 4px; padding: 6px 4px; border-radius: 10px; cursor: grab; transition: background .15s, transform .2s cubic-bezier(.2,1.4,.4,1); }
    .sh-card:hover { background: rgba(255,255,255,.08); }
    .sh-card:active { transform: scale(.94); }
    .sh-card img, .sh-card .sh-ph { width: 38px; height: 38px; }
    .sh-ph { display: grid; place-items: center; color: var(--fg2); }
    .sh-name { font-size: 10.5px; line-height: 1.25; text-align: center; width: 100%; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; word-break: break-all; }
    .sh-card.missing { opacity: .4; }
    .sh-x { position: absolute; top: 1px; right: 3px; width: 16px; height: 16px; border-radius: 50%; border: 0; background: #3a3a3c; color: #fff; font-size: 11px; line-height: 16px; display: none; cursor: pointer; }
    .sh-card:hover .sh-x { display: block; }
    .sh-empty, .sh-drop { border: 1.5px dashed rgba(255,255,255,.22); border-radius: 14px; height: 92px; display: grid; place-items: center; color: var(--fg2); font-size: 12.5px; }
    .sh-drop { display: none; border-color: var(--green); color: var(--green); animation: sh-pulse 1.2s ease-in-out infinite; }
    body.drop-target .sh-drop { display: grid; }
    body.drop-target .sh-grid, body.drop-target .sh-empty { display: none; }
    @keyframes sh-pulse { 50% { border-color: rgba(48,209,88,.35); } }
    .sh-menu { position: absolute; z-index: 5; background: #2c2c2e; border-radius: 9px; padding: 4px; box-shadow: 0 6px 20px rgba(0,0,0,.5); }
    .sh-menu button { display: block; width: 100%; text-align: left; border: 0; background: none; color: #fff; font: inherit; font-size: 12px; padding: 5px 10px; border-radius: 6px; cursor: pointer; }
    .sh-menu button:hover { background: rgba(255,255,255,.1); }
  `;
  document.head.append(style);

  const pane = document.querySelector('.pane[data-pane="shelf"]');
  pane.style.position = 'relative';
  const head = el('div', 'sh-head');
  const count = el('span');
  const clear = el('button', 'pill-btn', 'Golește');
  clear.style.padding = '3px 10px';
  head.append(count, clear);
  const grid = el('div', 'sh-grid');
  const empty = el('div', 'sh-empty', 'Trage fișiere pe insulă ca să le ții la îndemână');
  const drop = el('div', 'sh-drop', 'Lasă aici');
  pane.append(head, grid, empty, drop);

  let menu = null;
  const closeMenu = () => { menu?.remove(); menu = null; };

  function render() {
    count.textContent = items.length ? `${items.length} ${items.length === 1 ? 'element' : 'elemente'}` : 'Raft';
    clear.style.visibility = items.length ? 'visible' : 'hidden';
    empty.style.display = items.length ? 'none' : '';
    grid.style.display = items.length ? '' : 'none';
    grid.replaceChildren(...items.map(card));
    island.setBadge('shelf', items.length ? String(items.length) : '');
    island.refresh();
  }

  function card(it) {
    const c = el('div', `sh-card${it.exists ? '' : ' missing'}`);
    c.title = it.exists ? it.path : 'Fișier mutat sau șters';
    if (it.icon) c.append(Object.assign(document.createElement('img'), { src: it.icon, alt: '', draggable: false }));
    else { const ph = el('div', 'sh-ph'); ph.innerHTML = ICONS.shelf; c.append(ph); }
    c.append(el('div', 'sh-name', it.name));
    const x = el('button', 'sh-x', '×');
    x.addEventListener('click', (e) => { e.stopPropagation(); invoke('shelf_remove_path', { path: it.path }); });
    c.append(x);

    // tragere afară: după câțiva pixeli de mișcare, pornește drag-ul nativ
    let start = null;
    c.addEventListener('pointerdown', (e) => { if (e.button === 0 && it.exists) start = { x: e.clientX, y: e.clientY }; });
    c.addEventListener('pointermove', (e) => {
      if (!start || Math.hypot(e.clientX - start.x, e.clientY - start.y) < 5) return;
      start = null;
      invoke('shelf_drag_out', { path: it.path }).catch(() => {});
    });
    c.addEventListener('pointerup', () => { start = null; });
    c.addEventListener('dragstart', (e) => e.preventDefault());
    c.addEventListener('click', (e) => {
      e.stopPropagation();
      closeMenu();
      if (!it.exists) { invoke('shelf_remove_path', { path: it.path }); return; }
      invoke('shelf_open', { path: it.path });
      island.collapse();
    });
    c.addEventListener('contextmenu', (e) => {
      e.preventDefault();
      e.stopPropagation();
      closeMenu();
      menu = el('div', 'sh-menu');
      const reveal = el('button', '', 'Arată în Explorer');
      const remove = el('button', '', 'Scoate de pe raft');
      reveal.addEventListener('click', (ev) => { ev.stopPropagation(); closeMenu(); invoke('shelf_reveal', { path: it.path }); island.collapse(); });
      remove.addEventListener('click', (ev) => { ev.stopPropagation(); closeMenu(); invoke('shelf_remove_path', { path: it.path }); });
      menu.append(reveal, remove);
      const r = pane.getBoundingClientRect();
      const z = r.width / pane.offsetWidth || 1; // zoom-ul insulei
      menu.style.left = `${(e.clientX - r.left) / z}px`;
      menu.style.top = `${(e.clientY - r.top) / z}px`;
      pane.append(menu);
    });
    return c;
  }

  async function refresh() {
    items = await invoke('shelf_list').catch(() => []);
    render();
  }

  clear.addEventListener('click', (e) => { e.stopPropagation(); invoke('shelf_clear'); });
  document.addEventListener('click', closeMenu);
  island.registerPane('shelf', { module: 'shelf', icon: ICONS.shelf, title: 'Raft', onShow: refresh, onHide: closeMenu });
  tauri.event.listen('shelf-changed', refresh);

  // fișiere trase peste insulă
  self.onDragDropEvent((e) => {
    if (!enabled) return;
    const { type, paths } = e.payload;
    if (type === 'enter') {
      document.body.classList.add('drop-target');
      island.expandTo('shelf');
    } else if (type === 'drop') {
      document.body.classList.remove('drop-target');
      if (paths?.length) invoke('shelf_add_paths', { paths });
    } else if (type === 'leave') {
      document.body.classList.remove('drop-target');
    }
  });

  window.addEventListener('island-settings', (e) => { enabled = e.detail.modules.shelf !== false; });
  refresh();
}
