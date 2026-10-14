import { updateStatusText, monitorLabel } from './lib/settings-ui.js';

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;

const MODULES = [
  ['media', 'Media', 'Ce cântă: Spotify, YouTube, playere; control ⏮ ⏯ ⏭'],
  ['lyrics', 'Versuri', 'Versuri sincronizate de pe LRCLIB, în insula extinsă'],
  ['apps', 'Aplicații deschise', 'Iconițele ferestrelor; click aduce aplicația în față'],
  ['timer', 'Timer și cronometru', 'Timpul rămas apare în insulă'],
  ['privacy', 'Microfon și cameră', 'Punct portocaliu / verde când sunt folosite'],
  ['alerts', 'Alerte de sistem', 'Încărcător, baterie, dispozitive Bluetooth'],
  ['volume', 'Volum și luminozitate', 'Afișate în insulă (pe Windows 11 rămâne și pop-up-ul sistemului)'],
  ['notifications', 'Notificări Windows', 'Notificările apar și în insulă'],
  ['shelf', 'Raft de fișiere', 'Trage fișiere pe insulă ca să le ai la îndemână'],
  ['mirror', 'Oglindă cameră', 'Vezi-te înainte de un apel'],
];

let settings = null;
let updateState = { kind: 'idle' };

function save() {
  invoke('set_settings', { settings }).catch((e) => console.error(e));
}

function renderModules() {
  const box = document.getElementById('modules');
  box.replaceChildren();
  for (const [key, title, desc] of MODULES) {
    const row = document.createElement('div');
    row.className = 'row';
    const label = document.createElement('div');
    label.className = 'label';
    label.textContent = title;
    const small = document.createElement('small');
    small.textContent = desc;
    label.append(small);
    const sw = document.createElement('label');
    sw.className = 'switch';
    const input = document.createElement('input');
    input.type = 'checkbox';
    input.checked = !!settings.modules[key];
    input.addEventListener('change', () => { settings.modules[key] = input.checked; save(); });
    sw.append(input, document.createElement('span'));
    row.append(label, sw);
    box.append(row);
  }
}

function renderControls() {
  document.querySelectorAll('.segmented').forEach((seg) => {
    const key = seg.dataset.key;
    seg.querySelectorAll('button').forEach((b) => b.classList.toggle('on', b.dataset.value === settings[key]));
  });
  document.querySelectorAll('input[data-key]').forEach((i) => { i.checked = !!settings[i.dataset.key]; });
  document.getElementById('monitor').value = settings.monitor;
}

async function renderMonitors() {
  const select = document.getElementById('monitor');
  const monitors = await invoke('list_monitors').catch(() => []);
  const opts = [['primary', 'Principal'], ...monitors.map((m, i) => [m.name, monitorLabel(m, i)]), ['all', 'Toate monitoarele']];
  // un monitor ales care acum e deconectat rămâne în listă, ca alegerea să nu se piardă
  if (settings.monitor && !opts.some(([v]) => v === settings.monitor)) opts.splice(1, 0, [settings.monitor, 'Monitor deconectat']);
  select.replaceChildren(...opts.map(([value, text]) => Object.assign(document.createElement('option'), { value, textContent: text })));
  select.value = settings.monitor;
}

function renderUpdate() {
  document.getElementById('update-status').textContent = updateStatusText(updateState, Date.now() / 1000);
  const busy = updateState.kind === 'checking' || updateState.kind === 'downloading';
  document.getElementById('check-now').disabled = busy;
  document.getElementById('apply').hidden = updateState.kind !== 'ready';
}

document.querySelectorAll('.segmented').forEach((seg) => {
  seg.addEventListener('click', (e) => {
    const b = e.target.closest('button');
    if (!b) return;
    settings[seg.dataset.key] = b.dataset.value;
    renderControls();
    save();
  });
});
document.querySelectorAll('input[data-key]').forEach((i) => {
  i.addEventListener('change', () => { settings[i.dataset.key] = i.checked; save(); });
});
document.getElementById('monitor').addEventListener('change', (e) => { settings.monitor = e.target.value; save(); });
document.getElementById('check-now').addEventListener('click', () => invoke('check_updates_now'));
document.getElementById('apply').addEventListener('click', () => invoke('apply_update').catch((e) => { document.getElementById('update-status').textContent = `Eroare: ${e}`; }));
document.getElementById('open-logs').addEventListener('click', () => invoke('open_log_dir'));

tauri.event.listen('settings-changed', (e) => { settings = e.payload; renderModules(); renderControls(); });
tauri.event.listen('update-state', (e) => { updateState = e.payload; renderUpdate(); });
setInterval(renderUpdate, 30_000); // „verificat acum X minute” rămâne actual

(async () => {
  settings = await invoke('get_settings');
  updateState = await invoke('get_update_state');
  document.getElementById('version').textContent = `Versiunea ${await tauri.app.getVersion()}`;
  renderModules();
  await renderMonitors();
  renderControls();
  renderUpdate();
})();
