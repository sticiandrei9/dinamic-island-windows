import { ICONS } from './icons.js';
import { visibleBatteries } from './lib/batteries.js';

const LOW = 20; // sub acest procent chipul devine roșu
const SVG = 'http://www.w3.org/2000/svg';

function ring(pct) {
  const s = document.createElementNS(SVG, 'svg');
  s.setAttribute('viewBox', '0 0 20 20');
  s.classList.add('bat-ring');
  s.innerHTML = `<circle cx="10" cy="10" r="7.5" class="bat-bg"/><circle cx="10" cy="10" r="7.5" class="bat-fg" pathLength="100" stroke-dasharray="${Math.min(100, pct)} 100" transform="rotate(-90 10 10)"/>`;
  return s;
}

// Bateriile dispozitivelor Bluetooth conectate: în dreapta ceasului (fără media) sau, cu media, la capătul rândului cu butoane.
export function init(island) {
  const { invoke, tauri } = island;
  const targets = [document.getElementById('x-batteries'), document.getElementById('x-batteries-m')];
  let enabled = true;
  let list = [];
  let built = '';

  // un singur desen pentru ambele locuri; CSS-ul arată doar locul care se potrivește (cu/fără media)
  function chips() {
    const { shown, more } = visibleBatteries(list);
    const nodes = shown.map((d) => {
      const el = document.createElement('span');
      el.className = d.battery < LOW ? 'bat low' : 'bat';
      el.title = `${d.name} ${d.battery}%`;
      const icon = document.createElement('span');
      icon.className = 'bat-icon';
      icon.innerHTML = ICONS[d.kind === 'speaker' ? 'speakerDevice' : d.kind] ?? ICONS.bluetooth;
      const pct = document.createElement('b');
      pct.textContent = `${d.battery}%`;
      el.append(icon, pct, ring(d.battery));
      return el;
    });
    if (more) {
      const m = document.createElement('span');
      m.className = 'bat-more';
      m.textContent = `+${more}`;
      nodes.push(m);
    }
    return nodes;
  }

  function render() {
    const key = enabled ? JSON.stringify(list.slice(0, 4).map((d) => [d.id, d.kind, d.battery])) + list.length : '';
    if (key === built) return;
    built = key;
    for (const t of targets) t?.replaceChildren(...(enabled ? chips() : []));
    island.refresh();
  }

  function set(next) { list = Array.isArray(next) ? next.filter((d) => Number.isFinite(d.battery)) : []; render(); }

  window.addEventListener('island-settings', (e) => { enabled = e.detail.modules.batteries !== false; render(); });
  window.addEventListener('batteries-demo', (e) => set(e.detail)); // ?demo&batteries
  if (tauri) {
    tauri.event.listen('batteries', (e) => set(e.payload));
    invoke('batteries_list').then(set).catch(() => {});
  }
}
