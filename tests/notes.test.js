import { test } from 'node:test';
import assert from 'node:assert/strict';
import { sortNotes, openCount } from '../src/lib/notes.js';

test('ordinea notițelor: nebifatele întâi, apoi după data creării descrescător', () => {
  const l = [{ id: 1, done: true, at: 3 }, { id: 2, done: false, at: 1 }, { id: 3, done: false, at: 2 }];
  assert.deepEqual(sortNotes(l).map((n) => n.id), [3, 2, 1]);
  assert.equal(openCount(l), 2);
});

test('bifatele între ele tot după dată descrescător; lista originală rămâne neatinsă', () => {
  const l = [{ id: 1, done: true, at: 1 }, { id: 2, done: true, at: 5 }, { id: 3, done: false, at: 2 }];
  assert.deepEqual(sortNotes(l).map((n) => n.id), [3, 2, 1]);
  assert.deepEqual(l.map((n) => n.id), [1, 2, 3]);
});

test('aceeași dată: cea adăugată mai târziu (id mai mare) urcă', () => {
  const l = [{ id: 1, done: false, at: 7 }, { id: 2, done: false, at: 7 }];
  assert.deepEqual(sortNotes(l).map((n) => n.id), [2, 1]);
});

test('listă goală', () => {
  assert.deepEqual(sortNotes([]), []);
  assert.equal(openCount([]), 0);
});
