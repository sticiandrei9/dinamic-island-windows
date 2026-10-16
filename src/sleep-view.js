import { ICONS } from './icons.js';
import { t } from './lib/i18n.js';
import { formatClock, sleepProgress } from './lib/timer.js';

const PRESETS = [15, 30, 60];
const RING = 56.5; // lungimea cercului din pastilă (r = 9)
const el = (tag, cls, text) => Object.assign(document.createElement(tag), { className: cls ?? '', textContent: text ?? '' });

// Timerul de somn (Rust: `Clock.sleep` în `timer-state`): rândul „Somn” din fila Timer, activitatea `sleep`
// (lună + inel care se golește) și tranzitoria „Muzica s-a oprit” la final (`sleep-done`).
export function init(island) {
  const { invoke, tauri } = island;
  const $ = (id) => document.getElementById(id);
  $('c-sleep-icon').innerHTML = ICONS.moon;
  $('b-moon').innerHTML = ICONS.moon;
  let sleep = null; // { ends_at, total_ms, start_volume }
  let enabled = true;
  let timerOn = true; // cu modulul Timer oprit, în fila Timer rămâne doar acest rând
  let tick = 0;

  // ---------- rândul din fila Timer ----------
  const row = el('div', 'sl-row');
  const label = el('span', 'sl-label');
  label.innerHTML = ICONS.moon; // pictogramă constantă; textul se pune cu textContent
  const labelText = el('span');
  label.append(labelText);
  const presets = PRESETS.map((m) => {
    const b = el('button', 'pill-btn indigo');
    b.dataset.min = m;
    b.addEventListener('click', (e) => { e.stopPropagation(); invoke('timer_command', { action: 'sleep_start', seconds: m * 60 }).catch(() => {}); });
    return b;
  });
  const left = el('span', 'sl-left');
  const cancel = el('button', 'pill-btn');
  cancel.addEventListener('click', (e) => { e.stopPropagation(); invoke('timer_command', { action: 'sleep_cancel' }).catch(() => {}); });
  row.append(label, ...presets, left, cancel);
  document.querySelector('.pane[data-pane="timer"]').append(row);

  function render() {
    const now = Date.now();
    row.hidden = !enabled;
    row.classList.toggle('alone', !timerOn);
    labelText.textContent = t('sleep.label');
    presets.forEach((b) => { b.textContent = t('timer.preset', { n: b.dataset.min }); b.style.display = sleep ? 'none' : ''; });
    left.style.display = cancel.style.display = sleep ? '' : 'none';
    cancel.textContent = t('timer.cancel');
    if (!sleep) return;
    left.textContent = formatClock(sleep.ends_at - now);
    $('c-sleep-text').textContent = left.textContent;
    $('c-sleep-ring').querySelector('.fg').style.strokeDashoffset = `${RING * (1 - sleepProgress(sleep, now))}`;
  }

  function sync() {
    const active = enabled && !!sleep;
    island.setActivity('sleep', active ? { kind: 'sleep', progress: sleepProgress(sleep, Date.now()) } : null);
    clearInterval(tick);
    if (active) tick = setInterval(sync, 1000); // inelul se golește o dată pe secundă
    render();
  }

  function apply(next) {
    sleep = next ?? null;
    sync();
  }

  // demo din browser (?demo&sleep), fără Rust: timer-view nu rulează acolo, deci fila Timer se înregistrează aici
  window.addEventListener('sleep-demo', (e) => {
    island.registerPane('timer', { module: ['timer', 'sleep'], icon: ICONS.timer, title: 'tab.timer', onShow: render });
    apply(e.detail);
  });
  window.addEventListener('island-settings', (e) => {
    enabled = e.detail.modules.sleep !== false;
    timerOn = e.detail.modules.timer !== false;
    sync();
  });
  render();
  if (!tauri) return;

  tauri.event.listen('timer-state', (e) => apply(e.payload.sleep));
  invoke('get_timer_state').then((s) => apply(s.sleep)).catch(() => {});
  tauri.event.listen('sleep-done', () => {
    if (!enabled) return;
    island.pushTransient({ kind: 'alert', icon: 'moon', title: t('sleep.stopped'), color: 'var(--indigo)', duration: 4000 });
  });
}
