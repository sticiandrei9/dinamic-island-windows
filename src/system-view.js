import { ICONS } from './icons.js';
import { t } from './lib/i18n.js';
import { Spring } from './lib/spring.js';
import { formatBytes, formatRate, level } from './lib/format.js';

const CIRC = 163.4; // circumferința inelului (r = 26)
const WARN_PCT = 80;
const WARN_TEMP = 85;
const el = (tag, cls, text) => Object.assign(document.createElement(tag), { className: cls ?? '', textContent: text ?? '' });
const svgEl = (tag, attrs) => {
  const n = document.createElementNS('http://www.w3.org/2000/svg', tag);
  for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, v);
  return n;
};

// Fila Sistem: patru inele (procesor, RAM, video, temperatură) și rândul de rețea.
// Rust citește senzorii doar cât `system_watch(true)` e activ, adică cât fila e deschisă.
export function init(island) {
  const { invoke, tauri } = island;
  const demo = !tauri && new URLSearchParams(location.search).has('system'); // ?demo&system: valori inventate
  const pane = document.querySelector('.pane[data-pane="system"]');
  if (!pane) return;

  const rings = el('div', 'sys-rings');
  const tiles = ['cpu', 'ram', 'gpu', 'temp'].map((key, i) => {
    const root = el('div', 'sys-tile');
    root.style.setProperty('--i', i);
    root.hidden = true;
    const ring = el('div', 'sys-ring');
    const svg = svgEl('svg', { viewBox: '0 0 60 60', fill: 'none', 'stroke-width': 5 });
    const fg = svgEl('circle', { class: 'fg', cx: 30, cy: 30, r: 26 });
    svg.append(svgEl('circle', { class: 'bg', cx: 30, cy: 30, r: 26 }), fg);
    const val = el('div', 'sys-val');
    ring.append(svg, val);
    const label = el('div', 'sys-label', t(`system.${key}`));
    label.dataset.i18n = `system.${key}`;
    const sub = el('div', 'sys-sub');
    root.append(ring, label, sub);
    rings.append(root);
    return { key, root, fg, val, sub, spring: new Spring(0, { response: 0.5, damping: 0.72 }) };
  });
  const net = el('div', 'sys-net');
  net.hidden = true;
  const down = el('span');
  const up = el('span');
  const ping = el('span', 'sys-ping');
  net.append(down, up, ping);
  const box = el('div', 'sys');
  box.append(rings, net);
  pane.append(box);

  // ---------- animație ----------
  let raf = 0;
  let last = 0;
  function frame(now) {
    const dt = Math.min(0.05, (now - last) / 1000);
    last = now;
    let settled = true;
    for (const tile of tiles) {
      if (tile.root.hidden) continue;
      settled = tile.spring.step(dt) && settled;
      const v = Math.min(100, Math.max(0, tile.spring.value));
      tile.fg.style.strokeDashoffset = CIRC * (1 - v / 100);
      tile.fg.style.strokeOpacity = v < 0.5 ? 0 : 1; // capătul rotunjit al unui arc gol ar arăta ca un punct
    }
    raf = settled ? 0 : requestAnimationFrame(frame);
  }
  function kick() {
    if (raf) return;
    last = performance.now();
    raf = requestAnimationFrame(frame);
  }

  // ---------- randare ----------
  const gb = (n) => formatBytes(n).replace(/ GB$/, ''); // „10,9 / 16,9 GB”: unitatea o dată

  function rowsOf(s) {
    const ramPct = s.ram_total ? (s.ram_used / s.ram_total) * 100 : null;
    const temps = [[t('system.tempCpu'), s.cpu_temp], [t('system.tempGpu'), s.gpu_temp]].filter(([, v]) => v != null);
    const hottest = temps.length ? Math.max(...temps.map(([, v]) => v)) : null;
    return {
      cpu: s.cpu == null ? null : { v: s.cpu, text: `${Math.round(s.cpu)}%`, sub: '', warn: level(s.cpu, WARN_PCT) },
      ram: ramPct == null ? null : { v: ramPct, text: `${Math.round(ramPct)}%`, sub: `${gb(s.ram_used)} / ${formatBytes(s.ram_total)}`, warn: level(ramPct, WARN_PCT) },
      gpu: s.gpu == null ? null : { v: s.gpu, text: `${Math.round(s.gpu)}%`, sub: '', warn: level(s.gpu, WARN_PCT) },
      temp: hottest == null ? null : { v: hottest, text: `${Math.round(hottest)}°`, sub: temps.map(([n, v]) => `${n} ${Math.round(v)}°`).join(' · '), warn: level(hottest, WARN_TEMP) },
    };
  }

  function render(s) {
    const rows = rowsOf(s);
    for (const tile of tiles) {
      const row = rows[tile.key];
      tile.root.hidden = !row; // senzor lipsă: rândul dispare, fără text de eroare
      if (!row) continue;
      tile.spring.setTarget(row.v);
      tile.val.textContent = row.text;
      tile.sub.textContent = row.sub;
      tile.root.classList.toggle('warn', row.warn === 'warn');
    }
    down.textContent = `↓ ${formatRate(s.net_down)}`;
    up.textContent = `↑ ${formatRate(s.net_up)}`;
    ping.textContent = s.ping == null ? '—' : t('system.ping', { n: s.ping });
    net.hidden = false;
    kick();
  }

  // ---------- sursa de date ----------
  let shown = false;
  let demoTimer = 0;
  const watch = (on) => invoke('system_watch', { on }).catch(() => {});

  tauri?.event.listen('system', (e) => shown && render(e.payload));

  function onShow() {
    shown = true;
    for (const tile of tiles) {
      tile.spring.value = tile.spring.target = tile.spring.velocity = 0;
      tile.root.hidden = true;
    }
    net.hidden = true;
    watch(true);
    if (demo) {
      let n = 0;
      const tick = () => {
        n++;
        const busy = n % 15 > 6;
        render({
          cpu: 30 + 25 * Math.sin(n / 3) + (n % 12 > 8 ? 30 : 0), ram_used: 10.9e9, ram_total: 16.9e9,
          gpu: busy ? 35 + (n % 7) * 9 : 2, gpu_temp: busy ? 78 + (n % 15) : null, cpu_temp: null,
          net_down: Math.abs(Math.sin(n / 2)) * 4.2e6, net_up: 120_000 + (n % 5) * 30_000, ping: n % 9 === 8 ? null : 18 + (n % 4),
        });
      };
      tick();
      demoTimer = setInterval(tick, 1000);
    }
  }

  function onHide() {
    shown = false;
    clearInterval(demoTimer);
    watch(false);
  }

  island.registerPane('system', { module: 'system', icon: ICONS.system, title: 'tab.system', onShow, onHide });
  // modulul oprit cât fila e deschisă: Rust se oprește singur, dar contorul ferestrei se curăță aici
  window.addEventListener('island-settings', (e) => { if (e.detail.modules.system === false && shown) onHide(); });
}
