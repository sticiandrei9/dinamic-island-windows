import { test } from 'node:test';
import assert from 'node:assert/strict';
import { setLang } from '../src/lib/i18n.js';
import { formatBytes, formatRate } from '../src/lib/format.js';

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
