import { test } from 'node:test';
import assert from 'node:assert/strict';
import strings from '../src/locales.json' with { type: 'json' };
import { t, setLang, getLang, locale } from '../src/lib/i18n.js';

const LANGS = ['ro', 'en', 'ru'];
const params = (v) => [...JSON.stringify(v).matchAll(/\{(\w+)\}/g)].map((m) => m[1]).sort().join(',');

test('aceleași chei și aceiași parametri în cele 3 limbi', () => {
  const keys = Object.keys(strings.ro).sort();
  for (const l of LANGS) {
    assert.deepEqual(Object.keys(strings[l]).sort(), keys, `chei diferite în ${l}`);
    for (const k of keys) {
      const ro = strings.ro[k], v = strings[l][k];
      assert.equal(typeof v, typeof ro, `${l}.${k}: tip diferit`);
      const one = (x) => (typeof x === 'object' ? x.other : x);
      assert.equal(params(one(v)), params(one(ro)), `${l}.${k}: parametri diferiți`);
    }
  }
});

test('pluralele au formele cerute de fiecare limbă', () => {
  for (const l of LANGS) {
    const cats = new Intl.PluralRules(l).resolvedOptions().pluralCategories;
    for (const [k, v] of Object.entries(strings[l])) {
      if (typeof v !== 'object') continue;
      for (const c of cats) assert.ok(v[c], `${l}.${k}: lipsește forma „${c}”`);
    }
  }
});

test('traducere cu parametri și plurale', () => {
  setLang('ro');
  assert.equal(t('update.available', { v: '0.3.0' }), 'v0.3.0 disponibilă');
  assert.equal(t('shelf.count', { n: 1 }), '1 element');
  assert.equal(t('shelf.count', { n: 3 }), '3 elemente');
  assert.equal(t('shelf.count', { n: 20 }), '20 de elemente');
  setLang('ru');
  assert.equal(t('shelf.count', { n: 5 }), '5 элементов');
  assert.equal(t('shelf.count', { n: 22 }), '22 элемента');
  setLang('en');
  assert.equal(t('shelf.count', { n: 2 }), '2 items');
  assert.equal(locale(), 'en-GB');
});

test('limbă necunoscută ignorată, cheie lipsă afișată ca atare', () => {
  setLang('ro');
  setLang('xx');
  assert.equal(getLang(), 'ro');
  assert.equal(t('nu.exista'), 'nu.exista');
});
