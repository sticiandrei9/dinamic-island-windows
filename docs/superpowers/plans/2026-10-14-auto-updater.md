# Auto-updater — Plan de implementare

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Aplicația instalată își descarcă și instalează singură versiunile noi publicate pe GitHub Releases, iar publicarea se face cu o comandă.

**Architecture:** `tauri-plugin-updater` verifică un `latest.json` static din ultimul release GitHub. Modulul Rust `updater.rs` programează verificările și anunță insula înainte de instalare. Un script Node (`scripts/release.mjs`, cu funcții pure testate în `scripts/release-lib.mjs`) crește versiunea, construiește și semnează installerul, scrie `latest.json` și creează release-ul cu `gh`.

**Tech Stack:** Tauri 2 + `tauri-plugin-updater` 2, Node 24 (`node:test`), GitHub CLI 2.97 (logat ca `sticiandrei9`), PowerShell.

**Spec:** `docs/superpowers/specs/2026-10-14-auto-updater-design.md`

## Global Constraints

- Repo: `https://github.com/sticiandrei9/dinamic-island-windows`, branch `main`.
- Endpoint: `https://github.com/sticiandrei9/dinamic-island-windows/releases/latest/download/latest.json`.
- Cheia privată: `C:\Users\Andre\.tauri\dynamic-island.key`, parola în `C:\Users\Andre\.tauri\dynamic-island.password`. Niciodată în repo, niciodată în output-ul afișat.
- `windows.installMode = "quiet"`.
- Prima verificare la 30 s după pornire, apoi la fiecare 6 h. Nu se verifică în build-ul debug.
- Numele asset-ului: `DynamicIsland_<versiune>_x64-setup.exe` (fără spații).
- Licență MIT, „Copyright (c) 2026 Andrei”.
- Push, release-uri și instalarea peste copia lui Andrei se fac doar după confirmarea lui în chat.

## Review Focus

1. Release fără `latest.json`, sau repo încă privat (404) → aplicația nu se blochează și nu crapă; reîncearcă la 6 h (Task 3: eroarea e doar logată; verificat manual cu endpoint privat).
2. Versiune publicată mai mică sau egală cu cea instalată → nu se instalează nimic (comparatorul pluginului; Task 6 e2e verifică și cazul „nu e update”).
3. Release pornit dintr-un working tree murdar, sau cu o versiune ≤ cea curentă → scriptul refuză înainte de build (Task 4: `validateVersion`, `assertCleanTree`).
4. Semnătură greșită sau lipsă în `latest.json` → update-ul e respins (verificarea pluginului; Task 6: test negativ cu semnătură stricată).
5. Spații în numele installerului → URL-ul din `latest.json` corespunde exact asset-ului urcat (Task 4: `assetName`, `manifest`).

---

### Task 1: Repo — `main`, licență, README, remote

**Files:** Create `LICENSE`, `README.md`.

- [ ] **Step 1:** `git checkout master && git merge --ff-only feat/island && git branch -m master main`
- [ ] **Step 2:** Scrie `LICENSE` (textul MIT standard, `Copyright (c) 2026 Andrei`) și `README.md` (descriere, funcții, instalare din Releases, cum se publică o versiune: `npm run release -- X.Y.Z "note"`, unde stă cheia de semnare și de ce trebuie păstrată cu backup).
- [ ] **Step 3:** `git add LICENSE README.md && git commit -m "docs: licenta MIT si README"`
- [ ] **Step 4:** `git remote add origin https://github.com/sticiandrei9/dinamic-island-windows.git`
- [ ] **Step 5 (cere confirmare):** `git push -u origin main`
Expected: `main` apare pe GitHub; `gh repo view sticiandrei9/dinamic-island-windows --json isEmpty` → `false`.

---

### Task 2: Cheia de semnare

