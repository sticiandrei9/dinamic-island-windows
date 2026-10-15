import { test } from 'node:test';
import assert from 'node:assert/strict';
import { upcoming, toEpoch } from '../src/lib/ical.js';

const cal = (...events) => ['BEGIN:VCALENDAR', 'VERSION:2.0', ...events.flatMap((e) => ['BEGIN:VEVENT', ...e, 'END:VEVENT']), 'END:VCALENDAR'].join('\r\n');
const H = 3600_000;
const D = 24 * H;

test('fus orar din TZID, inclusiv la schimbarea orei de vară', () => {
  // București: UTC+3 vara, UTC+2 iarna
  assert.equal(toEpoch([2026, 7, 1, 10, 0, 0], 'Europe/Bucharest'), Date.UTC(2026, 6, 1, 7));
  assert.equal(toEpoch([2026, 12, 1, 10, 0, 0], 'Europe/Bucharest'), Date.UTC(2026, 11, 1, 8));
  assert.equal(toEpoch([2026, 12, 1, 10, 0, 0], 'UTC'), Date.UTC(2026, 11, 1, 10));
  // un TZID necunoscut (nume Windows) cade pe ora locală, fără excepție
  assert.equal(toEpoch([2026, 12, 1, 10, 0, 0], 'Romance Standard Time'), new Date(2026, 11, 1, 10).getTime());
});

test('eveniment simplu, linii împăturite, toată ziua și anulate sărite', () => {
  const text = cal(
    ['UID:a', 'SUMMARY:Ședință cu ech', ' ipa', 'DTSTART:20261015T120000Z', 'DTEND:20261015T130000Z'],
    ['UID:b', 'SUMMARY:Zi liberă', 'DTSTART;VALUE=DATE:20261015', 'DTEND;VALUE=DATE:20261016'],
    ['UID:c', 'SUMMARY:Anulat', 'STATUS:CANCELLED', 'DTSTART:20261015T140000Z', 'DTEND:20261015T150000Z'],
    ['UID:d', 'SUMMARY:Trecut', 'DTSTART:20261014T120000Z', 'DTEND:20261014T130000Z'],
  );
  const now = Date.UTC(2026, 9, 15, 11, 0);
  assert.deepEqual(upcoming(text, now, D), [{ title: 'Ședință cu echipa', start: Date.UTC(2026, 9, 15, 12), end: Date.UTC(2026, 9, 15, 13) }]);
  // în desfășurare: încă apare
  assert.equal(upcoming(text, Date.UTC(2026, 9, 15, 12, 30), D).length, 1);
});

test('DURATION în loc de DTEND, text cu caractere escapate', () => {
  const text = cal(['UID:a', 'SUMMARY:Stand-up\\, zilnic', 'DTSTART:20261015T090000Z', 'DURATION:PT15M']);
  const [e] = upcoming(text, Date.UTC(2026, 9, 15, 8), D);
  assert.equal(e.title, 'Stand-up, zilnic');
  assert.equal(e.end - e.start, 15 * 60_000);
});

test('repetare săptămânală cu BYDAY, excepții și o instanță mutată', () => {
  const text = cal(
    ['UID:w', 'SUMMARY:Sport', 'DTSTART;TZID=Europe/Bucharest:20260105T180000', 'DTEND;TZID=Europe/Bucharest:20260105T190000',
      'RRULE:FREQ=WEEKLY;BYDAY=MO,TH', 'EXDATE;TZID=Europe/Bucharest:20261015T180000'],
    // instanța de luni 19 oct mutată la 20:00
    ['UID:w', 'SUMMARY:Sport (mutat)', 'RECURRENCE-ID;TZID=Europe/Bucharest:20261019T180000',
      'DTSTART;TZID=Europe/Bucharest:20261019T200000', 'DTEND;TZID=Europe/Bucharest:20261019T210000'],
  );
  const now = Date.UTC(2026, 9, 15, 0); // joi 15 oct 2026
  const evs = upcoming(text, now, 8 * D);
  assert.deepEqual(evs.map((e) => [e.title, new Date(e.start).toISOString()]), [
    ['Sport (mutat)', '2026-10-19T17:00:00.000Z'], // 20:00 la București (UTC+3)
    ['Sport', '2026-10-22T15:00:00.000Z'], // joi 22 oct, 18:00 UTC+3
  ]);
});

test('repetare zilnică cu COUNT/UNTIL și lunară după a n-a zi a săptămânii', () => {
  const daily = cal(['UID:d', 'SUMMARY:Pastile', 'DTSTART:20261010T080000Z', 'DTEND:20261010T081000Z', 'RRULE:FREQ=DAILY;COUNT=5']);
  assert.equal(upcoming(daily, Date.UTC(2026, 9, 14, 9), D).length, 0); // a 5-a a fost pe 14
  assert.equal(upcoming(daily, Date.UTC(2026, 9, 14, 7), D).length, 1);
  const until = cal(['UID:u', 'SUMMARY:X', 'DTSTART:20261010T080000Z', 'DTEND:20261010T081000Z', 'RRULE:FREQ=DAILY;INTERVAL=2;UNTIL=20261016T000000Z']);
  assert.deepEqual(upcoming(until, Date.UTC(2026, 9, 13), 10 * D).map((e) => new Date(e.start).getUTCDate()), [14]);
  // a doua marți din lună
  const monthly = cal(['UID:m', 'SUMMARY:Ședință lunară', 'DTSTART:20260113T100000Z', 'DTEND:20260113T110000Z', 'RRULE:FREQ=MONTHLY;BYDAY=2TU']);
  assert.deepEqual(upcoming(monthly, Date.UTC(2026, 9, 1), 60 * D).map((e) => new Date(e.start).toISOString().slice(0, 10)), ['2026-10-13', '2026-11-10']);
});

test('text invalid nu aruncă excepții', () => {
  assert.deepEqual(upcoming('nu e calendar', Date.now(), D), []);
  assert.deepEqual(upcoming(cal(['UID:x', 'SUMMARY:fără dată']), Date.now(), D), []);
});
