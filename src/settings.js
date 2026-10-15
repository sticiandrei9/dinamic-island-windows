import { updateStatusText, monitorLabel } from './lib/settings-ui.js';
import { t, setLang, applyStatic } from './lib/i18n.js';

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;

const MODULES = ['media', 'lyrics', 'apps', 'output', 'timer', 'calendar', 'privacy', 'calls', 'alerts', 'network', 'keyboard', 'volume', 'notifications', 'screenshots', 'shelf', 'mirror'];
const SHELF_DAYS = [0, 1, 7, 30];

let settings = null;
let updateState = { kind: 'idle' };
let version = '';

function save() {
  invoke('set_settings', { settings }).catch((e) => console.error(e));
}

function renderModules() {
  const box = document.getElementById('modules');
  box.replaceChildren();
  for (const key of MODULES) {
    const row = document.createElement('div');
    row.className = 'row';
    const label = document.createElement('div');
    label.className = 'label';
    label.textContent = t(`module.${key}`);
    const small = document.createElement('small');
    small.textContent = t(`module.${key}.hint`);
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
  document.getElementById('language').value = settings.language ?? 'auto';
  document.getElementById('shelf-days').value = String(settings.shelf_days ?? 0);
  const cal = document.getElementById('calendar-url');
  if (document.activeElement !== cal) cal.value = settings.calendar_url ?? '';
  if (!recording) document.getElementById('hotkey').textContent = settings.hotkey || t('settings.hotkeyNone');
  document.getElementById('hotkey-clear').hidden = !settings.hotkey;
}

// scurtătura: butonul ascultă următoarea combinație (cel puțin un modificator + o tastă)
let recording = false;

function comboFrom(e) {
  const key = /^Key[A-Z]$/.test(e.code) ? e.code.slice(3) : /^Digit\d$/.test(e.code) ? e.code.slice(5) : /^F\d{1,2}$/.test(e.code) ? e.code : e.code === 'Space' ? 'Space' : null;
  if (!key) return null;
  const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Win'].filter(Boolean);
  // doar Shift nu ajunge: Shift+A ar fura litera A mare din tot ce scrii
  return mods.some((m) => m !== 'Shift') ? [...mods, key].join('+') : null;
}
async function showHotkeyError() {
  const busy = await invoke('hotkey_error').catch(() => null);
  const hint = document.getElementById('hotkey-hint');
  hint.textContent = busy && busy === settings.hotkey ? t('settings.hotkeyBusy') : t('settings.hotkeyHint');
  hint.classList.toggle('error', !!busy && busy === settings.hotkey);
}
document.getElementById('hotkey').addEventListener('click', (e) => {
  recording = true;
  e.target.classList.add('recording');
  e.target.textContent = t('settings.hotkeyPress');
});
window.addEventListener('keydown', (e) => {
  if (!recording) return;
  e.preventDefault();
  const btn = document.getElementById('hotkey');
  if (e.key === 'Escape') { recording = false; btn.classList.remove('recording'); renderControls(); return; }
  const combo = comboFrom(e);
  if (!combo) return; // doar modificatori până acum
  recording = false;
  btn.classList.remove('recording');
  settings.hotkey = combo;
  renderControls();
  save();
});
document.getElementById('hotkey-clear').addEventListener('click', () => { settings.hotkey = ''; renderControls(); save(); });
document.getElementById('shelf-days').addEventListener('change', (e) => { settings.shelf_days = Number(e.target.value); save(); });
document.getElementById('calendar-url').addEventListener('change', async (e) => {
  settings.calendar_url = e.target.value.trim();
  await invoke('set_settings', { settings }).catch((err) => console.error(err));
  checkCalendar();
});

// linkul calendarului: îl citim o dată, ca să afli pe loc dacă merge
async function checkCalendar() {
  const hint = document.getElementById('calendar-hint');
  hint.classList.remove('error');
  if (!settings.calendar_url) { hint.textContent = t('settings.calendarHint'); return; }
  try {
    await invoke('calendar_fetch');
    hint.textContent = t('settings.calendarOk');
  } catch (err) {
    hint.textContent = t('settings.calendarError', { v: err });
    hint.classList.add('error');
  }
}

function renderShelfDays() {
  document.getElementById('shelf-days').replaceChildren(...SHELF_DAYS.map((n) => Object.assign(document.createElement('option'), { value: String(n), textContent: n ? t('settings.afterDays', { n }) : t('settings.never') })));
}

// limba: textele statice, cele generate și titlul ferestrei
function applyLanguage() {
  setLang(settings.resolved_language ?? 'ro');
  applyStatic();
  document.title = t('settings.title');
  document.getElementById('version').textContent = t('settings.version', { v: version });
}

async function renderMonitors() {
  const select = document.getElementById('monitor');
  const monitors = await invoke('list_monitors').catch(() => []);
  const opts = [['primary', t('settings.primary')], ...monitors.map((m, i) => [m.name, monitorLabel(m, i)]), ['all', t('settings.allMonitors')]];
  // un monitor ales care acum e deconectat rămâne în listă, ca alegerea să nu se piardă
  if (settings.monitor && !opts.some(([v]) => v === settings.monitor)) opts.splice(1, 0, [settings.monitor, t('settings.disconnected')]);
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
document.getElementById('language').addEventListener('change', (e) => { settings.language = e.target.value; save(); });
document.getElementById('check-now').addEventListener('click', () => invoke('check_updates_now'));
document.getElementById('apply').addEventListener('click', () => invoke('apply_update').catch((e) => { document.getElementById('update-status').textContent = t('error', { v: e }); }));
document.getElementById('open-logs').addEventListener('click', () => invoke('open_log_dir'));

tauri.event.listen('settings-changed', (e) => {
  const langChanged = e.payload.resolved_language !== settings?.resolved_language;
  settings = e.payload;
  if (langChanged) { applyLanguage(); renderMonitors(); renderUpdate(); renderShelfDays(); }
  renderModules();
  renderControls();
  showHotkeyError();
});
tauri.event.listen('update-state', (e) => { updateState = e.payload; renderUpdate(); });
setInterval(renderUpdate, 30_000); // „verificat acum X minute” rămâne actual

(async () => {
  settings = await invoke('get_settings');
  updateState = await invoke('get_update_state');
  version = await tauri.app.getVersion();
  applyLanguage();
  renderModules();
  renderShelfDays();
  await renderMonitors();
  renderControls();
  showHotkeyError();
  checkCalendar();
  renderUpdate();
})();
