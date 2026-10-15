import { test } from 'node:test';
import assert from 'node:assert/strict';
import { layout, enqueue, advance, current, holdHead } from '../src/lib/activities.js';

const media = { id: 'media', kind: 'media' };
const timer = { id: 'timer', kind: 'timer' };

test('fără activități: pastila goală, fără bulă', () => {
  assert.deepEqual(layout([]), { pill: null, bubble: null });
});

test('o singură activitate ocupă pastila', () => {
  assert.deepEqual(layout([media]), { pill: media, bubble: null });
});

test('timerul are prioritate; media trece în bulă', () => {
  assert.deepEqual(layout([media, timer]), { pill: timer, bubble: media });
});

test('tranzitoriile se afișează pe rând', () => {
  let q = [];
  q = enqueue(q, { kind: 'alert', title: 'A', duration: 3000 }, 0);
  q = enqueue(q, { kind: 'alert', title: 'B', duration: 3000 }, 100);
  assert.equal(current(q, 1000).title, 'A');
  q = advance(q, 3100);
  assert.equal(current(q, 3100).title, 'B');
  q = advance(q, 6200);
  assert.equal(current(q, 6200), null);
});

test('volumul schimbat de mai multe ori e o singură activitate, actualizată și prelungită', () => {
  let q = [];
  q = enqueue(q, { kind: 'volume', level: 0.2, duration: 1500 }, 0);
  q = enqueue(q, { kind: 'volume', level: 0.3, duration: 1500 }, 1000);
  q = enqueue(q, { kind: 'volume', level: 0.4, duration: 1500 }, 2000);
  assert.equal(q.length, 1);
  assert.equal(current(q, 2000).level, 0.4);
  q = advance(q, 3000); // 1000 ms după ultima schimbare: încă vizibil
  assert.equal(current(q, 3000).level, 0.4);
  q = advance(q, 3600);
  assert.equal(current(q, 3600), null);
});

test('volumul trece în fața cozii (feedback imediat), alerta revine după el', () => {
  let q = [];
  q = enqueue(q, { kind: 'alert', title: 'A', duration: 3000 }, 0);
  q = enqueue(q, { kind: 'volume', level: 0.5, duration: 1500 }, 500);
  assert.equal(current(q, 600).kind, 'volume');
  q = advance(q, 2100);
  assert.equal(current(q, 2100).title, 'A');
});

test('cel mult 3 notificări în așteptare; cele mai vechi se pierd', () => {
  let q = [];
  for (let i = 1; i <= 6; i++) q = enqueue(q, { kind: 'notification', title: `n${i}`, duration: 5000 }, 0);
  assert.deepEqual(q.map((a) => a.title), ['n1', 'n4', 'n5', 'n6']); // n1 e deja afișată
});

test('apelul are prioritate peste timer și media', () => {
  const l = layout([{ kind: 'media' }, { kind: 'call' }, { kind: 'timer' }]);
  assert.equal(l.pill.kind, 'call');
  assert.equal(l.bubble.kind, 'timer');
});

test('tranzitoria interactivă nu expiră cât e ținută (cursorul pe ea)', () => {
  let q = enqueue([], { kind: 'screenshot', duration: 6000, interactive: true }, 0);
  q = holdHead(q, 5900);
  q = holdHead(q, 9000);
  assert.ok(current(advance(q, 10000), 10000), 'încă afișată după ce a fost ținută');
  assert.equal(current(advance(q, 10600), 10600), null); // 1,5 s după ultima ținere
  // una obișnuită nu e ținută
  const a = enqueue([], { kind: 'alert', duration: 3000 }, 0);
  assert.deepEqual(holdHead(a, 2900), a);
});
