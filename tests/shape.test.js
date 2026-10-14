import { test } from 'node:test';
import assert from 'node:assert/strict';
import { islandPath, islandRect } from '../src/lib/shape.js';

test('path-ul începe la urechea stângă și se închide', () => {
  const d = islandPath(520, 200, 32, 10, 6);
  assert.ok(d.startsWith('M154 0'));
  assert.ok(d.trim().endsWith('Z'));
});

test('latura dreaptă urcă până la urechea dreaptă', () => {
  const d = islandPath(520, 200, 32, 10, 6);
  assert.ok(d.includes('L360 6'));
  assert.ok(d.includes('366 0'));
});

test('raza de jos e limitată la jumătate din înălțime', () => {
  const d = islandPath(520, 200, 32, 100, 0);
  assert.ok(d.includes('L160 16'));
});

test('lățime zero nu produce NaN', () => {
  assert.ok(!islandPath(520, 0, 32, 10, 7).includes('NaN'));
});

test('rect-ul include urechile și e centrat', () => {
  const r = islandRect(520, 200, 32, 6);
  assert.deepEqual(r, { x: 154, y: 0, w: 212, h: 32 });
  assert.equal(r.x + r.w / 2, 260);
});

test('forma poate fi desenată cu alt centru (bula de lângă pastilă)', () => {
  const d = islandPath(520, 32, 32, 16, 0, 400);
  assert.ok(d.startsWith('M384 0'));
  assert.ok(d.includes('L416 0'));
});
