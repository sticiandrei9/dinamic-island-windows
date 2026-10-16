import { test } from 'node:test';
import assert from 'node:assert/strict';
import { setLang } from '../src/lib/i18n.js';
import { formatBytes, formatRate, level } from '../src/lib/format.js';

test('mărimi și viteze: unități zecimale, virgulă în ro, punct în en', () => {
  setLang('ro');
  assert.equal(formatRate(4_200_000), '4,2 MB/s');
  assert.equal(formatRate(120_000), '120 KB/s');
  assert.equal(formatRate(0), '0 KB/s');
  assert.equal(formatBytes(45_000_000), '45 MB');
  assert.equal(formatBytes(1_500_000_000), '1,5 GB');
  setLang('en');
  assert.equal(formatRate(4_200_000), '4.2 MB/s');
  setLang('ro');
});

test('level: warn de la prag, ok pentru valori lipsă', () => {
  assert.equal(level(81, 80), 'warn');
  assert.equal(level(80, 80), 'warn');
  assert.equal(level(79, 80), 'ok');
  assert.equal(level(null, 80), 'ok');
});

test('clipPreview: spații colapsate, tăiat la 40 de caractere cu „…”', async () => {
  const { clipPreview } = await import('../src/lib/format.js');
  assert.equal(clipPreview('  salut\n\n  lume\t!  '), 'salut lume !');
  assert.equal(clipPreview('a'.repeat(40)), 'a'.repeat(40));
  assert.equal(clipPreview('a'.repeat(41)), `${'a'.repeat(40)}…`);
  assert.equal(clipPreview('😀'.repeat(41)), `${'😀'.repeat(40)}…`); // perechile surogat nu se rup
});
