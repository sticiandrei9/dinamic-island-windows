const p2 = (n) => String(n).padStart(2, '0');

// Timpul rămas, rotunjit în sus: „0:00” apare abia când timerul chiar s-a terminat.
export function formatClock(ms) {
  const s = Math.ceil(Math.max(0, ms) / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  return h > 0 ? `${h}:${p2(m)}:${p2(s % 60)}` : `${m}:${p2(s % 60)}`;
}

export function formatStopwatch(ms) {
  const tenths = Math.floor(Math.max(0, ms) / 100);
  const s = Math.floor(tenths / 10);
  return `${Math.floor(s / 60)}:${p2(s % 60)}.${tenths % 10}`;
}

// aceleași calcule ca în timer.rs (Clock::timer_remaining / stopwatch_elapsed)
export function remainingMs(countdown, now) {
  if (!countdown) return 0;
  return countdown.ends_at != null ? Math.max(0, countdown.ends_at - now) : countdown.remaining_ms;
}

export function elapsedMs(stopwatch, now) {
  if (!stopwatch) return 0;
  return stopwatch.base_ms + (stopwatch.started_at != null ? Math.max(0, now - stopwatch.started_at) : 0);
}
