import { ICONS } from './icons.js';
import { t } from './lib/i18n.js';
import { formatClock, formatStopwatch, remainingMs, elapsedMs } from './lib/timer.js';

const PRESETS = [1, 5, 10, 25];
const el = (tag, cls, text) => Object.assign(document.createElement(tag), { className: cls ?? '', textContent: text ?? '' });
// buton cu text tradus (actualizat de applyStatic la schimbarea limbii)
const btn = (cls, key) => { const b = el('button', cls, t(key)); b.dataset.i18n = key; return b; };

export function init(island) {
  const { invoke, tauri } = island;
  if (!tauri) return;
  let clock = { timer: null, stopwatch: null, finished: false };
  let minutes = 5;
  let enabled = true;
  const cmd = (action, seconds) => invoke('timer_command', { action, seconds }).catch(() => {});

  // ---------- fila Timer ----------
  const pane = document.querySelector('.pane[data-pane="timer"]');
  const style = document.createElement('style');
  style.textContent = `
    .tm { display: flex; flex-direction: column; align-items: center; gap: 10px; }
    .tm-big { font-size: 40px; font-weight: 600; letter-spacing: -1px; font-variant-numeric: tabular-nums; color: var(--orange); cursor: ns-resize; }
    .tm-big.idle { color: #fff; }
    .tm-row { display: flex; gap: 6px; justify-content: center; }
    .tm-row .pill-btn { white-space: nowrap; }
    .tm-sep { width: 100%; height: 1px; background: rgba(255,255,255,.08); margin: 2px 0; }
    .sw-big { font-size: 26px; font-weight: 600; font-variant-numeric: tabular-nums; color: var(--green); }
    .sw-laps { display: flex; gap: 10px; flex-wrap: wrap; justify-content: center; font-size: 11px; color: var(--fg2); font-variant-numeric: tabular-nums; min-height: 0; }
    .tm-step { width: 28px; padding: 6px 0; }
    .tm-phase { font-size: 12px; font-weight: 600; color: var(--orange); margin-bottom: -6px; }
    .tm-phase.rest, .tm-big.rest { color: var(--green); }
  `;
  document.head.append(style);

  const box = el('div', 'tm');
  const phase = el('div', 'tm-phase');
  const big = el('div', 'tm-big idle');
  const presets = el('div', 'tm-row');
  for (const m of PRESETS) {
    const b = el('button', 'pill-btn', t('timer.preset', { n: m }));
    b.dataset.min = m;
    b.addEventListener('click', (e) => { e.stopPropagation(); minutes = m; cmd('start', m * 60); });
    presets.append(b);
  }
  const pomoBtn = btn('pill-btn orange', 'pomo.button');
  pomoBtn.addEventListener('click', (e) => { e.stopPropagation(); cmd('pomodoro'); });
  presets.append(pomoBtn);
  const minus = el('button', 'pill-btn tm-step', '−');
  const plus = el('button', 'pill-btn tm-step', '+');
  const startBtn = el('button', 'pill-btn orange', 'Start');
  const pauseBtn = btn('pill-btn orange', 'timer.pause');
  const cancelBtn = btn('pill-btn', 'timer.cancel');
  const controls = el('div', 'tm-row');
  controls.append(minus, startBtn, pauseBtn, cancelBtn, plus);

  const swBig = el('div', 'sw-big');
  const swStart = el('button', 'pill-btn green', 'Start');
  const swLap = btn('pill-btn', 'sw.lap');
  const swReset = btn('pill-btn', 'sw.reset');
  const swRow = el('div', 'tm-row');
  swRow.append(swStart, swLap, swReset);
  const laps = el('div', 'sw-laps');

  box.append(phase, big, presets, controls, el('div', 'tm-sep'), swBig, swRow, laps);
  pane.prepend(box); // rândul „Somn” (sleep-view.js) vine după controalele timerului

  const stop = (fn) => (e) => { e.stopPropagation(); fn(); };
  const setMinutes = (m) => { minutes = Math.max(1, Math.min(180, m)); render(); };
  minus.addEventListener('click', stop(() => setMinutes(minutes - 1)));
  plus.addEventListener('click', stop(() => setMinutes(minutes + 1)));
  big.addEventListener('wheel', (e) => { if (!clock.timer) { e.preventDefault(); setMinutes(minutes + (e.deltaY < 0 ? 1 : -1)); } });
  startBtn.addEventListener('click', stop(() => (clock.timer ? cmd('resume') : cmd('start', minutes * 60))));
  pauseBtn.addEventListener('click', stop(() => cmd('pause')));
  cancelBtn.addEventListener('click', stop(() => cmd('cancel')));
  swStart.addEventListener('click', stop(() => cmd(clock.stopwatch?.started_at != null ? 'sw_pause' : 'sw_start')));
  swLap.addEventListener('click', stop(() => cmd('sw_lap')));
  swReset.addEventListener('click', stop(() => cmd('sw_reset')));

  // după un timer terminat, un click pe pastilă oprește pulsarea
  document.getElementById('compact').addEventListener('click', () => clock.finished && cmd('dismiss'));

  island.registerPane('timer', { module: ['timer', 'sleep'], icon: ICONS.timer, title: 'tab.timer', onShow: () => { if (clock.finished) cmd('dismiss'); render(); } });

  // ---------- randare ----------
  function render() {
    const now = Date.now();
    const tm = clock.timer;
    const running = tm?.ends_at != null;
    big.textContent = tm ? formatClock(remainingMs(tm, now)) : clock.finished ? t('timer.done') : formatClock(minutes * 60_000);
    big.classList.toggle('idle', !tm);
    const pomo = tm ? clock.pomodoro : null;
    const rest = pomo && pomo.phase !== 'work';
    phase.style.display = pomo ? '' : 'none';
    if (pomo) phase.textContent = rest ? t(`pomo.${pomo.phase}`) : t('pomo.round', { r: pomo.round, n: 4 });
    phase.classList.toggle('rest', !!rest);
    big.classList.toggle('rest', !!rest);
    presets.style.display = tm ? 'none' : '';
    presets.querySelectorAll('button[data-min]').forEach((b) => { b.textContent = t('timer.preset', { n: b.dataset.min }); });
    minus.style.display = plus.style.display = tm ? 'none' : '';
    startBtn.style.display = !tm || !running ? '' : 'none';
    startBtn.textContent = t(tm ? 'timer.resume' : 'timer.start');
    pauseBtn.style.display = running ? '' : 'none';
    cancelBtn.style.display = tm ? '' : 'none';

    const sw = clock.stopwatch;
    swBig.textContent = formatStopwatch(elapsedMs(sw, now));
    swStart.textContent = t(sw?.started_at != null ? 'timer.pause' : sw ? 'timer.resume' : 'timer.start');
    swLap.disabled = sw?.started_at == null;
    swReset.disabled = !sw;
    laps.replaceChildren(...(sw?.laps ?? []).map((l, i) => el('span', '', t('sw.lapItem', { n: (sw.lap_total ?? sw.laps.length) - sw.laps.length + i + 1, v: formatStopwatch(l) }))));

    // textul din pastilă, pentru activitatea care o ocupă
    const kind = document.getElementById('compact').dataset.kind;
    const icon = document.getElementById('c-timer-icon');
    const text = document.getElementById('c-timer-text');
    if (kind === 'timer') {
      icon.innerHTML = ICONS.hourglass;
      text.textContent = tm ? formatClock(remainingMs(tm, now)) : t('timer.done');
    } else if (kind === 'stopwatch') {
      icon.innerHTML = ICONS.stopwatch;
      text.textContent = formatStopwatch(elapsedMs(sw, now)).slice(0, -2);
    }
  }

  function syncActivities() {
    const now = Date.now();
    const tm = clock.timer;
    if (enabled && (tm || clock.finished)) {
      island.setActivity('timer', { kind: 'timer', progress: tm ? remainingMs(tm, now) / tm.total_ms : 0 });
    } else island.setActivity('timer', null);
    const sw = clock.stopwatch;
    if (enabled && sw) island.setActivity('stopwatch', { kind: 'stopwatch', progress: (elapsedMs(sw, now) % 60_000) / 60_000 });
    else island.setActivity('stopwatch', null);
    document.body.classList.toggle('timer-done', enabled && clock.finished);
    render();
  }

  function apply(state, fromEvent) {
    const wasFinished = clock.finished;
    const wasPhase = clock.pomodoro?.phase;
    clock = state;
    // Pomodoro: etapa următoare a pornit singură
    const p = state.pomodoro;
    if (fromEvent && enabled && p && wasPhase && p.phase !== wasPhase) {
      island.pushTransient({ kind: 'alert', icon: 'timer', title: t(p.phase === 'work' ? 'pomo.backToWork' : `pomo.${p.phase}`), detail: t('timer.preset', { n: Math.round(state.timer.total_ms / 60_000) }), color: p.phase === 'work' ? 'var(--orange)' : 'var(--green)', duration: 4000 });
    }
    if (fromEvent && enabled && state.finished && !wasFinished) {
      island.pushTransient({ kind: 'alert', icon: 'timer', title: t('timer.finished'), color: 'var(--orange)', duration: 3000 });
    }
    syncActivities();
  }

  tauri.event.listen('timer-state', (e) => apply(e.payload, true));
  invoke('get_timer_state').then((s) => apply(s, false)).catch(() => {});
  window.addEventListener('island-settings', (e) => { enabled = e.detail.modules.timer !== false; box.style.display = enabled ? '' : 'none'; syncActivities(); });

  // cât rulează ceva, actualizăm textele (și inelul bulei) de câteva ori pe secundă
  setInterval(() => {
    if (clock.timer?.ends_at != null || clock.stopwatch?.started_at != null) syncActivities();
  }, 200);
}
