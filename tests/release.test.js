import { test } from 'node:test';
import assert from 'node:assert/strict';
import { validateVersion, setVersion, assetName, manifest } from '../scripts/release-lib.mjs';

test('validateVersion acceptă doar semver mai mare', () => {
  assert.doesNotThrow(() => validateVersion('0.2.0', '0.1.0'));
  assert.doesNotThrow(() => validateVersion('0.1.10', '0.1.9'));
  assert.throws(() => validateVersion('0.1.0', '0.1.0'), /mai mare/);
  assert.throws(() => validateVersion('0.0.9', '0.1.0'), /mai mare/);
  assert.throws(() => validateVersion('v0.2', '0.1.0'), /invalid/);
  assert.throws(() => validateVersion(undefined, '0.1.0'), /invalid/);
});

test('setVersion modifică doar versiunea pachetului', () => {
  const cargo = '[package]\nname = "x"\nversion = "0.1.0"\n\n[dependencies]\nfoo = { version = "0.1.0" }\n';
  assert.equal(setVersion('Cargo.toml', cargo, '0.2.0'),
    '[package]\nname = "x"\nversion = "0.2.0"\n\n[dependencies]\nfoo = { version = "0.1.0" }\n');
  const json = '{\n  "name": "x",\n  "version": "0.1.0"\n}\n';
  assert.equal(JSON.parse(setVersion('package.json', json, '0.2.0')).version, '0.2.0');
  assert.ok(setVersion('package.json', json, '0.2.0').endsWith('\n'));
});

test('numele asset-ului nu are spații', () => {
  assert.equal(assetName('0.2.0'), 'DynamicIsland_0.2.0_x64-setup.exe');
});

test('latest.json are formatul cerut de updater', () => {
  const m = manifest({ version: '0.2.0', notes: 'n', pubDate: '2026-10-14T20:00:00.000Z', signature: 'SIG', repo: 'u/r' });
  assert.deepEqual(m, {
    version: '0.2.0',
    notes: 'n',
    pub_date: '2026-10-14T20:00:00.000Z',
    platforms: {
      'windows-x86_64': {
        signature: 'SIG',
        url: 'https://github.com/u/r/releases/download/v0.2.0/DynamicIsland_0.2.0_x64-setup.exe',
      },
    },
  });
});

test('latest.json poate indica un server local (pentru testul end-to-end)', () => {
  const m = manifest({ version: '0.1.1', notes: '', pubDate: 'd', signature: 'S', baseUrl: 'http://127.0.0.1:8765' });
  assert.equal(m.platforms['windows-x86_64'].url, 'http://127.0.0.1:8765/DynamicIsland_0.1.1_x64-setup.exe');
});
