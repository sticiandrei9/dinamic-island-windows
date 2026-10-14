// Publică o versiune nouă: npm run release -- 0.2.0 "ce s-a schimbat"
// Doar build semnat, fără publicare (pentru teste): npm run release -- 0.1.1 "" --local http://127.0.0.1:8765
import { execSync, execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, copyFileSync, mkdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { validateVersion, setVersion, assetName, manifest, recoveryHint, VERSION_FILES } from './release-lib.mjs';

const REPO = 'sticiandrei9/dinamic-island-windows';
const KEY = join(homedir(), '.tauri', 'dynamic-island.key');
const PASSWORD = join(homedir(), '.tauri', 'dynamic-island.password');
const OUT = 'dist-release';

const args = process.argv.slice(2);
const localIdx = args.indexOf('--local');
const localUrl = localIdx >= 0 ? args.splice(localIdx, 2)[1] : null;
const [version, notes = ''] = args;
const git = (...a) => execFileSync('git', a, { stdio: 'inherit' });

function fail(stage, err) {
  console.error(`\n${err.message}\n\n${recoveryHint(stage, version, REPO)}\n`);
  process.exit(1);
}

if (!localUrl && execSync('git status --porcelain').toString().trim()) {
  throw new Error('working tree-ul nu e curat; fă commit întâi');
}
const current = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8')).version;
validateVersion(version, current);

// cheia se citește înainte de orice modificare, ca o cheie lipsă să nu lase fișierele schimbate
const signingEnv = {
  TAURI_SIGNING_PRIVATE_KEY: readFileSync(KEY, 'utf8'),
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD: readFileSync(PASSWORD, 'utf8').trim(),
};

for (const f of VERSION_FILES) writeFileSync(f, setVersion(f, readFileSync(f, 'utf8'), version));

try {
  execSync('npm run tauri build', { stdio: 'inherit', env: { ...process.env, ...signingEnv } });
} catch (err) {
  git('checkout', '--', ...VERSION_FILES, 'src-tauri/Cargo.lock');
  console.error('\nBuild-ul a eșuat; fișierele de versiune au fost readuse automat.');
  process.exit(1);
}

const built = `src-tauri/target/release/bundle/nsis/Dynamic Island_${version}_x64-setup.exe`;
mkdirSync(OUT, { recursive: true });
const asset = `${OUT}/${assetName(version)}`;
copyFileSync(built, asset);
const signature = readFileSync(`${built}.sig`, 'utf8').trim();
const latest = manifest({ version, notes, pubDate: new Date().toISOString(), signature, repo: REPO, baseUrl: localUrl ?? undefined });
writeFileSync(`${OUT}/latest.json`, JSON.stringify(latest, null, 2) + '\n');

if (localUrl) {
  console.log(`\nBuild semnat în ${OUT}/ (nepublicat). Fișierele de versiune au fost modificate, nu sunt în commit.`);
  process.exit(0);
}

git('add', ...VERSION_FILES, 'src-tauri/Cargo.lock');
git('commit', '-m', `release: v${version}`);
git('tag', `v${version}`);
try {
  git('push', '--atomic', 'origin', 'main', `v${version}`);
} catch (err) {
  fail('push', err);
}
try {
  execFileSync('gh', ['release', 'create', `v${version}`, asset, `${OUT}/latest.json`,
    '--repo', REPO, '--title', `v${version}`, '--notes', notes || `Versiunea ${version}`], { stdio: 'inherit' });
} catch (err) {
  fail('release', err);
}
console.log(`\nPublicat: https://github.com/${REPO}/releases/tag/v${version}`);
