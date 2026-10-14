import { test } from 'node:test';
import assert from 'node:assert/strict';
import { nextCamera, pickCamera } from '../src/lib/mirror.js';

const cams = [{ deviceId: 'a' }, { deviceId: 'b' }, { deviceId: 'c' }];

test('comutarea trece la camera următoare, circular', () => {
  assert.equal(nextCamera(cams, 'a'), 'b');
  assert.equal(nextCamera(cams, 'c'), 'a');
  assert.equal(nextCamera(cams, 'necunoscută'), 'a');
  assert.equal(nextCamera([], 'a'), null);
});

test('camera aleasă e folosită doar dacă există', () => {
  assert.equal(pickCamera(cams, 'b'), 'b');
  assert.equal(pickCamera(cams, 'deconectată'), null); // camera implicită
  assert.equal(pickCamera(cams, null), null);
});
