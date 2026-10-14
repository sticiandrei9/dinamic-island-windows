import { test } from 'node:test';
import assert from 'node:assert/strict';
import { formatTime, currentPosition, dominantColor, sizeScale, waveLevel, seekFromPointer } from '../src/lib/util.js';

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

test('sizeScale: mică / normală / mare, necunoscut → normală', () => {
  assert.equal(sizeScale('small'), 0.85);
  assert.equal(sizeScale('normal'), 1);
  assert.equal(sizeScale('large'), 1.15);
  assert.equal(sizeScale('urias'), 1);
});

test('waveLevel: undele difuzorului după volum', () => {
  assert.equal(waveLevel(0, false), 0);
  assert.equal(waveLevel(0.2, false), 1);
  assert.equal(waveLevel(0.5, false), 2);
  assert.equal(waveLevel(0.9, false), 3);
  assert.equal(waveLevel(0.9, true), 0);
});

test('seekFromPointer: poziția din bară, limitată la [0, durată]', () => {
  assert.equal(seekFromPointer(150, 100, 200, 240), 60);
  assert.equal(seekFromPointer(50, 100, 200, 240), 0);
  assert.equal(seekFromPointer(400, 100, 200, 240), 240);
  assert.equal(seekFromPointer(150, 100, 0, 240), 0);
});
