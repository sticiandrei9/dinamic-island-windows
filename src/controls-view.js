import { ICONS } from './icons.js';
import { t } from './lib/i18n.js';
import { visibleButtons, sleepClick, SLEEP_CONFIRM_MS } from './lib/controls.js';

// Câmpul din starea Rust pentru fiecare buton care se comută; blocarea și sleep sunt acțiuni.
const FIELD = { wifi: 'wifi', bluetooth: 'bluetooth', dark: 'dark', night: 'night', dnd: 'dnd', mic: 'mic_muted' };
const ICON = { wifi: 'wifi', bluetooth: 'bluetooth', dark: 'contrast', night: 'sun', dnd: 'moon', mic: 'mic', lock: 'lock', sleep: 'sleep' };

// Centrul de control: rândul rotund de sub ceas (Wi-Fi, Bluetooth, mod întunecat, microfon, blocare, sleep).
export function init(island) {
  const { invoke, tauri } = island;
  const row = document.getElementById('x-controls');
  let enabled = true;
  let demo = false; // ?demo&controls: starea e locală, fără Rust
  let state = null;
  let built = ''; // lista de butoane desenată; rândul se reface doar când se schimbă (altfel intrarea ar reporni la 2 s)
  let armedAt = 0;
  let disarm = 0;
  const buttons = new Map();
  const log = (msg) => tauri && invoke('log_js', { msg: `centru de control: ${msg}` }).catch(() => {});

  function paint(name, b) {
    const field = FIELD[name];
    const on = field ? state?.[field] === true : false;
    b.classList.toggle('on', on);
    b.classList.toggle('armed', name === 'sleep' && armedAt > 0);
    b.setAttribute('aria-pressed', field ? String(on) : 'false');
    b.innerHTML = name === 'wifi' && !on ? ICONS.wifiOff : name === 'mic' && on ? ICONS.micOff : ICONS[ICON[name]];
    const label = name === 'sleep' && armedAt > 0 ? t('controls.sleepConfirm') : t(`controls.${name}`);
    b.title = label;
    b.setAttribute('aria-label', label);
  }

  function render() {
    const names = enabled ? visibleButtons(state) : [];
    row.hidden = !enabled || !state;
    const key = names.join();
    if (key !== built) {
      built = key;
      buttons.clear();
      row.replaceChildren(...names.map((name, i) => {
        const b = Object.assign(document.createElement('button'), { className: 'ctl' });
        b.dataset.name = name;
        b.style.animationDelay = `${i * 40}ms`;
        b.addEventListener('click', (e) => { e.stopPropagation(); click(name); });
        buttons.set(name, b);
        return b;
      }));
      island.refresh();
    }
    for (const [name, b] of buttons) paint(name, b);
  }

  async function refresh() {
    if (!enabled || demo) return;
    state = await invoke('controls_state').catch(() => state);
    render();
  }

  async function click(name) {
    if (name === 'sleep') {
      const s = sleepClick(armedAt, Date.now());
      armedAt = s.armedAt;
      clearTimeout(disarm);
      if (armedAt > 0) disarm = setTimeout(() => { armedAt = 0; render(); }, SLEEP_CONFIRM_MS);
      render();
      if (s.fire && !demo) invoke('controls_action', { name }).catch((err) => log(`${name}: ${err}`));
      return;
    }
    if (name === 'lock') {
      if (!demo) invoke('controls_action', { name }).catch((err) => log(`${name}: ${err}`));
      return;
    }
    const field = FIELD[name];
    const next = !(state?.[field] === true);
    if (demo) state = { ...state, [field]: next };
    else state = await invoke('controls_set', { name, on: next }).catch((err) => { log(`${name}: ${err}`); return state; });
    render();
  }

  window.addEventListener('home-show', refresh);
  setInterval(() => island.expanded && document.body.dataset.tab === 'home' && refresh(), 2000);
  window.addEventListener('island-settings', (e) => { enabled = e.detail.modules.controls !== false; if (state) render(); else refresh(); });
  window.addEventListener('controls-demo', (e) => { demo = true; enabled = true; state = e.detail; render(); });
}
