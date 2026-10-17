import { test } from 'node:test';
import assert from 'node:assert/strict';
import { visibleButtons, sleepClick } from '../src/lib/controls.js';

test('butoane vizibile și ordinea', () => {
  assert.deepEqual(visibleButtons({ wifi: true, bluetooth: null, dark: false, night: null, dnd: null, mic_muted: false }), ['wifi', 'dark', 'mic', 'lock', 'sleep']);
});
test('fără stare, rămân doar blocarea și sleep', () => {
  assert.deepEqual(visibleButtons(null), ['lock', 'sleep']);
});
test('ordinea completă', () => {
  const all = { wifi: false, bluetooth: false, dark: false, night: false, dnd: false, mic_muted: false };
  assert.deepEqual(visibleButtons(all), ['wifi', 'bluetooth', 'dark', 'night', 'dnd', 'mic', 'lock', 'sleep']);
});
test('sleep cere al doilea click în 3 s', () => {
  let s = sleepClick(0, 1000); assert.equal(s.fire, false);
  s = sleepClick(s.armedAt, 2500); assert.equal(s.fire, true);
  s = sleepClick(1000, 5000); assert.equal(s.fire, false);
});
