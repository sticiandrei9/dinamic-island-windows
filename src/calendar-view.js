import { ICONS } from './icons.js';
import { t, locale } from './lib/i18n.js';
import { upcoming, googleEventUrl } from './lib/ical.js';

const SOON = 10 * 60_000; // pastila apare cu 10 minute înainte
const AFTER = 5 * 60_000; // și rămâne 5 minute după început

// Calendarul Google (link iCal): următorul eveniment de azi în fila Acasă, iar cu 10 minute
// înainte de o întâlnire, pastila arată cât a mai rămas.
export function init(island) {
  const { invoke, tauri } = island;
  if (!tauri) return;
  const row = document.getElementById('x-cal');
  document.getElementById('c-meet-icon').innerHTML = ICONS.calendar;
  const pillText = document.getElementById('c-meet-text');
  let text = null;
  let expanded = []; // evenimentele următoarelor 48 h, calculate o dată la descărcare
  let url = '';
  let enabled = false;
  let shownEvent = null; // evenimentul din rândul filei Acasă

  const open = (link) => {
    invoke('calendar_open', { url: link }).catch((err) => invoke('log_js', { msg: `calendar: deschiderea a eșuat: ${err}` }));
    island.collapse();
  };
  // click pe rând: evenimentul în Google Calendar
  row.addEventListener('click', (e) => { e.stopPropagation(); if (shownEvent) open(googleEventUrl(shownEvent, url)); });

  async function load() {
    if (!enabled) return tick();
    // la o eroare de rețea rămâne calendarul descărcat ultima dată
    text = (await invoke('calendar_fetch').catch(() => text)) ?? null;
    try { expanded = text ? upcoming(text, Date.now(), 48 * 3600_000) : []; } catch (e) { invoke('log_js', { msg: `calendar: ${e}` }); }
    tick();
  }

  function tick() {
    const now = Date.now();
    const events = enabled ? expanded.filter((e) => e.end > now) : [];
    const midnight = new Date(now).setHours(24, 0, 0, 0);

    const today = events.find((e) => e.start < midnight);
    row.hidden = !today;
    shownEvent = today ?? null;
    if (today) {
      const when = today.start <= now ? t('cal.now') : new Date(today.start).toLocaleTimeString(locale(), { hour: '2-digit', minute: '2-digit' });
      row.innerHTML = ICONS.calendar; // pictogramă constantă; textul evenimentului intră doar ca text
      row.append(Object.assign(document.createElement('b'), { textContent: today.title }), ` · ${when}`);
      // întâlnire online: buton „Intră” direct în Meet/Zoom/Teams
      if (today.link) {
        const join = Object.assign(document.createElement('button'), { className: 'cal-join', textContent: t('cal.join') });
        join.addEventListener('click', (e) => { e.stopPropagation(); open(today.link); });
        row.append(join);
      }
    }

    const soon = events.find((e) => e.start - now <= SOON && now < e.start + AFTER);
    if (soon) {
      const left = soon.start - now;
      pillText.textContent = `${soon.title} · ${left > 0 ? t('cal.in', { n: Math.ceil(left / 60_000) }) : t('cal.now')}`;
    }
    island.setActivity('meeting', soon ? { kind: 'meeting' } : null);
    island.refresh();
  }

  setInterval(load, 3 * 60_000);
  window.addEventListener('home-show', () => enabled && load()); // descărcarea e în cache: ieftin
  setInterval(tick, 15_000);
  window.addEventListener('island-settings', (e) => {
    const on = e.detail.modules.calendar !== false && !!e.detail.calendar_url?.trim();
    const changed = on !== enabled || e.detail.calendar_url !== url;
    enabled = on;
    url = e.detail.calendar_url ?? '';
    if (!changed) return;
    text = null;
    expanded = [];
    load();
  });
}
