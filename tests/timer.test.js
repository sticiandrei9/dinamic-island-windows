import { test } from 'node:test';
import assert from 'node:assert/strict';
import { formatClock, formatStopwatch, remainingMs, elapsedMs, sleepProgress } from '../src/lib/timer.js';

test('formatClock rotunjește în sus, ca timpul rămas să nu arate 0:00 înainte de final', () => {
  assert.equal(formatClock(299_001), '5:00');
  assert.equal(formatClock(299_000), '4:59');
  assert.equal(formatClock(0), '0:00');
  assert.equal(formatClock(3_725_000), '1:02:05');
});

test('formatStopwatch cu zecimi', () => {
  assert.equal(formatStopwatch(12_345), '0:12.3');
  assert.equal(formatStopwatch(61_000), '1:01.0');
});

test('timpul rămas și scurs din starea Rust', () => {
  assert.equal(remainingMs({ total_ms: 60_000, ends_at: 10_000, remaining_ms: 60_000 }, 4_000), 6_000);
  assert.equal(remainingMs({ total_ms: 60_000, ends_at: null, remaining_ms: 25_000 }, 99_000), 25_000);
  assert.equal(remainingMs(null, 0), 0);
  assert.equal(elapsedMs({ base_ms: 1_000, started_at: 5_000, laps: [] }, 7_000), 3_000);
  assert.equal(elapsedMs({ base_ms: 1_000, started_at: null, laps: [] }, 7_000), 1_000);
});

test('progresul somnului se golește de la 1 la 0 și rămâne în [0, 1]', () => {
  const s = { ends_at: 100_000, total_ms: 40_000 };
  assert.equal(sleepProgress(s, 60_000), 1);
  assert.equal(sleepProgress(s, 80_000), 0.5);
  assert.equal(sleepProgress(s, 100_000), 0);
  assert.equal(sleepProgress(s, 150_000), 0);
  assert.equal(sleepProgress(s, 0), 1);
  assert.equal(sleepProgress(null, 0), 0);
});