- [ ] **Step 1:** Generează o parolă aleatoare de 32 de caractere și scrie-o în `C:\Users\Andre\.tauri\dynamic-island.password`, fără să o afișezi.
- [ ] **Step 2:** `npx tauri signer generate --ci -w C:\Users\Andre\.tauri\dynamic-island.key -p <parola din fișier>`, apelat din PowerShell cu parola citită din fișier, nu tipărită.
Expected: există `dynamic-island.key` și `dynamic-island.key.pub`.
- [ ] **Step 3:** Restrânge drepturile ambelor fișiere secrete la utilizatorul curent: `icacls <f> /inheritance:r /grant:r "$env:USERNAME:(R,W)"`.
- [ ] **Step 4:** Verifică faptul că `git status` nu arată nimic din `.tauri`, pentru că e în afara repo-ului.

---

### Task 3: Updater în aplicație

**Files:**
- Create: `src-tauri/src/updater.rs`
- Modify: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/src/main.rs`, `src/index.html`, `src/styles.css`, `src/island.js`

**Interfaces:**
- Produces: evenimentul `update-installing` (payload: string cu versiunea); `updater::spawn(AppHandle)`; funcția pură `updater::next_delay(first: bool) -> Duration`.

- [ ] **Step 1: Test (în `updater.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prima_verificare_la_30s_apoi_la_6h() {
        assert_eq!(next_delay(true), Duration::from_secs(30));
        assert_eq!(next_delay(false), Duration::from_secs(6 * 3600));
    }
}
```
Run: `cargo test updater` → FAIL (nu compilează).

- [ ] **Step 2: Dependență și config**

`Cargo.toml`: `tauri-plugin-updater = "2"`.

`tauri.conf.json`: în `bundle` adaugă `"createUpdaterArtifacts": true`; la rădăcină adaugă:
```json
"plugins": {
  "updater": {
    "pubkey": "<conținutul din C:\\Users\\Andre\\.tauri\\dynamic-island.key.pub>",
    "endpoints": ["https://github.com/sticiandrei9/dinamic-island-windows/releases/latest/download/latest.json"],
    "windows": { "installMode": "quiet" }
  }
}
```

- [ ] **Step 3: `src-tauri/src/updater.rs`**

```rust
use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

pub fn next_delay(first: bool) -> Duration {
    if first { Duration::from_secs(30) } else { Duration::from_secs(6 * 3600) }
}

/// Verifică periodic GitHub Releases; dacă există o versiune nouă, anunță insula,
/// apoi o descarcă, o verifică (semnătura) și o instalează. Installerul repornește aplicația.
pub fn spawn(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let mut first = true;
        loop {
            tokio_sleep(next_delay(first)).await;
            first = false;
            if let Err(e) = check_and_install(&app).await {
                eprintln!("actualizare eșuată: {e}");
            }
        }
    });
}

async fn tokio_sleep(d: Duration) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(d)).await;
}

async fn check_and_install(app: &AppHandle) -> tauri_plugin_updater::Result<()> {
    if let Some(update) = app.updater()?.check().await? {
        let _ = app.emit("update-installing", update.version.clone());
        tokio_sleep(Duration::from_secs(2)).await;
        update.download_and_install(|_, _| {}, || {}).await?;
        app.restart();
    }
    Ok(())
}
```

- [ ] **Step 4: `main.rs`**: adaugă `mod updater;`, `.plugin(tauri_plugin_updater::Builder::new().build())` și `updater::spawn(app.handle().clone());` la finalul `setup`.

- [ ] **Step 5: UI**: în `index.html`, în `#stage` după `#compact`, adaugă `<div id="update" class="layer"></div>`. În `styles.css`:
```css
#update {
  height: 32px; display: flex; align-items: center; justify-content: center;
  transform: translateX(-50%); font-size: 12px; font-weight: 600; color: #30d158;
  opacity: 0; transition: opacity .3s ease; white-space: nowrap; pointer-events: none;
}
body.updating #update { opacity: 1; }
body.updating #compact { opacity: 0; }
```
În `island.js`, în ramura `if (tauri)`:
```js
  tauri.event.listen('update-installing', (e) => {
    document.getElementById('update').textContent = `Actualizare la v${e.payload}`;
    document.body.classList.add('updating');
    setExpanded(false);
  });
```
și în `render()`: `document.getElementById('update').style.width = `${Math.max(0, w)}px`;`

- [ ] **Step 6:** `cargo test` → PASS; `npm test` → PASS; `cargo build` fără avertismente.
- [ ] **Step 7:** Commit `feat: auto-updater din GitHub Releases`.

