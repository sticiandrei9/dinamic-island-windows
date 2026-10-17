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

  function update(el, d) {
    el.classList.toggle('low', d.battery < LOW);
    el.title = `${d.name} ${d.battery}%`;
    el.querySelector('b').textContent = `${d.battery}%`;
    el.querySelector('.bat-fg').setAttribute('stroke-dasharray', `${Math.min(100, d.battery)} 100`);
  }

  function chip(d) {
    const el = document.createElement('span');
    el.className = 'bat';
    const icon = document.createElement('span');
    icon.className = 'bat-icon';
    icon.innerHTML = ICONS[d.kind === 'speaker' ? 'speakerDevice' : d.kind] ?? ICONS.bluetooth;
    el.append(icon, document.createElement('b'), ring(d.battery));
    update(el, d);
    return el;
  }

  // un singur desen pentru ambele locuri; CSS-ul arată doar locul care se potrivește (cu/fără media)
  function chips(shown, more) {
    const nodes = shown.map(chip);
    if (more) {
      const m = document.createElement('span');
      m.className = 'bat-more';
      m.textContent = `+${more}`;
      nodes.push(m);
    }
    return nodes;
  }

  function render() {
    const { shown, more } = visibleBatteries(enabled ? list : []);
    // rândul se reface doar când se schimbă dispozitivele afișate; la schimbarea procentului chipurile se actualizează pe loc
    const key = JSON.stringify([shown.map((d) => [d.id, d.kind]), more]);
    if (key !== built) {
      built = key;
      for (const t of targets) t?.replaceChildren(...chips(shown, more));
      island.refresh();
      return;
    }
    for (const t of targets) shown.forEach((d, i) => t?.children[i] && update(t.children[i], d));
  }

  function set(next) { list = Array.isArray(next) ? next.filter((d) => Number.isFinite(d.battery)) : []; render(); }

  window.addEventListener('island-settings', (e) => { enabled = e.detail.modules.batteries !== false; render(); });
  window.addEventListener('batteries-demo', (e) => set(e.detail)); // ?demo&batteries
  if (tauri) {
    tauri.event.listen('batteries', (e) => set(e.payload));
    invoke('batteries_list').then(set).catch(() => {});
  }
}
