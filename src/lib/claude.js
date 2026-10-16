import { t } from './i18n.js';

// Ce s-a schimbat între două liste de sesiuni: doar trecerile în `waiting` / `done` ale unor sesiuni
// cunoscute dinainte (la pornire sau la prima apariție nu anunțăm nimic).
export function transitions(prev, next) {
  const before = new Map(prev.map((s) => [s.id, s.state]));
  return next
    .filter((s) => before.has(s.id) && before.get(s.id) !== s.state && (s.state === 'waiting' || s.state === 'done'))
    .map((s) => ({ id: s.id, project: s.project, to: s.state }));
}

// Textul pastilei: numele proiectului sau câte sesiuni sunt.
export function pillLabel(sessions) {
  return sessions.length === 1 ? sessions[0].project : t('claude.sessions', { n: sessions.length });
}

// „5 min” de când e sesiunea în starea curentă; sub un minut nu arătăm nimic.
export function sinceLabel(since, now) {
  const n = Math.floor((now - since) / 60_000);
  return n < 1 ? '' : t('claude.min', { n });
}
