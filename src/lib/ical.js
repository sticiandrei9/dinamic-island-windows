// Evenimentele următoare dintr-un calendar iCal (linkul secret din Google Calendar), cu repetări.
// Acoperă ce exportă Google/Outlook: RRULE DAILY/WEEKLY/MONTHLY/YEARLY (INTERVAL, COUNT, UNTIL,
// BYDAY, BYMONTHDAY), EXDATE și instanțele mutate (RECURRENCE-ID). Evenimentele de toată ziua sunt sărite.

const DAY = 86_400_000;
const WEEKDAYS = ['SU', 'MO', 'TU', 'WE', 'TH', 'FR', 'SA'];
const fmtCache = new Map();

// cât e ceasul în `tz` la momentul `ms`, minus ora UTC (offset-ul fusului, în ms)
function zoneOffset(ms, tz) {
  if (!fmtCache.has(tz)) {
    fmtCache.set(tz, new Intl.DateTimeFormat('en-US', { timeZone: tz, hourCycle: 'h23', year: 'numeric', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit' }));
  }
  const p = Object.fromEntries(fmtCache.get(tz).formatToParts(new Date(ms)).map((x) => [x.type, x.value]));
  return Date.UTC(+p.year, p.month - 1, +p.day, +p.hour, +p.minute, +p.second) - Math.floor(ms / 1000) * 1000;
}

// ora „de perete” [an, lună, zi, oră, min, s] din fusul `tz` → milisecunde UTC.
// 'UTC' = ora cu Z; null = oră „plutitoare” (locală); un nume necunoscut (de ex. Windows) = local.
export function toEpoch([y, mo, d, h, mi, s], tz) {
  if (tz === 'UTC') return Date.UTC(y, mo - 1, d, h, mi, s);
  const local = new Date(y, mo - 1, d, h, mi, s).getTime();
  if (!tz) return local;
  try {
    const wall = Date.UTC(y, mo - 1, d, h, mi, s);
    const first = wall - zoneOffset(wall, tz);
    return wall - zoneOffset(first, tz); // a doua trecere corectează lângă schimbarea orei
  } catch {
    return local;
  }
}

function unescape(v) {
  return v.replace(/\\([\\;,nN])/g, (m, c) => (c === 'n' || c === 'N' ? ' ' : c));
}

function parseLine(line) {
  let quoted = false, i = 0;
  for (; i < line.length; i++) {
    if (line[i] === '"') quoted = !quoted;
    else if (line[i] === ':' && !quoted) break;
  }
  const [name, ...ps] = line.slice(0, i).split(';');
  const params = {};
  for (const p of ps) {
    const j = p.indexOf('=');
    if (j > 0) params[p.slice(0, j).toUpperCase()] = p.slice(j + 1).replace(/^"|"$/g, '');
  }
  return { name: name.toUpperCase(), params, value: line.slice(i + 1) };
}

// "20261015T120000Z" / "20261015" → { wall, tz, allDay }
function parseDate(value, params) {
  const m = /^(\d{4})(\d{2})(\d{2})(?:T(\d{2})(\d{2})(\d{2})(Z)?)?/.exec(value.trim());
  if (!m) return null;
  const wall = [+m[1], +m[2], +m[3], +(m[4] ?? 0), +(m[5] ?? 0), +(m[6] ?? 0)];
  const allDay = params.VALUE === 'DATE' || m[4] == null;
  return { wall, tz: m[7] ? 'UTC' : params.TZID ?? null, allDay };
}

function parseDuration(v) {
  const m = /^([+-])?P(?:(\d+)W)?(?:(\d+)D)?(?:T(?:(\d+)H)?(?:(\d+)M)?(?:(\d+)S)?)?$/.exec(v.trim());
  if (!m) return 0;
  const ms = (+(m[2] ?? 0) * 7 + +(m[3] ?? 0)) * DAY + (+(m[4] ?? 0) * 3600 + +(m[5] ?? 0) * 60 + +(m[6] ?? 0)) * 1000;
  return m[1] === '-' ? -ms : ms;
}

function parseEvents(text) {
  const lines = text.replace(/\r?\n[ \t]/g, '').split(/\r?\n/);
  const events = [];
  let ev = null;
  for (const line of lines) {
    if (line === 'BEGIN:VEVENT') { ev = { exdates: [] }; continue; }
    if (line === 'END:VEVENT') { if (ev) events.push(ev); ev = null; continue; }
    if (!ev) continue;
    const { name, params, value } = parseLine(line);
    if (name === 'SUMMARY') ev.title = unescape(value);
    else if (name === 'UID') ev.uid = value;
    else if (name === 'STATUS') ev.cancelled = value.toUpperCase() === 'CANCELLED';
    else if (name === 'DTSTART') ev.start = parseDate(value, params);
    else if (name === 'DTEND') ev.end = parseDate(value, params);
    else if (name === 'DURATION') ev.duration = parseDuration(value);
    else if (name === 'RRULE') ev.rrule = Object.fromEntries(value.split(';').map((p) => p.split('=')).map(([k, v]) => [k.toUpperCase(), v]));
    else if (name === 'EXDATE') {
      for (const v of value.split(',')) {
        const d = parseDate(v, params);
        if (d) ev.exdates.push(toEpoch(d.wall, d.tz));
      }
    } else if (name === 'RECURRENCE-ID') {
      const d = parseDate(value, params);
      if (d) ev.recurrenceId = toEpoch(d.wall, d.tz);
    }
  }
  return events;
}

// ora de perete ca UTC (doar pentru aritmetica pe zile/luni) ↔ [an, lună, …]
const wallMs = (w) => Date.UTC(w[0], w[1] - 1, w[2], w[3], w[4], w[5]);
const wallOf = (ms) => { const d = new Date(ms); return [d.getUTCFullYear(), d.getUTCMonth() + 1, d.getUTCDate(), d.getUTCHours(), d.getUTCMinutes(), d.getUTCSeconds()]; };

// zilele (ora de perete) ale perioadei `k` a regulii, în ordine
function periodDays(base, freq, k, interval, rule) {
  const [y, mo, d] = base;
  const time = base[3] * 3600_000 + base[4] * 60_000 + base[5] * 1000;
  const byday = rule.BYDAY ? rule.BYDAY.split(',') : null;
  if (freq === 'DAILY') {
    const ms = Date.UTC(y, mo - 1, d) + k * interval * DAY + time;
    return !byday || byday.includes(WEEKDAYS[new Date(ms).getUTCDay()]) ? [ms] : [];
  }
  if (freq === 'WEEKLY') {
    const monday = Date.UTC(y, mo - 1, d) - ((new Date(Date.UTC(y, mo - 1, d)).getUTCDay() + 6) % 7) * DAY + k * interval * 7 * DAY;
    const days = byday ?? [WEEKDAYS[new Date(Date.UTC(y, mo - 1, d)).getUTCDay()]];
    return days.map((w) => monday + ((WEEKDAYS.indexOf(w.slice(-2)) + 6) % 7) * DAY + time).sort((a, b) => a - b);
  }
  if (freq === 'MONTHLY' || freq === 'YEARLY') {
    const step = freq === 'MONTHLY' ? k * interval : k * interval * 12;
    const first = new Date(Date.UTC(y, mo - 1 + step, 1));
    const [yy, mm] = [first.getUTCFullYear(), first.getUTCMonth()];
    const inMonth = new Date(Date.UTC(yy, mm + 1, 0)).getUTCDate();
    if (byday) {
      // „2TU” = a doua marți, „-1FR” = ultima vineri
      return byday.flatMap((w) => {
        const n = parseInt(w, 10);
        const wd = WEEKDAYS.indexOf(w.slice(-2));
        const dates = [];
        for (let day = 1; day <= inMonth; day++) if (new Date(Date.UTC(yy, mm, day)).getUTCDay() === wd) dates.push(day);
        const pick = Number.isNaN(n) ? dates : [dates[n > 0 ? n - 1 : dates.length + n]];
        return pick.filter(Boolean).map((day) => Date.UTC(yy, mm, day) + time);
      }).sort((a, b) => a - b);
    }
    const day = rule.BYMONTHDAY ? +rule.BYMONTHDAY : d;
    return day <= inMonth ? [Date.UTC(yy, mm, day) + time] : [];
  }
  return [];
}

// începuturile (ms UTC) ale aparițiilor care se suprapun cu [from, to)
function occurrences(ev, duration, from, to) {
  const start = toEpoch(ev.start.wall, ev.start.tz);
  if (!ev.rrule) return start < to && start + duration > from ? [start] : [];
  const rule = ev.rrule;
  const freq = rule.FREQ;
  if (!['DAILY', 'WEEKLY', 'MONTHLY', 'YEARLY'].includes(freq)) return [];
  const interval = Math.max(1, +(rule.INTERVAL ?? 1));
  const count = rule.COUNT ? +rule.COUNT : Infinity;
  const untilDate = rule.UNTIL ? parseDate(rule.UNTIL, {}) : null;
  const until = untilDate ? toEpoch(untilDate.wall, untilDate.tz ?? ev.start.tz) : Infinity;
  const out = [];
  let n = 0;
  for (let k = 0; k < 20_000; k++) {
    const days = periodDays(ev.start.wall, freq, k, interval, rule);
    for (const w of days) {
      if (w < wallMs(ev.start.wall)) continue;
      const at = toEpoch(wallOf(w), ev.start.tz);
      if (at > until || n >= count || at >= to) return out;
      n++;
      if (at + duration > from && !ev.exdates.includes(at)) out.push(at);
    }
  }
  return out;
}

// Evenimentele care se termină după `now` și încep înainte de `now + horizon`, sortate.
export function upcoming(text, now, horizon) {
  const events = parseEvents(text).filter((e) => e.start && !e.start.allDay);
  // instanțele mutate înlocuiesc apariția originală (același UID, ora din RECURRENCE-ID)
  const moved = new Map();
  for (const e of events) if (e.recurrenceId != null) moved.set(`${e.uid}|${e.recurrenceId}`, true);
  const out = [];
  for (const e of events) {
    if (e.cancelled && e.recurrenceId == null) continue;
    const start = toEpoch(e.start.wall, e.start.tz);
    const duration = e.end ? toEpoch(e.end.wall, e.end.tz) - start : e.duration ?? 0;
    for (const at of occurrences(e.recurrenceId != null ? { ...e, rrule: null } : e, duration, now, now + horizon)) {
      if (e.recurrenceId == null && moved.has(`${e.uid}|${at}`)) continue;
      if (!e.cancelled) out.push({ title: e.title ?? '', start: at, end: at + duration });
    }
  }
  return out.sort((a, b) => a.start - b.start);
}
