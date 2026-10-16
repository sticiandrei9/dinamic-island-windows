import { test } from 'node:test';
import assert from 'node:assert/strict';
import { paneAvailable } from '../src/lib/tabs.js';

test('fila fără modul e mereu disponibilă', () => {
  assert.equal(paneAvailable(null, { timer: false }), true);
  assert.equal(paneAvailable(undefined, undefined), true);
});

test('fila cu un modul: disponibilă doar dacă modulul nu e oprit', () => {
  assert.equal(paneAvailable('shelf', {}), true);
  assert.equal(paneAvailable('shelf', { shelf: true }), true);
  assert.equal(paneAvailable('shelf', { shelf: false }), false);
});

test('fila cu mai multe module: disponibilă dacă măcar unul nu e oprit (timer oprit, somn pornit)', () => {
  const pane = ['timer', 'sleep'];
  assert.equal(paneAvailable(pane, { timer: false, sleep: true }), true);
  assert.equal(paneAvailable(pane, { timer: true, sleep: false }), true);
  assert.equal(paneAvailable(pane, { timer: false, sleep: false }), false);
  assert.equal(paneAvailable(pane, {}), true);
});
