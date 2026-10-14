import { test } from 'node:test';
import assert from 'node:assert/strict';
import { formatTime, currentPosition, dominantColor } from '../src/lib/util.js';

test('formatTime', () => {
  assert.equal(formatTime(0), '0:00');
  assert.equal(formatTime(65.9), '1:05');
  assert.equal(formatTime(3725), '1:02:05');
  assert.equal(formatTime(-3), '0:00');
  assert.equal(formatTime(NaN), '0:00');
});

test('currentPosition extrapolează doar când rulează', () => {
  const info = { position: 10, duration: 100, playing: true };
  assert.equal(currentPosition(info, 1000, 3000), 12);
  assert.equal(currentPosition({ ...info, playing: false }, 1000, 3000), 10);
});

test('currentPosition nu trece de durată și tratează null', () => {
  assert.equal(currentPosition({ position: 99, duration: 100, playing: true }, 0, 10000), 100);
  assert.equal(currentPosition(null, 0, 0), 0);
});

test('dominantColor alege culoarea saturată', () => {
  const px = [];
  for (let i = 0; i < 10; i++) px.push(255, 0, 0, 255);
  px.push(128, 128, 128, 255);
  assert.equal(dominantColor(px), 'rgb(255, 0, 0)');
});

test('dominantColor: transparent → alb, întunecat → luminat', () => {
  assert.equal(dominantColor([0, 0, 0, 0]), 'rgb(255, 255, 255)');
  assert.equal(dominantColor([100, 0, 0, 255]), 'rgb(140, 0, 0)');
});
