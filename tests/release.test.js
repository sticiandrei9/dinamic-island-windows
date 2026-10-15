import { test } from 'node:test';
import assert from 'node:assert/strict';
import { validateVersion, setVersion, assetName, manifest, recoveryHint, builtInstaller, VERSION_FILES, checkBranch } from '../scripts/release-lib.mjs';

test('installerul e căutat în folderul de compilare raportat de cargo', () => {
  assert.equal(builtInstaller('C:/Users/A/.cargo-target/di', '0.2.0'),
    'C:/Users/A/.cargo-target/di/release/bundle/nsis/Dynamic Island_0.2.0_x64-setup.exe');
});

test('după un build eșuat, indicația readuce fișierele de versiune', () => {
  const h = recoveryHint('build', '0.2.0', 'u/r');
  assert.match(h, /git checkout -- package\.json package-lock\.json src-tauri\/Cargo\.toml src-tauri\/tauri\.conf\.json src-tauri\/Cargo\.lock/);
});

test('după un push eșuat, indicația termină publicarea fără rebuild', () => {
  const h = recoveryHint('push', '0.2.0', 'u/r');
  assert.match(h, /git push --atomic origin main v0\.2\.0/);
  assert.match(h, /gh release create v0\.2\.0 dist-release\/DynamicIsland_0\.2\.0_x64-setup\.exe dist-release\/latest\.json --repo u\/r/);
});

test('după un gh eșuat, indicația doar creează release-ul', () => {
  const h = recoveryHint('release', '0.2.0', 'u/r');
  assert.doesNotMatch(h, /git push/);
  assert.match(h, /gh release create v0\.2\.0/);
});

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

test('package-lock.json primește versiunea (sus și la pachetul rădăcină)', () => {
  const lock = JSON.stringify({'name':'x','lockfileVersion':3,'packages':{'':{'name':'x'},'node_modules/a':{'version':'9.9.9'}}}, null, 2);
  const out = JSON.parse(setVersion('package-lock.json', lock, '0.3.0'));
  assert.equal(out.version, '0.3.0');
  assert.equal(out.packages[''].version, '0.3.0');
  assert.equal(out.packages['node_modules/a'].version, '9.9.9');
  assert.ok(VERSION_FILES.includes('package-lock.json'));
});

test('release doar de pe main', () => {
  assert.doesNotThrow(() => checkBranch('main'));
  assert.throws(() => checkBranch('v030'), /main/);
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
