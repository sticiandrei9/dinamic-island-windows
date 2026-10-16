import { updateStatusText, monitorLabel } from './lib/settings-ui.js';
import { t, setLang, applyStatic } from './lib/i18n.js';

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;

const MODULES = ['media', 'lyrics', 'apps', 'output', 'timer', 'calendar', 'privacy', 'calls', 'alerts', 'network', 'keyboard', 'volume', 'notifications', 'screenshots', 'downloads', 'clipboard', 'unlock', 'system', 'sleep', 'notes', 'claude', 'shelf', 'mirror'];
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
    if (key === 'claude') box.append(claudeStatusRow());
  }
}

// starea hook-ului Claude sub comutator: instalat / lipsește (+ Instalează) / build de dezvoltare / eroare
function claudeStatusRow() {
  const row = document.createElement('div');
  row.className = 'row';
  row.hidden = true;
  const label = document.createElement('div');
  label.className = 'label';
  const small = document.createElement('small');
  label.append(small);
  const btn = Object.assign(document.createElement('button'), { className: 'btn', textContent: t('settings.claudeInstall'), hidden: true });
  row.append(label, btn);
  const fill = async () => {
    const st = await invoke('claude_hook_status').catch(() => 'missing');
    const kind = typeof st === 'string' ? st : Object.keys(st)[0];
    const failed = kind === 'dev' || kind === 'invalid';
    row.hidden = !settings.modules.claude && !failed; // oprit și fără eșec: nimic de arătat
    small.className = failed ? 'error' : '';
    small.textContent = kind === 'invalid' ? t('settings.claudeInvalid', { v: st.invalid }) : t(`settings.claude${kind[0].toUpperCase()}${kind.slice(1)}`);
    btn.hidden = kind !== 'missing';
  };
  btn.addEventListener('click', async () => { await invoke('claude_hook_install').catch(() => {}); fill(); });
  fill();
  return row;
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
  for (const h of HOTKEYS) {
    if (recording !== h) document.getElementById(h.id).textContent = settings[h.key] || t('settings.hotkeyNone');
    document.getElementById(`${h.id}-clear`).hidden = !settings[h.key];
  }
}

// scurtăturile (insula și notițele): butonul ascultă următoarea combinație (cel puțin un modificator + o tastă).
// `err` = câmpul din `hotkey_error`; `hint` = textul de sub nume când scurtătura nu e refuzată de Windows
const HOTKEYS = [
  { key: 'hotkey', id: 'hotkey', err: 'island', hint: () => t('settings.hotkeyHint') },
  { key: 'notes_hotkey', id: 'notes-hotkey', err: 'notes', hint: () => t('settings.notesHotkeyHint', { combo: 'Win+Shift+N' }) },
];
let recording = null; // scurtătura care ascultă acum

function comboFrom(e) {
  const key = /^Key[A-Z]$/.test(e.code) ? e.code.slice(3) : /^Digit\d$/.test(e.code) ? e.code.slice(5) : /^F\d{1,2}$/.test(e.code) ? e.code : e.code === 'Space' ? 'Space' : null;
  if (!key) return null;
  const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Win'].filter(Boolean);
  // doar Shift nu ajunge: Shift+A ar fura litera A mare din tot ce scrii
  return mods.some((m) => m !== 'Shift') ? [...mods, key].join('+') : null;
}
async function showHotkeyError() {
  const errors = (await invoke('hotkey_error').catch(() => null)) ?? {};
  for (const h of HOTKEYS) {
    const busy = errors[h.err];
    const refused = !!busy && busy === settings[h.key];
    const hint = document.getElementById(`${h.id}-hint`);
    hint.textContent = refused ? t('settings.hotkeyBusy') : h.hint();
    hint.classList.toggle('error', refused);
  }
}
for (const h of HOTKEYS) {
  document.getElementById(h.id).addEventListener('click', (e) => {
    recording = h;
    e.target.classList.add('recording');
    e.target.textContent = t('settings.hotkeyPress');
  });
  document.getElementById(`${h.id}-clear`).addEventListener('click', () => { settings[h.key] = ''; renderControls(); save(); });
}
window.addEventListener('keydown', (e) => {
  if (!recording) return;
  e.preventDefault();
  const h = recording;
  const btn = document.getElementById(h.id);
  if (e.key === 'Escape') { recording = null; btn.classList.remove('recording'); renderControls(); return; }
  const combo = comboFrom(e);
  if (!combo) return; // doar modificatori până acum
  recording = null;
  btn.classList.remove('recording');
  settings[h.key] = combo;
  renderControls();
  save();
});
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
