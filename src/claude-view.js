import { ICONS } from './icons.js';
import { t } from './lib/i18n.js';
import { transitions, pillLabel, sinceLabel } from './lib/claude.js';

// Sesiunile Claude Code (hook-ul scrie starea, Rust o emite ca `claude-sessions`):
// pastila cât lucrează sau te așteaptă, tranzitorii la „te așteaptă” / „a terminat”, rând în Acasă.
export function init(island) {
  const { invoke, tauri } = island;
  const $ = (id) => document.getElementById(id);
  const row = $('x-claude');
  // iconița aplicației Claude Desktop (data URL, din exe-ul instalat); fără ea rămâne steluța
  let img = null;
  const fill = (el) => {
    if (img) el.replaceChildren(Object.assign(new Image(), { src: img, alt: '', draggable: false }));
    else el.innerHTML = ICONS.sparkle;
  };
  const pillAndBubble = () => { fill($('c-claude-icon')); fill($('b-claude')); };
  let loadingIcon = false;
  let lastTry = -Infinity;
  function loadIcon() {
    if (img || loadingIcon || !tauri || performance.now() - lastTry < 60_000) return; // lipsa: cel mult o dată pe minut
    loadingIcon = true;
    lastTry = performance.now();
    invoke('claude_icon').then((url) => {
      if (!url) return;
      img = url;
      pillAndBubble();
      rowKey = ''; // rândul din Acasă se redesenează cu imaginea
      renderRow();
    }).catch(() => {}).finally(() => { loadingIcon = false; });
  }
  pillAndBubble();
  let enabled = false;
  let prev = []; // ultima listă primită, cu `done`: din ea ies tranzițiile
  let active = []; // doar `working` și `waiting`
  let rowKey = '';

  const focus = (id) => invoke('claude_focus', { id }).catch((e) => invoke('log_js', { msg: `claude: ${e}` }));

  function announce({ id, project, to }) {
    island.pushTransient({
      kind: 'claude', id, icon: 'sparkle', img, color: to === 'waiting' ? 'var(--orange)' : 'var(--green)',
      title: t(`claude.${to}`), detail: project, duration: to === 'waiting' ? 6000 : 5000, interactive: true,
    });
  }

  // click pe tranzitorie: fereastra sesiunii; opresc propagarea ca să nu se extindă și insula
  $('transient').addEventListener('click', (e) => {
    const s = island.shown;
    if (s?.kind !== 'claude') return;
    e.stopPropagation();
    focus(s.id);
    island.dropTransients('claude');
  });

  function updateTimes() {
    const now = Date.now();
    for (const el of row.children) {
      const s = active.find((x) => x.id === el.dataset.id);
      if (!s) continue;
      const since = sinceLabel(s.since, now);
      el.querySelector('.st').textContent = ` · ${t(`claude.state.${s.state}`)}${since ? ` · ${since}` : ''}`;
    }
  }

  function renderRow() {
    row.hidden = !active.length;
    const key = active.map((s) => `${s.id}:${s.state}`).join();
    if (key !== rowKey) {
      rowKey = key;
      row.replaceChildren(...active.map((s) => {
        const el = Object.assign(document.createElement('div'), { className: `claude-item ${s.state}` });
        el.dataset.id = s.id;
        fill(el); // pictogramă constantă; proiectul intră doar ca text
        el.append(Object.assign(document.createElement('b'), { textContent: s.project }), Object.assign(document.createElement('span'), { className: 'st' }));
        el.addEventListener('click', (e) => { e.stopPropagation(); focus(s.id); island.collapse(); });
        return el;
      }));
    }
    updateTimes();
  }

  function apply(next) {
    for (const tr of transitions(prev, next)) announce(tr);
    prev = next;
    active = next.filter((s) => s.state !== 'done');
    const waiting = active.filter((s) => s.state === 'waiting');
    const shown = waiting.length ? waiting : active; // active are acum doar working
    if (shown.length) loadIcon();
    if (shown.length) $('c-claude-text').textContent = pillLabel(shown);
    island.setActivity('claude', shown.length ? { kind: waiting.length ? 'claude-waiting' : 'claude' } : null);
    renderRow();
    island.refresh();
  }

  // demo din browser (?demo&claude), fără Rust
  window.addEventListener('claude-demo', (e) => { enabled = true; apply(e.detail); });
  if (!tauri) return;

  loadIcon();
  tauri.event.listen('claude-sessions', (e) => enabled && apply(e.payload));
  window.addEventListener('home-show', updateTimes);
  setInterval(updateTimes, 15_000);
  window.addEventListener('island-settings', (e) => {
    const on = e.detail.modules.claude === true; // implicit oprit
    if (on === enabled) { if (enabled) apply(prev); return; } // aceeași stare: doar textele (limba)
    enabled = on;
    if (!on) { prev = []; apply([]); return; }
    invoke('claude_sessions').then((list) => { if (enabled) { prev = list; apply(list); } }).catch(() => {});
  });
}
