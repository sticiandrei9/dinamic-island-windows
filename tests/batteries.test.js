import { test } from 'node:test';
import assert from 'node:assert/strict';
import { visibleBatteries } from '../src/lib/batteries.js';

const lista = (n) => Array.from({ length: n }, (_, i) => ({ id: String(i + 1), name: `d${i + 1}`, battery: 50 }));

test('cel mult 3 baterii + rest', () => {
  const r = visibleBatteries(lista(5));
  assert.equal(r.shown.length, 3);
  assert.equal(r.more, 2);
});
test('sub limită: toate, fără rest', () => {
  const r = visibleBatteries(lista(2));
  assert.equal(r.shown.length, 2);
  assert.equal(r.more, 0);
  assert.deepEqual(visibleBatteries([]), { shown: [], more: 0 });
});
test('limita se poate schimba', () => {
  assert.equal(visibleBatteries(lista(5), 1).more, 4);
});
