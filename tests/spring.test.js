import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Spring } from '../src/lib/spring.js';

function run(s, seconds) {
  let max = -Infinity;
  for (let t = 0; t < seconds; t += 1 / 60) { s.step(1 / 60); max = Math.max(max, s.value); }
  return max;
}

test('un spring nou e în repaus', () => {
  assert.equal(new Spring(5).isSettled(), true);
});

test('converge la țintă', () => {
  const s = new Spring(0, { response: 0.5, damping: 0.72 });
  s.setTarget(100);
  run(s, 3);
  assert.ok(s.isSettled());
  assert.ok(Math.abs(s.value - 100) < 0.05);
});

test('depășește ușor ținta când damping < 1 (efectul macOS)', () => {
  const s = new Spring(0, { response: 0.5, damping: 0.6 });
  s.setTarget(100);
  assert.ok(run(s, 2) > 100.5);
});

test('fără depășire la damping critic', () => {
  const s = new Spring(0, { response: 0.5, damping: 1 });
  s.setTarget(100);
  assert.ok(run(s, 3) <= 100.001);
});

test('snap sare direct la țintă', () => {
  const s = new Spring(0);
  s.setTarget(42);
  s.snap();
  assert.equal(s.value, 42);
  assert.ok(s.isSettled());
});