---

### Task 4: Scriptul de release

**Files:**
- Create: `scripts/release-lib.mjs`, `scripts/release.mjs`, `tests/release.test.js`
- Modify: `package.json` (script `"release": "node scripts/release.mjs"`)

**Interfaces:**
- Produces: `validateVersion(next, current)` (aruncă eroare dacă versiunea e invalidă sau nu e mai mare), `setVersion(fileName, text, version) -> text`, `assetName(version)`, `manifest({version, notes, pubDate, signature, repo})`.

- [ ] **Step 1: Teste (`tests/release.test.js`)**

```js
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { validateVersion, setVersion, assetName, manifest } from '../scripts/release-lib.mjs';

test('validateVersion acceptă doar semver mai mare', () => {
  assert.doesNotThrow(() => validateVersion('0.2.0', '0.1.0'));
  assert.doesNotThrow(() => validateVersion('0.1.10', '0.1.9'));
  assert.throws(() => validateVersion('0.1.0', '0.1.0'), /mai mare/);
  assert.throws(() => validateVersion('0.0.9', '0.1.0'), /mai mare/);
  assert.throws(() => validateVersion('v0.2', '0.1.0'), /invalid/);
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
```
Run: `npm test` → FAIL (modulul lipsește).

- [ ] **Step 2: `scripts/release-lib.mjs`**

