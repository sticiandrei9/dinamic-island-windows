// Publică o versiune nouă: npm run release -- 0.2.0 "ce s-a schimbat"
// Doar build semnat, fără publicare (pentru teste): npm run release -- 0.1.1 "" --local http://127.0.0.1:8765
import { execSync, execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, copyFileSync, mkdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { validateVersion, setVersion, assetName, manifest } from './release-lib.mjs';

const REPO = 'sticiandrei9/dinamic-island-windows';
const KEY = join(homedir(), '.tauri', 'dynamic-island.key');
const PASSWORD = join(homedir(), '.tauri', 'dynamic-island.password');
const FILES = ['package.json', 'src-tauri/Cargo.toml', 'src-tauri/tauri.conf.json'];
const OUT = 'dist-release';

const args = process.argv.slice(2);
const localIdx = args.indexOf('--local');
const localUrl = localIdx >= 0 ? args.splice(localIdx, 2)[1] : null;
const [version, notes = ''] = args;
const git = (...a) => execFileSync('git', a, { stdio: 'inherit' });

if (!localUrl && execSync('git status --porcelain').toString().trim()) {
  throw new Error('working tree-ul nu e curat; fă commit întâi');
}
const current = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8')).version;
validateVersion(version, current);

for (const f of FILES) writeFileSync(f, setVersion(f, readFileSync(f, 'utf8'), version));

execSync('npm run tauri build', {
  stdio: 'inherit',
  env: {
    ...process.env,
    TAURI_SIGNING_PRIVATE_KEY: readFileSync(KEY, 'utf8'),
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: readFileSync(PASSWORD, 'utf8').trim(),
  },
});

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

git('add', ...FILES, 'src-tauri/Cargo.lock');
git('commit', '-m', `release: v${version}`);
git('tag', `v${version}`);
git('push', 'origin', 'main', '--tags');
execFileSync('gh', ['release', 'create', `v${version}`, asset, `${OUT}/latest.json`,
  '--repo', REPO, '--title', `v${version}`, '--notes', notes || `Versiunea ${version}`], { stdio: 'inherit' });
console.log(`\nPublicat: https://github.com/${REPO}/releases/tag/v${version}`);
