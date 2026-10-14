import { test } from 'node:test';
import assert from 'node:assert/strict';
import { currentLine } from '../src/lib/lyrics.js';

const lines = [{ t: 10, text: 'a' }, { t: 15, text: 'b' }, { t: 20, text: 'c' }];

test('rândul curent după poziție', () => {
  assert.equal(currentLine(lines, 5), -1); // înainte de primul rând
  assert.equal(currentLine(lines, 10), 0);
  assert.equal(currentLine(lines, 17.5), 1);
  assert.equal(currentLine(lines, 999), 2);
  assert.equal(currentLine([], 3), -1);
});