```js
const SEMVER = /^(\d+)\.(\d+)\.(\d+)$/;

export function validateVersion(next, current) {
  const a = SEMVER.exec(next);
  const b = SEMVER.exec(current);
  if (!a) throw new Error(`versiune invalidă: ${next} (format X.Y.Z)`);
  const cmp = [1, 2, 3].map((i) => Number(a[i]) - Number(b[i])).find((d) => d !== 0) ?? 0;
  if (cmp <= 0) throw new Error(`versiunea ${next} trebuie să fie mai mare decât ${current}`);
}

export function setVersion(fileName, text, version) {
  if (fileName.endsWith('.json')) {
    const obj = JSON.parse(text);
    obj.version = version;
    return JSON.stringify(obj, null, 2) + '\n';
  }
  // Cargo.toml: doar prima linie `version = "..."` (cea din [package])
  return text.replace(/^version = "[^"]*"/m, `version = "${version}"`);
}

export function assetName(version) {
  return `DynamicIsland_${version}_x64-setup.exe`;
}

export function manifest({ version, notes, pubDate, signature, repo }) {
  return {
    version,
    notes,
    pub_date: pubDate,
    platforms: {
      'windows-x86_64': {
        signature,
        url: `https://github.com/${repo}/releases/download/v${version}/${assetName(version)}`,
      },
    },
  };
}
```

- [ ] **Step 3:** `npm test` → PASS.

- [ ] **Step 4: `scripts/release.mjs`**

```js
import { execSync } from 'node:child_process';
import { readFileSync, writeFileSync, copyFileSync, mkdirSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';
import { validateVersion, setVersion, assetName, manifest } from './release-lib.mjs';

const REPO = 'sticiandrei9/dinamic-island-windows';
const KEY = join(homedir(), '.tauri', 'dynamic-island.key');
const PASSWORD = join(homedir(), '.tauri', 'dynamic-island.password');
const FILES = ['package.json', 'src-tauri/Cargo.toml', 'src-tauri/tauri.conf.json'];

const [version, notes = ''] = process.argv.slice(2);
const run = (cmd, env) => execSync(cmd, { stdio: 'inherit', env: { ...process.env, ...env } });

if (execSync('git status --porcelain').toString().trim()) throw new Error('working tree-ul nu e curat; fă commit întâi');
const current = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8')).version;
validateVersion(version, current);

for (const f of FILES) writeFileSync(f, setVersion(f, readFileSync(f, 'utf8'), version));

run('npm run tauri build', {
  TAURI_SIGNING_PRIVATE_KEY: readFileSync(KEY, 'utf8'),
  TAURI_SIGNING_PRIVATE_KEY_PASSWORD: readFileSync(PASSWORD, 'utf8').trim(),
});

const bundle = 'src-tauri/target/release/bundle/nsis';
const built = `${bundle}/Dynamic Island_${version}_x64-setup.exe`;
mkdirSync('dist-release', { recursive: true });
const asset = `dist-release/${assetName(version)}`;
copyFileSync(built, asset);
const signature = readFileSync(`${built}.sig`, 'utf8').trim();
writeFileSync('dist-release/latest.json', JSON.stringify(manifest({ version, notes, pubDate: new Date().toISOString(), signature, repo: REPO }), null, 2));

run(`git add ${FILES.join(' ')} src-tauri/Cargo.lock`);
run(`git commit -m "release: v${version}"`);
run(`git tag v${version}`);
run('git push origin main --tags');
run(`gh release create v${version} "${asset}" dist-release/latest.json --repo ${REPO} --title "v${version}" --notes "${notes.replace(/"/g, "'") || `Versiunea ${version}`}"`);
console.log(`\nPublicat: https://github.com/${REPO}/releases/tag/v${version}`);
```
Adaugă `dist-release/` în `.gitignore`. Adaugă `"release": "node scripts/release.mjs"` în `package.json`.

- [ ] **Step 5:** Commit `feat: script de release cu semnare si latest.json`.

---

### Task 5: Build 0.1.0 semnat și verificare că eșecul de rețea e inofensiv

- [ ] **Step 1:** Construiește release-ul semnat cu variabilele de mediu din fișierele cheii.
Expected: există `Dynamic Island_0.1.0_x64-setup.exe.sig`.
- [ ] **Step 2:** Pornește `target/release/dynamic-island.exe` cu stderr redirecționat, doar dacă nu rulează copia instalată a lui Andrei (altfel cere-i să o închidă). Așteaptă 40 s.
Expected: în log apare „actualizare eșuată: …” (repo privat sau release inexistent), iar aplicația rulează în continuare și insula răspunde la hover.

---

### Task 6: Test end-to-end local (cere confirmare: înlocuiește copia instalată)

- [ ] **Step 1:** Construiește un build 0.1.0 „vechi” cu `npx tauri build --config tmp-e2e.json`, unde `tmp-e2e.json` setează `plugins.updater.endpoints = ["http://127.0.0.1:8765/latest.json"]` și `dangerousInsecureTransportProtocol: true`. Instalează-l (installer quiet `/S`).
- [ ] **Step 2:** Construiește 0.1.1 semnat (fără override), scrie `latest.json` cu URL `http://127.0.0.1:8765/<asset>`, servește folderul cu `npx http-server -p 8765`.
- [ ] **Step 3:** Pornește aplicația instalată (0.1.0) și așteaptă ~60 s.
Expected: insula arată „Actualizare la v0.1.1”, apoi aplicația repornește; `(Get-Item "$env:LOCALAPPDATA\Dynamic Island\dynamic-island.exe").VersionInfo.ProductVersion` → `0.1.1`.
- [ ] **Step 4 (negativ):** Strică semnătura în `latest.json`, setează versiunea 0.1.2 și repornește aplicația.
Expected: nu se instalează nimic, iar în log apare o eroare de semnătură.
- [ ] **Step 5:** Reinstalează un build 0.1.1 normal (endpoint GitHub) peste copia de test, ca Andrei să rămână cu versiunea corectă. Șterge `tmp-e2e.json` și readu fișierele de versiune la starea din git (`git checkout -- package.json src-tauri/Cargo.toml src-tauri/tauri.conf.json src-tauri/Cargo.lock`). Fără commit pentru build-urile de test: versiunea din repo rămâne 0.1.0 până la release.
Expected: `git status --porcelain` e gol.

---

### Task 7: Primul release real (cere confirmare)

- [ ] **Step 1:** `npm run release -- 0.1.1 "Reparații: sursa media, fullscreen, timeout"`
Expected: release-ul `v0.1.1` pe GitHub conține `DynamicIsland_0.1.1_x64-setup.exe` și `latest.json`.
- [ ] **Step 2:** `gh release view v0.1.1 --repo sticiandrei9/dinamic-island-windows --json assets` → 2 asset-uri.
