import { test } from 'node:test';
import assert from 'node:assert/strict';
import { transitions, pillLabel, sinceLabel } from '../src/lib/claude.js';
import { setLang } from '../src/lib/i18n.js';

test('tranzițiile sesiunilor Claude', () => {
  const s = (id, state) => ({ id, state, project: 'insula' });
  assert.deepEqual(transitions([s('a', 'working')], [s('a', 'waiting')]), [{ id: 'a', project: 'insula', to: 'waiting' }]);
  assert.deepEqual(transitions([s('a', 'working')], [s('a', 'done')]), [{ id: 'a', project: 'insula', to: 'done' }]);
  assert.deepEqual(transitions([s('a', 'waiting')], [s('a', 'waiting')]), []);
  assert.deepEqual(transitions([], [s('a', 'done')]), []); // „done” fără o stare anterioară cunoscută (pornire) nu se anunță
  assert.deepEqual(transitions([s('a', 'waiting')], [s('a', 'working')]), []); // revenirea la lucru nu se anunță
});

test('eticheta pastilei', () => {
  setLang('ro');
  assert.equal(pillLabel([{ project: 'insula' }]), 'insula');
  assert.equal(pillLabel([{ project: 'a' }, { project: 'b' }, { project: 'c' }]), '3 sesiuni');
  setLang('en');
  assert.equal(pillLabel([{ project: 'a' }, { project: 'b' }]), '2 sessions');
});

test('de cât timp e în starea curentă', () => {
  setLang('ro');
  assert.equal(sinceLabel(1_000_000, 1_000_000 + 30_000), '');
  assert.equal(sinceLabel(1_000_000, 1_000_000 + 5 * 60_000), '5 min');
});
