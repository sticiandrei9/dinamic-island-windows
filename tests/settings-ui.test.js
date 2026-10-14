import { test } from 'node:test';
import assert from 'node:assert/strict';
import { updateStatusText, monitorLabel } from '../src/lib/settings-ui.js';

const NOW = 1_000_000;

test('starea update-ului în cuvinte', () => {
  assert.equal(updateStatusText({ kind: 'idle' }, NOW), 'Nicio verificare încă');
  assert.equal(updateStatusText({ kind: 'checking' }, NOW), 'Se verifică…');
  assert.equal(updateStatusText({ kind: 'up_to_date', checked_at: NOW - 20 }, NOW), 'La zi · verificat acum');
  assert.equal(updateStatusText({ kind: 'up_to_date', checked_at: NOW - 5 * 60 }, NOW), 'La zi · verificat acum 5 minute');
  assert.equal(updateStatusText({ kind: 'up_to_date', checked_at: NOW - 60 }, NOW), 'La zi · verificat acum un minut');
  assert.equal(updateStatusText({ kind: 'up_to_date', checked_at: NOW - 2 * 3600 }, NOW), 'La zi · verificat acum 2 ore');
  assert.equal(updateStatusText({ kind: 'up_to_date', checked_at: NOW - 3600 }, NOW), 'La zi · verificat acum o oră');
  assert.equal(updateStatusText({ kind: 'downloading', version: '0.3.0' }, NOW), 'Se descarcă v0.3.0…');
  assert.equal(updateStatusText({ kind: 'ready', version: '0.3.0' }, NOW), 'v0.3.0 e gata de instalare');
  assert.equal(updateStatusText({ kind: 'error', msg: 'timeout', checked_at: NOW }, NOW), 'Eroare: timeout');
});

test('eticheta unui monitor', () => {
  assert.equal(monitorLabel({ name: '\\\\.\\DISPLAY2', width: 2560, height: 1440, primary: false }, 1), 'Monitor 2 · 2560×1440');
  assert.equal(monitorLabel({ name: '\\\\.\\DISPLAY1', width: 1920, height: 1080, primary: true }, 0), 'Monitor 1 · 1920×1080 (principal)');
});
