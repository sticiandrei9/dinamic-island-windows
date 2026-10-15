import { ICONS } from './icons.js';
import { t } from './lib/i18n.js';
import { formatTime } from './lib/util.js';

// Punctele de confidențialitate (ca pe iPhone): portocaliu = microfon, verde = cameră.
// Cât microfonul e folosit, pastila arată apelul: aplicația, durata și butonul de mute.
export function init(island) {
  const { tauri, invoke } = island;
  if (!tauri) return;
  const row = document.getElementById('x-privacy');
  const micBtn = document.getElementById('c-mic');
  const callText = document.getElementById('c-call-text');
  let state = null;
  let modules = { privacy: true, calls: true };
  let ticker = 0;

  function part(color, label, apps) {
    const span = document.createElement('span');
    const dot = document.createElement('i');
    dot.style.background = color;
    span.append(dot, `${label}: ${apps.join(', ')}`);
    return span;
  }

  function toggleMute(e) {
    e.stopPropagation();
    const muted = !state?.mic_muted;
    state = { ...state, mic_muted: muted }; // afișare imediată; Rust confirmă în ≤ 1 s
    render();
    invoke('mic_set_mute', { muted })
      .then((m) => { state = { ...state, mic_muted: m }; render(); })
      .catch(() => island.pushTransient({ kind: 'alert', icon: 'mic', title: t('call.failed'), color: 'var(--red)', duration: 2500 }));
  }

  function callTick() {
    const since = state?.mic_since;
    const app = state?.mic?.[0] ?? '';
    callText.textContent = since ? `${app} · ${formatTime(Math.max(0, (Date.now() - since) / 1000))}` : app;
  }

  function render() {
    const mic = state?.mic ?? [];
    const cam = state?.cam ?? [];
    const dots = modules.privacy !== false;
    document.body.classList.toggle('mic-on', dots && mic.length > 0);
    document.body.classList.toggle('cam-on', dots && cam.length > 0);
    document.body.classList.toggle('mic-muted', !!state?.mic_muted);

    const parts = [];
    if (dots && mic.length) parts.push(part('var(--orange)', t('privacy.mic'), mic));
    if (dots && cam.length) parts.push(part('var(--green)', t('privacy.cam'), cam));
    const inCall = modules.calls !== false && mic.length > 0;
    if (inCall && state.mic_muted != null) {
      const b = document.createElement('button');
      b.className = `pill-btn ${state.mic_muted ? 'red' : 'orange'}`;
      b.textContent = t(state.mic_muted ? 'call.unmute' : 'call.mute');
      b.addEventListener('click', toggleMute);
      parts.push(b);
    }
    row.replaceChildren(...parts);
    row.hidden = parts.length === 0;

    micBtn.innerHTML = state?.mic_muted ? ICONS.micOff : ICONS.mic; // pictograme constante
    micBtn.title = t(state?.mic_muted ? 'call.unmute' : 'call.mute');
    micBtn.disabled = state?.mic_muted == null;
    clearInterval(ticker);
    if (inCall) {
      callTick();
      ticker = setInterval(callTick, 1000);
    }
    island.setActivity('call', inCall ? { kind: 'call' } : null);
    island.refresh();
  }

  micBtn.addEventListener('click', toggleMute);
  window.addEventListener('island-settings', (e) => { modules = e.detail.modules; render(); });
  tauri.event.listen('privacy', (e) => { state = e.payload; render(); })
    .then(() => invoke('privacy_current'))
    .then((p) => { state = p; render(); })
    .catch(() => {});
}
