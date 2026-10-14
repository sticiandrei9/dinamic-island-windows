# Dynamic Island pentru Windows — Plan de implementare

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** O aplicație Tauri care afișează sus pe centru o insulă în stil MacBook, cu media curentă (control play/pauză/next/prev) și aplicațiile deschise (click = adu în față).

**Architecture:** Partea Rust (Tauri 2) citește media prin GSMTC (WinRT), enumeră ferestrele prin Win32 și controlează fereastra (click-through, ascundere la fullscreen, tray, autostart). Frontendul e HTML/CSS/JS fără framework: forma insulei e un path SVG animat cu un spring fizic, iar conținutul e în straturi suprapuse.

**Tech Stack:** Rust stable (MSVC), Tauri 2, crate `windows` 0.58, `tauri-plugin-autostart` 2, `tauri-plugin-single-instance` 2, `png`, `base64`; JS ES modules, teste cu `node --test` (Node 24).

**Spec:** `docs/superpowers/specs/2026-10-14-dynamic-island-design.md`

## Global Constraints

- Platformă: Windows 10/11 x64, doar monitorul principal.
- Fereastră: 520×260 CSS px, transparentă, fără margini, fără umbră, mereu deasupra, fără taskbar, poziționată la y=0, centrată orizontal.
- Insula: negru pur `#000`, lipită de marginea de sus, cu „urechi” concave sus (raza 7px).
- Dimensiuni (lățime×înălțime, raza de jos): repaus 190×32 r10; activă 300×32 r10; extinsă cu media 420×206 r36; extinsă fără media 360×140 r32.
- Spring: response 0.5s, damping 0.72 pentru formă.
- Font: `-apple-system, "SF Pro Display", "Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif`.
- Text gri secundar `#8e8e93`. Toate textele vizibile sunt în română.
- Consum țintă: procesul `dynamic-island.exe` < 60 MB RAM, CPU ~0% în repaus (procesele WebView2 se măsoară separat, tipic ~60–100 MB în total).
- Autostart activat implicit la prima pornire, doar în build-ul release.

## Review Focus

1. Video sau joc pe tot ecranul → insula se ascunde și revine după ieșire (`covers_monitor`, Task 4).
2. Mai multe surse media (Spotify pe pauză, YouTube rulează) → insula arată și controlează sursa care rulează (`choose_session_index`, Task 5).
3. Coperta vine după titlu (Spotify) → coperta se reîncearcă, nu rămâne placeholder (`ThumbCache`, Task 5).
4. Seek, sau pauză cu poziție extrapolată → bara de progres nu sare și nu fuge (`live_position`, `should_emit`, Task 5; `currentPosition`, Task 2).
5. Aplicația pornită de două ori (autostart + manual) → o singură insulă (single-instance, Task 7, verificare manuală).

---

## Structura fișierelor

```
dynamic-island/
  package.json                 — scripturi npm (test, tauri) + @tauri-apps/cli
  app-icon.svg                 — sursa iconiței
  tests/spring.test.js, shape.test.js, util.test.js
  src/                         — frontend (frontendDist)
    index.html, styles.css
    island.js                  — controller: stări, spring, hover, raportare rect
    media-view.js              — randare media + butoane
    apps-view.js               — rândul de aplicații
    lib/spring.js              — spring fizic (pur)
    lib/shape.js               — path SVG pentru insulă + rect (pur)
    lib/util.js                — formatTime, currentPosition, dominantColor (pur)
  src-tauri/
    Cargo.toml, build.rs, tauri.conf.json, capabilities/default.json, icons/
    src/main.rs                — asamblare
    src/platform.rs            — HWND, poziționare, no-activate, show/hide, cursor, fullscreen
    src/hittest.rs             — click-through + evenimente hover + ascundere la fullscreen
    src/media.rs               — GSMTC
    src/windows_list.rs        — ferestre deschise + iconițe + focus
    src/tray.rs                — tray + autostart
```

---

### Task 1: Schelet Tauri — pastila neagră sus pe centru

**Files:**
- Create: `package.json`, `.gitignore`, `app-icon.svg`, `src/index.html` (temporar), `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/src/main.rs`, `src-tauri/src/platform.rs`

**Interfaces:**
- Produces: `platform::hwnd_of(&WebviewWindow) -> Option<HWND>`, `platform::place_top_center(&WebviewWindow) -> tauri::Result<()>`, `platform::make_no_activate(&WebviewWindow)`, `platform::show_no_activate(&WebviewWindow)`, `platform::hide(&WebviewWindow)`

- [ ] **Step 1: Verifică toolchain-ul**

Run: `rustup default stable-msvc; cargo --version; rustc --version`
Expected: versiuni afișate (redeschide shell-ul dacă `cargo` nu e găsit; e în `%USERPROFILE%\.cargo\bin`).

- [ ] **Step 2: Creează `package.json` și `.gitignore`**

```json
{
  "name": "dynamic-island",
  "private": true,
  "type": "module",
  "scripts": {
    "test": "node --test tests/",
    "tauri": "tauri"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2"
  }
}
```

`.gitignore`:
```
node_modules/
src-tauri/target/
src-tauri/gen/
```

Run: `npm install`

- [ ] **Step 3: Creează `app-icon.svg` și generează iconițele**

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512">
  <rect x="40" y="176" width="432" height="160" rx="80" fill="#000" stroke="#fff" stroke-width="14"/>
  <circle cx="396" cy="256" r="30" fill="#30d158"/>
</svg>
```

Run: `npx tauri icon app-icon.svg`
Expected: `src-tauri/icons/` conține `icon.ico`, `32x32.png`, `128x128.png` etc.

- [ ] **Step 4: Creează `src-tauri/Cargo.toml` și `build.rs`**

```toml
[package]
name = "dynamic-island"
version = "0.1.0"
edition = "2021"

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-autostart = "2"
tauri-plugin-single-instance = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
base64 = "0.22"
png = "0.17"

[dependencies.windows]
version = "0.58"
features = [
  "Foundation",
  "Foundation_Collections",
  "Media_Control",
  "Storage_Streams",
  "Win32_Foundation",
  "Win32_Graphics_Dwm",
  "Win32_Graphics_Gdi",
  "Win32_Storage_FileSystem",
  "Win32_System_Com",
  "Win32_System_Threading",
  "Win32_UI_Input_KeyboardAndMouse",
  "Win32_UI_Shell",
  "Win32_UI_WindowsAndMessaging",
]

[profile.release]
codegen-units = 1
lto = true
opt-level = "s"
strip = true
```

`build.rs`:
```rust
fn main() {
    tauri_build::build()
}
```

- [ ] **Step 5: Creează `tauri.conf.json` și `capabilities/default.json`**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Dynamic Island",
  "version": "0.1.0",
  "identifier": "ro.andrei.dynamicisland",
  "build": { "frontendDist": "../src" },
  "app": {
    "withGlobalTauri": true,
    "windows": [
      {
        "label": "island",
        "title": "Dynamic Island",
        "url": "index.html",
        "width": 520,
        "height": 260,
        "decorations": false,
        "transparent": true,
        "alwaysOnTop": true,
        "skipTaskbar": true,
        "resizable": false,
        "maximizable": false,
        "minimizable": false,
        "shadow": false,
        "focus": false,
        "visible": false
      }
    ],
    "security": { "csp": null }
  },
  "bundle": {
    "active": true,
    "targets": ["nsis"],
    "icon": ["icons/32x32.png", "icons/128x128.png", "icons/icon.ico"]
  }
}
```

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "windows": ["island"],
  "permissions": ["core:default"]
}
```

- [ ] **Step 6: Creează `src-tauri/src/platform.rs`**

```rust
use tauri::{PhysicalPosition, WebviewWindow};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, ShowWindow, GWL_EXSTYLE, SW_HIDE, SW_SHOWNOACTIVATE,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

/// HWND-ul ferestrei Tauri, convertit în tipul din crate-ul nostru `windows`.
pub fn hwnd_of(win: &WebviewWindow) -> Option<HWND> {
    win.hwnd().ok().map(|h| HWND(h.0 as *mut core::ffi::c_void))
}

/// Așază fereastra lipită de marginea de sus, centrată pe monitorul principal.
pub fn place_top_center(win: &WebviewWindow) -> tauri::Result<()> {
    let monitor = match win.primary_monitor()? {
        Some(m) => Some(m),
        None => win.current_monitor()?,
    };
    if let Some(m) = monitor {
        let size = win.outer_size()?;
        let x = m.position().x + (m.size().width as i32 - size.width as i32) / 2;
        win.set_position(PhysicalPosition::new(x, m.position().y))?;
    }
    Ok(())
}

/// Click-urile pe insulă nu fură focusul aplicației active; fereastra nu apare în Alt+Tab.
pub fn make_no_activate(win: &WebviewWindow) {
    if let Some(h) = hwnd_of(win) {
        unsafe {
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            let extra = (WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) as isize;
            SetWindowLongPtrW(h, GWL_EXSTYLE, ex | extra);
        }
    }
}

pub fn show_no_activate(win: &WebviewWindow) {
    if let Some(h) = hwnd_of(win) {
        unsafe {
            let _ = ShowWindow(h, SW_SHOWNOACTIVATE);
        }
    }
}

pub fn hide(win: &WebviewWindow) {
    if let Some(h) = hwnd_of(win) {
        unsafe {
            let _ = ShowWindow(h, SW_HIDE);
        }
    }
}
```

- [ ] **Step 7: Creează `src-tauri/src/main.rs`**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod platform;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let win = app.get_webview_window("island").expect("fereastra island lipsește");
            platform::place_top_center(&win)?;
            platform::make_no_activate(&win);
            platform::show_no_activate(&win);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("eroare la pornirea aplicației");
}
```

- [ ] **Step 8: Creează `src/index.html` temporar**

```html
<!doctype html>
<html lang="ro">
<head><meta charset="utf-8" /><title>Dynamic Island</title>
<style>
  html, body { margin: 0; background: transparent; }
  .pill { width: 190px; height: 32px; margin: 0 auto; background: #000; border-radius: 0 0 10px 10px; }
</style></head>
<body><div class="pill"></div></body>
</html>
```

- [ ] **Step 9: Rulează și verifică vizual**

Run: `npm run tauri dev` (prima compilare durează câteva minute)
Expected: o pastilă neagră sus pe centru, fără fundal alb, fără bordură sau umbră, fără buton în taskbar. Închide cu Ctrl+C în terminal.

- [ ] **Step 10: Commit**

```bash
git add .
git commit -m "feat: schelet Tauri cu fereastra insulei sus pe centru"
```

---

### Task 2: Module JS pure — spring, formă, utilitare

**Files:**
- Create: `src/lib/spring.js`, `src/lib/shape.js`, `src/lib/util.js`
- Test: `tests/spring.test.js`, `tests/shape.test.js`, `tests/util.test.js`

**Interfaces:**
- Produces:
  - `class Spring(value, {response, damping})` cu `setTarget(t)`, `step(dt) -> bool (settled)`, `isSettled()`, `snap()`, câmpul `value`
  - `islandPath(canvasWidth, w, h, r, ear) -> string`, `islandRect(canvasWidth, w, h, ear) -> {x,y,w,h}`
  - `formatTime(sec) -> string`, `currentPosition(info, receivedAtMs, nowMs) -> number`, `dominantColor(rgbaBytes) -> "rgb(r, g, b)"`

- [ ] **Step 1: Scrie testele**

`tests/spring.test.js`:
```js
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { Spring } from '../src/lib/spring.js';

function run(s, seconds) {
  let max = -Infinity;
  for (let t = 0; t < seconds; t += 1 / 60) { s.step(1 / 60); max = Math.max(max, s.value); }
  return max;
}

test('un spring nou e în repaus', () => {
  assert.equal(new Spring(5).isSettled(), true);
});

test('converge la țintă', () => {
  const s = new Spring(0, { response: 0.5, damping: 0.72 });
  s.setTarget(100);
  run(s, 3);
  assert.ok(s.isSettled());
  assert.ok(Math.abs(s.value - 100) < 0.05);
});

test('depășește ușor ținta când damping < 1 (efectul macOS)', () => {
  const s = new Spring(0, { response: 0.5, damping: 0.6 });
  s.setTarget(100);
  assert.ok(run(s, 2) > 100.5);
});

test('fără depășire la damping critic', () => {
  const s = new Spring(0, { response: 0.5, damping: 1 });
  s.setTarget(100);
  assert.ok(run(s, 3) <= 100.001);
});

test('snap sare direct la țintă', () => {
  const s = new Spring(0);
  s.setTarget(42);
  s.snap();
  assert.equal(s.value, 42);
  assert.ok(s.isSettled());
});
```

`tests/shape.test.js`:
```js
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
```

`tests/util.test.js`:
```js
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
```

- [ ] **Step 2: Rulează testele, trebuie să pice**

Run: `npm test`
Expected: FAIL (modulele nu există).

- [ ] **Step 3: Implementează `src/lib/spring.js`**

```js
// Spring fizic (Euler semi-implicit cu sub-pași), parametrizat ca în SwiftUI:
// response = perioada în secunde, damping = fracția de amortizare (1 = fără depășire).
export class Spring {
  constructor(value, { response = 0.5, damping = 0.72 } = {}) {
    this.value = value;
    this.target = value;
    this.velocity = 0;
    const omega = (2 * Math.PI) / response;
    this.k = omega * omega;
    this.c = 2 * damping * omega;
  }

  setTarget(target) {
    this.target = target;
  }

  step(dt) {
    const sub = Math.max(1, Math.ceil(dt * 240));
    const h = dt / sub;
    for (let i = 0; i < sub; i++) {
      const force = -this.k * (this.value - this.target) - this.c * this.velocity;
      this.velocity += force * h;
      this.value += this.velocity * h;
    }
    return this.isSettled();
  }

  isSettled() {
    return Math.abs(this.value - this.target) < 0.05 && Math.abs(this.velocity) < 0.05;
  }

  snap() {
    this.value = this.target;
    this.velocity = 0;
  }
}
```

- [ ] **Step 4: Implementează `src/lib/shape.js`**

```js
// Forma notch-ului de MacBook: lipit de marginea de sus, colțuri rotunjite jos
// și „urechi” concave sus, care se topesc în marginea ecranului.
const K = 0.5523; // aproximarea unui sfert de cerc cu o curbă Bézier cubică

const f = (n) => Math.round(n * 100) / 100;

export function islandPath(canvasWidth, w, h, r, ear) {
  w = Math.max(0, w);
  h = Math.max(0, h);
  r = Math.max(0, Math.min(r, h / 2, w / 2));
  ear = Math.max(0, Math.min(ear, h - r, w / 4));
  const x0 = (canvasWidth - w) / 2;
  const x1 = x0 + w;
  return [
    `M${f(x0 - ear)} 0`,
    `C${f(x0 - ear + K * ear)} 0 ${f(x0)} ${f(ear - K * ear)} ${f(x0)} ${f(ear)}`,
    `L${f(x0)} ${f(h - r)}`,
    `C${f(x0)} ${f(h - r + K * r)} ${f(x0 + r - K * r)} ${f(h)} ${f(x0 + r)} ${f(h)}`,
    `L${f(x1 - r)} ${f(h)}`,
    `C${f(x1 - r + K * r)} ${f(h)} ${f(x1)} ${f(h - r + K * r)} ${f(x1)} ${f(h - r)}`,
    `L${f(x1)} ${f(ear)}`,
    `C${f(x1)} ${f(ear - K * ear)} ${f(x1 + ear - K * ear)} 0 ${f(x1 + ear)} 0`,
    'Z',
  ].join(' ');
}

export function islandRect(canvasWidth, w, h, ear) {
  return { x: (canvasWidth - w) / 2 - ear, y: 0, w: w + 2 * ear, h };
}
```

- [ ] **Step 5: Implementează `src/lib/util.js`**

```js
export function formatTime(sec) {
  if (!Number.isFinite(sec) || sec < 0) sec = 0;
  const s = Math.floor(sec);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = s % 60;
  const p = (n) => String(n).padStart(2, '0');
  return h > 0 ? `${h}:${p(m)}:${p(ss)}` : `${m}:${p(ss)}`;
}

// Poziția redării acum, extrapolată din ultimul eveniment primit de la Rust.
export function currentPosition(info, receivedAtMs, nowMs) {
  if (!info) return 0;
  let pos = info.position + (info.playing ? (nowMs - receivedAtMs) / 1000 : 0);
  if (info.duration > 0) pos = Math.min(pos, info.duration);
  return Math.max(0, pos);
}

// Culoarea de accent a copertei: medie ponderată cu saturația, luminată ca să se vadă pe negru.
export function dominantColor(rgba) {
  let r = 0, g = 0, b = 0, wsum = 0;
  for (let i = 0; i + 3 < rgba.length; i += 4) {
    const R = rgba[i], G = rgba[i + 1], B = rgba[i + 2], A = rgba[i + 3];
    if (A < 128) continue;
    const max = Math.max(R, G, B);
    const min = Math.min(R, G, B);
    const sat = max === 0 ? 0 : (max - min) / max;
    const w = sat * sat * (max / 255) + 0.001;
    r += R * w; g += G * w; b += B * w; wsum += w;
  }
  if (wsum === 0) return 'rgb(255, 255, 255)';
  let R = r / wsum, G = g / wsum, B = b / wsum;
  const max = Math.max(R, G, B);
  if (max < 140) {
    const k = 140 / Math.max(max, 1);
    R *= k; G *= k; B *= k;
  }
  const c = (v) => Math.round(Math.min(255, v));
  return `rgb(${c(R)}, ${c(G)}, ${c(B)})`;
}
```

- [ ] **Step 6: Rulează testele, trebuie să treacă**

Run: `npm test`
Expected: toate PASS.

- [ ] **Step 7: Commit**

```bash
git add src/lib tests
git commit -m "feat: spring, forma insulei si utilitare JS, cu teste"
```

---

### Task 3: Interfața insulei — stări, animații, design MacBook (cu mod demo)

**Files:**
- Modify (rescriere): `src/index.html`
- Create: `src/styles.css`, `src/island.js`, `src/media-view.js`, `src/apps-view.js`

**Interfaces:**
- Consumes: `Spring`, `islandPath`, `islandRect`, `formatTime`, `currentPosition`, `dominantColor` (Task 2)
- Produces (contract cu Rust, implementat în Task 4–6):
  - JS → Rust: `invoke('set_island_rect', { rect: {x,y,w,h} })` (CSS px relativ la fereastră), `invoke('media_control', { action: 'play_pause'|'next'|'prev' })`, `invoke('list_windows') -> [{hwnd, title, exe, icon}]`, `invoke('focus_window', { hwnd })`
  - Rust → JS: evenimentul `island-hover` (payload `bool`) și `media-update` (payload `null` sau `{title, artist, album, thumbnail, playing, position, duration, source}`)
  - Fără Tauri (deschis în browser), `?demo` simulează media și aplicațiile, iar hover-ul vine din `pointermove`.

- [ ] **Step 1: Scrie `src/index.html`**

```html
<!doctype html>
<html lang="ro">
<head>
  <meta charset="utf-8" />
  <title>Dynamic Island</title>
  <link rel="stylesheet" href="styles.css" />
</head>
<body>
  <div id="stage">
    <svg id="shape" width="520" height="260" viewBox="0 0 520 260" aria-hidden="true">
      <path id="shape-path" d="" />
    </svg>

    <div id="compact" class="layer">
      <img id="c-art" class="art" alt="" />
      <div class="viz"><i></i><i></i><i></i><i></i><i></i></div>
    </div>

    <div id="expanded" class="layer">
      <section id="x-media">
        <div class="row-top">
          <img id="x-art" class="art" alt="" />
          <div class="meta">
            <div id="x-title" class="title"></div>
            <div id="x-artist" class="artist"></div>
          </div>
          <div class="viz big"><i></i><i></i><i></i><i></i><i></i></div>
        </div>
        <div class="progress">
          <span id="x-pos">0:00</span>
          <div class="track"><div id="x-fill" class="fill"></div></div>
          <span id="x-rem">-0:00</span>
        </div>
        <div class="controls">
          <button data-action="prev" aria-label="Anterior">
            <svg viewBox="0 0 28 20"><path d="M13 4.2v11.6L4 10z M25 4.2v11.6L16 10z" /></svg>
          </button>
          <button data-action="play_pause" aria-label="Play/Pauză" id="x-play"></button>
          <button data-action="next" aria-label="Următor">
            <svg viewBox="0 0 28 20"><path d="M3 4.2v11.6L12 10z M15 4.2v11.6L24 10z" /></svg>
          </button>
        </div>
      </section>
      <section id="x-clock">
        <div id="x-time" class="clock-time"></div>
        <div id="x-date" class="clock-date"></div>
      </section>
      <div id="x-apps" class="apps"></div>
    </div>
  </div>
  <script type="module" src="island.js"></script>
</body>
</html>
```

- [ ] **Step 2: Scrie `src/styles.css`**

```css
:root {
  --bg: #000;
  --fg: #fff;
  --fg2: #8e8e93;
  --accent: rgb(255, 255, 255);
  --font: -apple-system, "SF Pro Display", "Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif;
}
* { box-sizing: border-box; margin: 0; padding: 0; }
html, body {
  width: 520px; height: 260px; overflow: hidden; background: transparent;
  font-family: var(--font); color: var(--fg); user-select: none; cursor: default;
  -webkit-font-smoothing: antialiased;
}
#stage { position: relative; width: 520px; height: 260px; }
#shape { position: absolute; inset: 0; overflow: visible; transition: filter .35s ease; }
#shape path { fill: var(--bg); }
body.expanded #shape { filter: drop-shadow(0 10px 22px rgba(0, 0, 0, .35)); }

.layer { position: absolute; top: 0; left: 50%; transform-origin: 50% 0; }

/* ---------- compact ---------- */
#compact {
  height: 32px; display: flex; align-items: center; justify-content: space-between;
  padding: 0 11px; transform: translateX(-50%);
  transition: opacity .25s ease, filter .25s ease;
}
#compact .art { width: 20px; height: 20px; border-radius: 5px; object-fit: cover; }
body:not(.has-media) #compact, body.expanded #compact { opacity: 0; filter: blur(6px); pointer-events: none; }

/* ---------- extins ---------- */
#expanded {
  width: 420px; padding: 14px 22px 14px;
  transform: translateX(-50%) scale(.9); opacity: 0; filter: blur(10px); pointer-events: none;
  transition: opacity .16s ease, filter .16s ease, transform .2s ease;
}
body:not(.has-media) #expanded { width: 360px; }
body.expanded #expanded {
  transform: translateX(-50%) scale(1); opacity: 1; filter: blur(0); pointer-events: auto;
  transition: opacity .3s ease .07s, filter .3s ease .07s, transform .5s cubic-bezier(.2, 1.25, .4, 1) .04s;
}

.row-top { display: flex; align-items: center; gap: 12px; }
#x-art { width: 56px; height: 56px; border-radius: 12px; object-fit: cover; background: #1c1c1e; }
.meta { flex: 1; min-width: 0; }
.title { font-size: 15px; font-weight: 600; letter-spacing: -.2px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.artist { font-size: 13px; color: var(--fg2); margin-top: 2px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.title.slide, .artist.slide { animation: slide .4s cubic-bezier(.2, 1, .4, 1); }
@keyframes slide { from { opacity: 0; transform: translateY(6px); } to { opacity: 1; transform: none; } }

.progress {
  display: flex; align-items: center; gap: 8px; margin-top: 12px;
  font-size: 11px; color: var(--fg2); font-variant-numeric: tabular-nums;
}
.progress span { width: 38px; }
.progress span:last-child { text-align: right; }
.progress .track { flex: 1; height: 5px; border-radius: 3px; background: rgba(255, 255, 255, .18); overflow: hidden; }
.progress .fill { height: 100%; width: 0; border-radius: 3px; background: rgba(255, 255, 255, .92); }
body.no-timeline .progress { visibility: hidden; }

.controls { display: flex; justify-content: center; gap: 30px; margin-top: 6px; }
.controls button {
  width: 44px; height: 34px; border: 0; border-radius: 10px; background: none; color: var(--fg);
  display: grid; place-items: center; transition: transform .18s cubic-bezier(.2, 1.4, .4, 1), background .15s;
}
.controls button:hover { background: rgba(255, 255, 255, .08); }
.controls button:active { transform: scale(.82); }
.controls svg { height: 17px; fill: currentColor; stroke: currentColor; stroke-width: 1.6; stroke-linejoin: round; }
#x-play svg { height: 22px; stroke-width: 0; }

#x-clock { display: none; text-align: center; padding-top: 2px; }
body:not(.has-media) #x-clock { display: block; }
body:not(.has-media) #x-media { display: none; }
.clock-time { font-size: 30px; font-weight: 600; letter-spacing: -.6px; font-variant-numeric: tabular-nums; }
.clock-date { font-size: 12px; color: var(--fg2); margin-top: 2px; }

.apps {
  display: flex; justify-content: center; align-items: center; gap: 8px;
  margin-top: 10px; padding-top: 10px; border-top: 1px solid rgba(255, 255, 255, .08); min-height: 41px;
}
.apps button {
  width: 31px; height: 31px; border: 0; border-radius: 8px; background: none; color: var(--fg);
  display: grid; place-items: center; font-size: 13px; font-weight: 600;
  transition: transform .22s cubic-bezier(.2, 1.5, .4, 1), background .15s;
}
.apps button:hover { transform: scale(1.18); background: rgba(255, 255, 255, .08); }
.apps button:active { transform: scale(.9); }
.apps img { width: 26px; height: 26px; }
.apps .more { font-size: 12px; color: var(--fg2); }

/* ---------- vizualizator ---------- */
.viz { display: flex; align-items: center; gap: 2px; height: 14px; }
.viz.big { height: 22px; gap: 3px; }
.viz i {
  display: block; width: 3px; height: 100%; border-radius: 2px; background: var(--accent);
  transform: scaleY(.2); animation: bar .8s ease-in-out infinite alternate;
  transition: background .6s ease;
}
.viz i:nth-child(1) { animation-duration: .74s; }
.viz i:nth-child(2) { animation-duration: .52s; animation-delay: -.2s; }
.viz i:nth-child(3) { animation-duration: .87s; animation-delay: -.4s; }
.viz i:nth-child(4) { animation-duration: .61s; animation-delay: -.1s; }
.viz i:nth-child(5) { animation-duration: .95s; animation-delay: -.3s; }
@keyframes bar { 0% { transform: scaleY(.2); } 50% { transform: scaleY(1); } 100% { transform: scaleY(.4); } }
body:not(.playing) .viz i { animation: none; transform: scaleY(.2); }

.art.swap { animation: swap .5s cubic-bezier(.2, 1.2, .4, 1); }
@keyframes swap { from { opacity: .15; transform: scale(.85); filter: blur(4px); } to { opacity: 1; transform: none; filter: none; } }
```

- [ ] **Step 3: Scrie `src/media-view.js`**

```js
import { formatTime, currentPosition, dominantColor } from './lib/util.js';

const PLAY = '<svg viewBox="0 0 20 20"><path d="M5.5 3.3v13.4c0 .8.9 1.3 1.6.9l10.6-6.7c.6-.4.6-1.3 0-1.7L7.1 2.4c-.7-.4-1.6.1-1.6.9z"/></svg>';
const PAUSE = '<svg viewBox="0 0 20 20"><rect x="4" y="3" width="4.4" height="14" rx="1.4"/><rect x="11.6" y="3" width="4.4" height="14" rx="1.4"/></svg>';
const PLACEHOLDER = 'data:image/svg+xml,' + encodeURIComponent(
  '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 56 56"><rect width="56" height="56" fill="#2c2c2e"/>' +
  '<path d="M23 38a5 5 0 1 1-2-4V18l14-3v18a5 5 0 1 1-2-4V20l-10 2z" fill="#8e8e93"/></svg>');

const $ = (id) => document.getElementById(id);

export class MediaView {
  constructor({ invoke, onChange }) {
    this.invoke = invoke;
    this.onChange = onChange;
    this.info = null;
    this.receivedAt = 0;
    this.timer = 0;
    this.playBtn = $('x-play');
    this.playBtn.innerHTML = PLAY;
    document.querySelectorAll('.controls button').forEach((btn) => {
      btn.addEventListener('click', (e) => {
        e.stopPropagation();
        const action = btn.dataset.action;
        if (action === 'play_pause' && this.info) {
          // feedback imediat; Rust confirmă în ≤ 500 ms
          this.update({ ...this.info, position: currentPosition(this.info, this.receivedAt, performance.now()), playing: !this.info.playing });
        }
        this.invoke('media_control', { action }).catch(() => {});
      });
    });
  }

  hasMedia() {
    return !!this.info;
  }

  update(info) {
    const prev = this.info;
    this.info = info;
    this.receivedAt = performance.now();
    document.body.classList.toggle('has-media', !!info);
    document.body.classList.toggle('playing', !!info?.playing);
    if (info) {
      const trackChanged = !prev || prev.title !== info.title || prev.artist !== info.artist;
      if (trackChanged) {
        this.setText($('x-title'), info.title || 'Necunoscut');
        this.setText($('x-artist'), info.artist || info.source || '');
      }
      if (!prev || prev.thumbnail !== info.thumbnail) this.setArt(info.thumbnail || PLACEHOLDER);
      this.playBtn.innerHTML = info.playing ? PAUSE : PLAY;
      document.body.classList.toggle('no-timeline', !(info.duration > 0));
      this.tick();
    }
    if (!!prev !== !!info) this.onChange();
  }

  setText(el, text) {
    el.textContent = text;
    el.classList.remove('slide');
    void el.offsetWidth;
    el.classList.add('slide');
  }

  setArt(src) {
    for (const img of [$('c-art'), $('x-art')]) {
      img.src = src;
      img.classList.remove('swap');
      void img.offsetWidth;
      img.classList.add('swap');
    }
    const probe = new Image();
    probe.onload = () => {
      const c = document.createElement('canvas');
      c.width = c.height = 12;
      const ctx = c.getContext('2d', { willReadFrequently: true });
      ctx.drawImage(probe, 0, 0, 12, 12);
      document.documentElement.style.setProperty('--accent', dominantColor(ctx.getImageData(0, 0, 12, 12).data));
    };
    probe.src = src;
  }

  setExpanded(on) {
    clearInterval(this.timer);
    if (on) {
      this.tick();
      this.timer = setInterval(() => this.tick(), 250);
    }
  }

  tick() {
    const info = this.info;
    if (!info || !(info.duration > 0)) return;
    const pos = currentPosition(info, this.receivedAt, performance.now());
    $('x-pos').textContent = formatTime(pos);
    $('x-rem').textContent = '-' + formatTime(info.duration - pos);
    $('x-fill').style.width = `${(pos / info.duration) * 100}%`;
  }
}
```

- [ ] **Step 4: Scrie `src/apps-view.js`**

```js
const MAX_ICONS = 9;

export class AppsView {
  constructor({ invoke, onPicked }) {
    this.invoke = invoke;
    this.onPicked = onPicked;
    this.el = document.getElementById('x-apps');
    this.timer = 0;
    this.key = '';
  }

  setActive(on) {
    clearInterval(this.timer);
    if (on) {
      this.refresh();
      this.timer = setInterval(() => this.refresh(), 2000);
    }
  }

  async refresh() {
    let list;
    try {
      list = await this.invoke('list_windows');
    } catch {
      return;
    }
    if (!Array.isArray(list)) return;
    const key = list.map((w) => `${w.hwnd}:${w.title}`).join('|');
    if (key === this.key) return;
    this.key = key;
    this.render(list);
  }

  render(list) {
    this.el.replaceChildren();
    for (const w of list.slice(0, MAX_ICONS)) {
      const btn = document.createElement('button');
      btn.title = w.title;
      if (w.icon) {
        const img = document.createElement('img');
        img.src = w.icon;
        img.alt = '';
        btn.append(img);
      } else {
        btn.textContent = (w.exe || w.title || '?').charAt(0).toUpperCase();
      }
      btn.addEventListener('click', (e) => {
        e.stopPropagation();
        this.invoke('focus_window', { hwnd: w.hwnd }).catch(() => { this.key = ''; this.refresh(); });
        this.onPicked();
      });
      this.el.append(btn);
    }
    if (list.length > MAX_ICONS) {
      const more = document.createElement('span');
      more.className = 'more';
      more.textContent = `+${list.length - MAX_ICONS}`;
      this.el.append(more);
    }
  }
}
```

- [ ] **Step 5: Scrie `src/island.js`**

```js
import { Spring } from './lib/spring.js';
import { islandPath, islandRect } from './lib/shape.js';
import { MediaView } from './media-view.js';
import { AppsView } from './apps-view.js';

const W = 520;
const EAR = 7;
const SIZES = {
  idle: { w: 190, h: 32, r: 10 },
  active: { w: 300, h: 32, r: 10 },
  expandedMedia: { w: 420, h: 206, r: 36 },
  expandedIdle: { w: 360, h: 140, r: 32 },
};
const SPRING = { response: 0.5, damping: 0.72 };

const tauri = window.__TAURI__;
const demo = !tauri && new URLSearchParams(location.search).has('demo');
const invoke = (cmd, args) => (tauri ? tauri.core.invoke(cmd, args) : demoInvoke(cmd, args));

const pathEl = document.getElementById('shape-path');
const compactEl = document.getElementById('compact');

// pornește de la lățime 0, deci insula „crește” la pornire
const springs = { w: new Spring(0, SPRING), h: new Spring(32, SPRING), r: new Spring(10, SPRING) };
let expanded = false;
let raf = 0;
let lastFrame = 0;
let rect = { x: 0, y: 0, w: 0, h: 0 };
let lastRectKey = '';

const media = new MediaView({ invoke, onChange: retarget });
const apps = new AppsView({ invoke, onPicked: () => setExpanded(false) });

function targetSize() {
  if (expanded) return media.hasMedia() ? SIZES.expandedMedia : SIZES.expandedIdle;
  return media.hasMedia() ? SIZES.active : SIZES.idle;
}

function retarget() {
  const t = targetSize();
  springs.w.setTarget(t.w);
  springs.h.setTarget(t.h);
  springs.r.setTarget(t.r);
  document.body.classList.toggle('expanded', expanded);
  if (!raf) {
    lastFrame = performance.now();
    raf = requestAnimationFrame(frame);
  }
}

function frame(now) {
  const dt = Math.min(0.05, (now - lastFrame) / 1000);
  lastFrame = now;
  let settled = true;
  for (const s of Object.values(springs)) settled = s.step(dt) && settled;
  if (settled) for (const s of Object.values(springs)) s.snap();
  render();
  raf = settled ? 0 : requestAnimationFrame(frame);
}

function render() {
  const w = springs.w.value, h = springs.h.value, r = springs.r.value;
  pathEl.setAttribute('d', islandPath(W, w, h, r, EAR));
  compactEl.style.width = `${Math.max(0, w)}px`;
  rect = islandRect(W, Math.max(0, w), Math.max(0, h), EAR);
  const key = [rect.x, rect.w, rect.h].map(Math.round).join(',');
  if (key !== lastRectKey) {
    lastRectKey = key;
    invoke('set_island_rect', { rect }).catch(() => {});
  }
}

function setExpanded(on) {
  if (expanded === on) return;
  expanded = on;
  media.setExpanded(on);
  apps.setActive(on);
  updateClock();
  retarget();
}

// hover: extindere după 120 ms, strângere după 220 ms (ca să nu „pâlpâie”)
let hoverTimer = 0;
function onHover(inside) {
  clearTimeout(hoverTimer);
  hoverTimer = setTimeout(() => setExpanded(inside), inside ? 120 : 220);
}

document.getElementById('stage').addEventListener('click', () => {
  if (!expanded) {
    clearTimeout(hoverTimer);
    setExpanded(true);
  }
});

function updateClock() {
  const now = new Date();
  document.getElementById('x-time').textContent = now.toLocaleTimeString('ro-RO', { hour: '2-digit', minute: '2-digit' });
  const date = now.toLocaleDateString('ro-RO', { weekday: 'long', day: 'numeric', month: 'long' });
  document.getElementById('x-date').textContent = date.charAt(0).toUpperCase() + date.slice(1);
}
setInterval(() => expanded && updateClock(), 1000);

if (tauri) {
  tauri.event.listen('island-hover', (e) => onHover(!!e.payload));
  tauri.event.listen('media-update', (e) => media.update(e.payload ?? null));
} else {
  // în browser: hover calculat local, cu același dreptunghi ca în Rust
  let inside = false;
  document.addEventListener('pointermove', (e) => {
    const now = e.clientX >= rect.x && e.clientX <= rect.x + rect.w && e.clientY >= rect.y && e.clientY <= rect.y + rect.h;
    if (now !== inside) { inside = now; onHover(now); }
  });
  document.addEventListener('pointerleave', () => { inside = false; onHover(false); });
  if (demo) startDemo();
}

retarget();

// ---------------- mod demo (doar în browser, pentru verificat designul) ----------------
function demoArt(hue) {
  const c = document.createElement('canvas');
  c.width = c.height = 120;
  const g = c.getContext('2d');
  const grad = g.createLinearGradient(0, 0, 120, 120);
  grad.addColorStop(0, `hsl(${hue} 80% 55%)`);
  grad.addColorStop(1, `hsl(${(hue + 60) % 360} 70% 25%)`);
  g.fillStyle = grad;
  g.fillRect(0, 0, 120, 120);
  return c.toDataURL();
}
const demoTracks = [
  { title: 'Blinding Lights', artist: 'The Weeknd', hue: 350, duration: 200 },
  { title: 'Midnight City', artist: 'M83', hue: 260, duration: 243 },
];
let demoIdx = 0;
let demoInfo = null;
function demoEmit(patch) {
  demoInfo = { ...demoInfo, ...patch };
  media.update(demoInfo);
}
function startDemo() {
  setTimeout(() => {
    const t = demoTracks[0];
    demoInfo = { title: t.title, artist: t.artist, album: '', thumbnail: demoArt(t.hue), playing: true, position: 37, duration: t.duration, source: 'Spotify' };
    media.update(demoInfo);
  }, 900);
}
async function demoInvoke(cmd, args) {
  if (!demo) return null;
  if (cmd === 'media_control' && demoInfo) {
    if (args.action === 'play_pause') return null; // starea e deja comutată optimist
    demoIdx = (demoIdx + (args.action === 'next' ? 1 : demoTracks.length - 1)) % demoTracks.length;
    const t = demoTracks[demoIdx];
    demoEmit({ title: t.title, artist: t.artist, thumbnail: demoArt(t.hue), position: 0, duration: t.duration, playing: true });
  }
  if (cmd === 'list_windows') {
    return ['Chrome', 'Spotify', 'Code', 'Explorer', 'Discord'].map((n, i) => ({ hwnd: i + 1, title: n, exe: n.toLowerCase() + '.exe', icon: null }));
  }
  return null;
}
```

- [ ] **Step 6: Verifică designul în browser (mod demo)**

Run: `npx --yes http-server src -p 5179 -c-1` (în fundal), apoi deschide `http://localhost:5179/?demo` în Chrome.
Expected:
- la pornire insula crește din nimic până la notch;
- după ~1s se lățește, cu coperta în stânga și barele colorate în dreapta;
- la hover se extinde elastic (ușor overshoot), iar conținutul apare cu blur și fade;
- ⏯ comută play/pauză și barele se opresc; ⏭/⏮ schimbă piesa, cu animație pe copertă și pe text;
- la ieșirea mouse-ului se strânge înapoi.

Fă un screenshot al stării extinse și compară-l cu notch-ul MacBook (formă, urechi, colțuri, tipografie). Ajustează CSS-ul dacă ceva arată diferit.

- [ ] **Step 7: Rulează testele JS (nu trebuie să se fi stricat nimic)**

Run: `npm test`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src
git commit -m "feat: interfata insulei cu stari, spring si mod demo"
```

---

### Task 4: Click-through, hover și ascundere la fullscreen

**Files:**
- Create: `src-tauri/src/hittest.rs`
- Modify: `src-tauri/src/platform.rs` (adaugă `cursor_pos`, `covers_monitor`, `foreground_is_fullscreen`), `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: `platform::hwnd_of`, `platform::show_no_activate`, `platform::hide` (Task 1); `set_island_rect` și `island-hover` din contractul Task 3
- Produces: `hittest::Rect { x, y, w, h: f64 }`, `hittest::IslandRect` (managed state), comanda `hittest::set_island_rect(rect: Rect)`, `hittest::spawn(AppHandle)`, `hittest::point_in_rect(px, py, &Rect, margin) -> bool`, `platform::covers_monitor((l,t,r,b), (l,t,r,b)) -> bool`

- [ ] **Step 1: Scrie testele (la finalul `hittest.rs` și `platform.rs`)**

În `src-tauri/src/hittest.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punct_in_dreptunghi() {
        let r = Rect { x: 10.0, y: 0.0, w: 100.0, h: 32.0 };
        assert!(point_in_rect(10.0, 0.0, &r, 0.0));
        assert!(point_in_rect(110.0, 32.0, &r, 0.0));
        assert!(!point_in_rect(111.0, 10.0, &r, 0.0));
        assert!(point_in_rect(111.0, 10.0, &r, 1.0));
        assert!(!point_in_rect(50.0, 40.0, &r, 1.0));
    }
}
```

La finalul `src-tauri/src/platform.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_detectat() {
        let mon = (0, 0, 1920, 1080);
        assert!(covers_monitor((0, 0, 1920, 1080), mon));
        assert!(covers_monitor((-8, -8, 1928, 1088), mon)); // ferestre cu margini în afara ecranului
        assert!(!covers_monitor((0, 0, 1920, 1040), mon)); // maximizat, cu taskbar vizibil
        assert!(!covers_monitor((100, 100, 800, 600), mon));
    }
}
```

- [ ] **Step 2: Rulează testele, trebuie să pice**

Run: `cd src-tauri; cargo test`
Expected: FAIL la compilare (`point_in_rect`, `covers_monitor` nu există).

- [ ] **Step 3: Adaugă în `platform.rs`**

Importuri suplimentare (în capul fișierului):
```rust
use windows::Win32::Foundation::{POINT, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL};
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetCursorPos, GetForegroundWindow, GetWindowRect, MONITORINFOF_PRIMARY};
```

Funcții:
```rust
pub fn cursor_pos() -> Option<(i32, i32)> {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p).ok()? };
    Some((p.x, p.y))
}

/// (left, top, right, bottom): fereastra acoperă tot monitorul.
pub fn covers_monitor(win: (i32, i32, i32, i32), mon: (i32, i32, i32, i32)) -> bool {
    win.0 <= mon.0 && win.1 <= mon.1 && win.2 >= mon.2 && win.3 >= mon.3
}

/// Aplicația din prim-plan rulează pe tot ecranul principal (video, joc)?
pub fn foreground_is_fullscreen(own: Option<HWND>) -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() || Some(fg) == own {
            return false;
        }
        let mut cls = [0u16; 64];
        let n = GetClassNameW(fg, &mut cls).max(0) as usize;
        let class = String::from_utf16_lossy(&cls[..n]);
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
            return false;
        }
        let mut wr = RECT::default();
        if GetWindowRect(fg, &mut wr).is_err() {
            return false;
        }
        let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONULL);
        if mon.is_invalid() {
            return false;
        }
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() || mi.dwFlags & MONITORINFOF_PRIMARY == 0 {
            return false;
        }
        let m = mi.rcMonitor;
        covers_monitor((wr.left, wr.top, wr.right, wr.bottom), (m.left, m.top, m.right, m.bottom))
    }
}
```

- [ ] **Step 4: Creează `src-tauri/src/hittest.rs`**

```rust
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::platform;

/// Dreptunghiul insulei, în CSS px relativ la fereastră (raportat de JS la fiecare cadru de animație).
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Default)]
pub struct IslandRect(pub Arc<Mutex<Rect>>);

pub fn point_in_rect(px: f64, py: f64, r: &Rect, margin: f64) -> bool {
    px >= r.x - margin && px <= r.x + r.w + margin && py >= r.y - margin && py <= r.y + r.h + margin
}

#[tauri::command]
pub fn set_island_rect(state: tauri::State<IslandRect>, rect: Rect) {
    *state.0.lock().unwrap() = rect;
}

/// Thread care: (1) face fereastra transparentă la click în afara insulei,
/// (2) trimite `island-hover` către JS, (3) ascunde insula când o aplicație e pe tot ecranul.
pub fn spawn(app: AppHandle) {
    let shared = app.state::<IslandRect>().0.clone();
    std::thread::spawn(move || {
        let Some(win) = app.get_webview_window("island") else { return };
        let own = platform::hwnd_of(&win);
        let mut inside_last: Option<bool> = None;
        let mut hidden = false;
        let mut geo = (0i32, 0i32, 1.0f64);
        let mut geo_at: Option<Instant> = None;
        let mut fs_at = Instant::now();

        loop {
            std::thread::sleep(Duration::from_millis(30));

            if geo_at.map_or(true, |t| t.elapsed() > Duration::from_secs(1)) {
                if let (Ok(p), Ok(s)) = (win.outer_position(), win.scale_factor()) {
                    geo = (p.x, p.y, s);
                }
                geo_at = Some(Instant::now());
            }

            if fs_at.elapsed() > Duration::from_millis(500) {
                fs_at = Instant::now();
                let fs = platform::foreground_is_fullscreen(own);
                if fs != hidden {
                    hidden = fs;
                    if fs { platform::hide(&win) } else { platform::show_no_activate(&win) }
                }
            }
            if hidden {
                continue;
            }

            let Some((cx, cy)) = platform::cursor_pos() else { continue };
            let rect = *shared.lock().unwrap();
            let px = (cx - geo.0) as f64 / geo.2;
            let py = (cy - geo.1) as f64 / geo.2;
            let inside = point_in_rect(px, py, &rect, 1.0);
            if inside_last != Some(inside) {
                inside_last = Some(inside);
                let _ = win.set_ignore_cursor_events(!inside);
                let _ = app.emit("island-hover", inside);
            }
        }
    });
}
```

- [ ] **Step 5: Leagă în `main.rs`**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hittest;
mod platform;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .manage(hittest::IslandRect::default())
        .invoke_handler(tauri::generate_handler![hittest::set_island_rect])
        .setup(|app| {
            let win = app.get_webview_window("island").expect("fereastra island lipsește");
            platform::place_top_center(&win)?;
            platform::make_no_activate(&win);
            platform::show_no_activate(&win);
            hittest::spawn(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("eroare la pornirea aplicației");
}
```

- [ ] **Step 6: Rulează testele, trebuie să treacă**

Run: `cd src-tauri; cargo test`
Expected: PASS (`punct_in_dreptunghi`, `fullscreen_detectat`).

- [ ] **Step 7: Verificare manuală**

Run: `npm run tauri dev`
Expected:
- click pe desktop/ferestre chiar lângă insulă trece prin ea;
- hover pe insulă o extinde (ceas + rând gol de aplicații), iar ieșirea o strânge;
- un video YouTube pe tot ecranul (F) ascunde insula, iar Esc o readuce.

- [ ] **Step 8: Commit**

```bash
git add src-tauri
git commit -m "feat: click-through, hover si ascundere la fullscreen"
```

---

### Task 5: Media (GSMTC) — ce cântă + control

**Files:**
- Create: `src-tauri/src/media.rs`
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: evenimentul `media-update` și comanda `media_control` din contractul Task 3
- Produces: `media::MediaInfo` (serializat ca în contract), `media::spawn(AppHandle)`, comanda `media::media_control(action: String) -> Result<(), String>`; funcții pure `live_position`, `friendly_source`, `should_emit`, `choose_session_index`, `MediaAction::parse`, `ThumbCache::get`

- [ ] **Step 1: Scrie testele (la finalul `media.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn info(title: &str, playing: bool, position: f64) -> MediaInfo {
        MediaInfo { title: title.into(), artist: "A".into(), playing, position, duration: 200.0, source: "Spotify".into(), ..Default::default() }
    }

    #[test]
    fn pozitie_extrapolata_doar_cand_ruleaza() {
        let s = 10_000_000i64;
        assert_eq!(live_position(10 * s, 100 * s, 103 * s, true, 0), 13.0);
        assert_eq!(live_position(10 * s, 100 * s, 103 * s, false, 0), 10.0);
        assert_eq!(live_position(10 * s, 100 * s, 500 * s, true, 20 * s), 20.0);
        assert_eq!(live_position(10 * s, 0, 500 * s, true, 0), 10.0);
    }

    #[test]
    fn nume_prietenoase() {
        assert_eq!(friendly_source("Spotify.exe"), "Spotify");
        assert_eq!(friendly_source("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"), "Spotify");
        assert_eq!(friendly_source("Chrome"), "Chrome");
        assert_eq!(friendly_source("MSEdge"), "Edge");
        assert_eq!(friendly_source("308046B0AF4A39CB"), "Firefox");
        assert_eq!(friendly_source("C:\\Apps\\foobar2000.exe"), "foobar2000");
    }

    #[test]
    fn emite_doar_la_schimbari() {
        let a = Some(info("X", true, 10.0));
        assert!(!should_emit(&None, &None, 1.0));
        assert!(should_emit(&None, &a, 0.0));
        assert!(should_emit(&a, &None, 0.0));
        assert!(!should_emit(&a, &Some(info("X", true, 10.5)), 0.5)); // avans normal
        assert!(should_emit(&a, &Some(info("X", true, 60.0)), 0.5)); // seek
        assert!(should_emit(&a, &Some(info("X", false, 10.0)), 0.0)); // pauză
        assert!(should_emit(&a, &Some(info("Y", true, 0.0)), 0.5)); // piesă nouă
    }

    #[test]
    fn prefera_sesiunea_care_ruleaza() {
        assert_eq!(choose_session_index(&[false, true, false], Some(0)), Some(1));
        assert_eq!(choose_session_index(&[false, false], Some(1)), Some(1));
        assert_eq!(choose_session_index(&[false, false], None), Some(0));
        assert_eq!(choose_session_index(&[], None), None);
    }

    #[test]
    fn actiuni_media() {
        assert_eq!(MediaAction::parse("play_pause"), Some(MediaAction::PlayPause));
        assert_eq!(MediaAction::parse("next"), Some(MediaAction::Next));
        assert_eq!(MediaAction::parse("prev"), Some(MediaAction::Prev));
        assert_eq!(MediaAction::parse("volum"), None);
    }

    #[test]
    fn coperta_reincercata_pana_apare() {
        let mut c = ThumbCache::with_retry(Duration::ZERO);
        assert_eq!(c.get("k1", || None), None);
        assert_eq!(c.get("k1", || Some("img".into())), Some("img".into()));
        // cât timp piesa e aceeași, nu se mai citește
        assert_eq!(c.get("k1", || panic!("nu trebuia citită")), Some("img".into()));
        // piesă nouă → citire nouă
        assert_eq!(c.get("k2", || Some("img2".into())), Some("img2".into()));
    }
}
```

- [ ] **Step 2: Rulează testele, trebuie să pice**

Run: `cd src-tauri; cargo test media`
Expected: FAIL la compilare.

- [ ] **Step 3: Implementează `src-tauri/src/media.rs`**

```rust
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use tauri::{AppHandle, Emitter};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionMediaProperties as Props,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::Foundation::E_FAIL;

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub thumbnail: Option<String>,
    pub playing: bool,
    pub position: f64,
    pub duration: f64,
    pub source: String,
}

const TICKS_PER_SEC: f64 = 10_000_000.0;
const UNIX_TO_FILETIME_SECS: i64 = 11_644_473_600;

fn now_universal_ticks() -> i64 {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    (d.as_secs() as i64 + UNIX_TO_FILETIME_SECS) * 10_000_000 + (d.subsec_nanos() / 100) as i64
}

/// Poziția în secunde acum, pornind de la poziția raportată la `last_updated`.
pub fn live_position(position: i64, last_updated: i64, now: i64, playing: bool, end: i64) -> f64 {
    let mut pos = position as f64 / TICKS_PER_SEC;
    if playing && last_updated > 0 && now > last_updated {
        pos += (now - last_updated) as f64 / TICKS_PER_SEC;
    }
    if end > 0 {
        pos = pos.min(end as f64 / TICKS_PER_SEC);
    }
    pos.max(0.0)
}

pub fn friendly_source(aumid: &str) -> String {
    let l = aumid.to_lowercase();
    let known = [
        ("spotify", "Spotify"), ("chrome", "Chrome"), ("msedge", "Edge"), ("firefox", "Firefox"),
        ("308046b0af4a39cb", "Firefox"), ("opera", "Opera"), ("brave", "Brave"), ("vlc", "VLC"),
        ("zunemusic", "Media Player"), ("applemusic", "Apple Music"),
    ];
    for (k, name) in known {
        if l.contains(k) {
            return name.to_string();
        }
    }
    let base = aumid.rsplit(['\\', '!']).next().unwrap_or(aumid);
    base.strip_suffix(".exe").or_else(|| base.strip_suffix(".EXE")).unwrap_or(base).to_string()
}

pub fn should_emit(prev: &Option<MediaInfo>, cur: &Option<MediaInfo>, secs_since_emit: f64) -> bool {
    match (prev, cur) {
        (None, None) => false,
        (Some(_), None) | (None, Some(_)) => true,
        (Some(p), Some(c)) => {
            if p.title != c.title || p.artist != c.artist || p.album != c.album || p.playing != c.playing
                || p.source != c.source || p.thumbnail != c.thumbnail || (p.duration - c.duration).abs() > 0.5
            {
                return true;
            }
            let expected = p.position + if p.playing { secs_since_emit } else { 0.0 };
            (c.position - expected).abs() > 1.5
        }
    }
}

/// Alege sesiunea care rulează; altfel sesiunea „curentă” a Windows; altfel prima.
pub fn choose_session_index(playing: &[bool], current: Option<usize>) -> Option<usize> {
    playing.iter().position(|p| *p).or(current).or(if playing.is_empty() { None } else { Some(0) })
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MediaAction {
    PlayPause,
    Next,
    Prev,
}

impl MediaAction {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "play_pause" => Some(Self::PlayPause),
            "next" => Some(Self::Next),
            "prev" => Some(Self::Prev),
            _ => None,
        }
    }
}

/// Coperta e citită o dată pe piesă; dacă lipsește (Spotify o trimite cu întârziere), se reîncearcă.
pub struct ThumbCache {
    key: String,
    value: Option<String>,
    tries: u32,
    last_try: Option<Instant>,
    retry_every: Duration,
}

impl Default for ThumbCache {
    fn default() -> Self {
        Self::with_retry(Duration::from_millis(1500))
    }
}

impl ThumbCache {
    pub fn with_retry(retry_every: Duration) -> Self {
        Self { key: String::new(), value: None, tries: 0, last_try: None, retry_every }
    }

    pub fn get(&mut self, key: &str, load: impl FnOnce() -> Option<String>) -> Option<String> {
        if self.key != key {
            self.key = key.to_string();
            self.value = None;
            self.tries = 0;
            self.last_try = None;
        }
        let due = self.last_try.map_or(true, |t| t.elapsed() >= self.retry_every);
        if self.value.is_none() && self.tries < 6 && due {
            self.tries += 1;
            self.last_try = Some(Instant::now());
            self.value = load();
        }
        self.value.clone()
    }
}

fn pick_session(m: &Manager) -> Option<Session> {
    let list: Vec<Session> = m.GetSessions().map(|v| v.into_iter().collect()).unwrap_or_default();
    let playing: Vec<bool> = list
        .iter()
        .map(|s| s.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).map(|st| st == Status::Playing).unwrap_or(false))
        .collect();
    let current_id = m.GetCurrentSession().and_then(|s| s.SourceAppUserModelId()).ok();
    let current = current_id.and_then(|id| list.iter().position(|s| s.SourceAppUserModelId().ok().as_ref() == Some(&id)));
    choose_session_index(&playing, current).and_then(|i| list.get(i).cloned())
}

fn read_thumbnail(props: &Props) -> Option<String> {
    let read = || -> windows::core::Result<String> {
        let stream = props.Thumbnail()?.OpenReadAsync()?.get()?;
        let size = stream.Size()? as u32;
        if size == 0 || size > 8 * 1024 * 1024 {
            return Err(E_FAIL.into());
        }
        let reader = DataReader::CreateDataReader(&stream)?;
        reader.LoadAsync(size)?.get()?;
        let mut buf = vec![0u8; size as usize];
        reader.ReadBytes(&mut buf)?;
        let ct = stream.ContentType().map(|s| s.to_string()).unwrap_or_default();
        let ct = if ct.starts_with("image/") { ct } else { "image/png".to_string() };
        Ok(format!("data:{ct};base64,{}", STANDARD.encode(&buf)))
    };
    read().ok()
}

fn read_session(session: &Session, cache: &mut ThumbCache) -> windows::core::Result<MediaInfo> {
    let props = session.TryGetMediaPropertiesAsync()?.get()?;
    let title = props.Title()?.to_string();
    let artist = props.Artist().map(|s| s.to_string()).unwrap_or_default();
    let album = props.AlbumTitle().map(|s| s.to_string()).unwrap_or_default();
    let playing = session.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing;
    let (position, duration) = match session.GetTimelineProperties() {
        Ok(tl) => {
            let pos = tl.Position().map(|t| t.Duration).unwrap_or(0);
            let end = tl.EndTime().map(|t| t.Duration).unwrap_or(0);
            let upd = tl.LastUpdatedTime().map(|t| t.UniversalTime).unwrap_or(0);
            (live_position(pos, upd, now_universal_ticks(), playing, end), end as f64 / TICKS_PER_SEC)
        }
        Err(_) => (0.0, 0.0),
    };
    let source = friendly_source(&session.SourceAppUserModelId()?.to_string());
    let key = format!("{source}|{title}|{artist}");
    let thumbnail = cache.get(&key, || read_thumbnail(&props));
    Ok(MediaInfo { title, artist, album, thumbnail, playing, position, duration, source })
}

/// Citește media la fiecare 500 ms și emite `media-update` doar când e nevoie.
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last: Option<MediaInfo> = None;
        let mut last_emit = Instant::now();
        let mut cache = ThumbCache::default();
        loop {
            let manager = match Manager::RequestAsync().and_then(|op| op.get()) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("GSMTC indisponibil: {e}");
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                }
            };
            // managerul e re-creat la ~5 minute, ca să nu rămână blocat după restartul serviciului media
            for _ in 0..600 {
                let cur = pick_session(&manager).and_then(|s| read_session(&s, &mut cache).ok());
                let since = last_emit.elapsed().as_secs_f64();
                if should_emit(&last, &cur, since) {
                    let _ = app.emit("media-update", cur.clone());
                    last_emit = Instant::now();
                    last = cur;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    });
}

#[tauri::command]
pub async fn media_control(action: String) -> Result<(), String> {
    let action = MediaAction::parse(&action).ok_or_else(|| format!("acțiune necunoscută: {action}"))?;
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let m = Manager::RequestAsync().and_then(|op| op.get()).map_err(|e| e.to_string())?;
        let s = pick_session(&m).ok_or("nicio sesiune media")?;
        let op = match action {
            MediaAction::PlayPause => s.TryTogglePlayPauseAsync(),
            MediaAction::Next => s.TrySkipNextAsync(),
            MediaAction::Prev => s.TrySkipPreviousAsync(),
        };
        op.and_then(|o| o.get()).map(|_| ()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
```

- [ ] **Step 4: Leagă în `main.rs`**

Adaugă `mod media;`, include `media::media_control` în `generate_handler!` și apelează `media::spawn(app.handle().clone());` după `hittest::spawn(...)`.

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hittest;
mod media;
mod platform;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .manage(hittest::IslandRect::default())
        .invoke_handler(tauri::generate_handler![hittest::set_island_rect, media::media_control])
        .setup(|app| {
            let win = app.get_webview_window("island").expect("fereastra island lipsește");
            platform::place_top_center(&win)?;
            platform::make_no_activate(&win);
            platform::show_no_activate(&win);
            hittest::spawn(app.handle().clone());
            media::spawn(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("eroare la pornirea aplicației");
}
```

- [ ] **Step 5: Rulează testele, trebuie să treacă**

Run: `cd src-tauri; cargo test`
Expected: PASS (toate testele din `media`, `hittest`, `platform`).

- [ ] **Step 6: Verificare manuală**

Run: `npm run tauri dev`
Expected:
- pornește Spotify → insula se lățește cu coperta și barele în culoarea copertei;
- hover → titlu, artist, progres care avansează, iar ⏯ ⏭ ⏮ controlează Spotify;
- pauză în Spotify și pornește un video YouTube în Chrome → insula trece pe YouTube;
- oprește tot → insula revine la notch.

- [ ] **Step 7: Commit**

```bash
git add src-tauri
git commit -m "feat: media curenta prin GSMTC si control play/next/prev"
```

---

### Task 6: Aplicațiile deschise — listă, iconițe, adu în față

**Files:**
- Create: `src-tauri/src/windows_list.rs`
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: comenzile `list_windows` și `focus_window` din contractul Task 3
- Produces: `windows_list::AppWindow { hwnd: isize, title, exe, icon: Option<String> }`, comenzile `list_windows() -> Vec<AppWindow>` și `focus_window(hwnd: isize) -> Result<(), String>`; funcții pure `is_app_window(&Candidate)`, `dedupe_by_exe(Vec<AppWindow>)`, `bgra_to_rgba(&mut [u8])`, `encode_png(&[u8], u32, u32)`

- [ ] **Step 1: Scrie testele (la finalul `windows_list.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn cand(title: &str) -> Candidate {
        Candidate { visible: true, has_owner: false, title: title.into(), ex_style: 0, cloaked: false, is_self: false }
    }

    fn win(hwnd: isize, exe: &str) -> AppWindow {
        AppWindow { hwnd, title: format!("w{hwnd}"), exe: exe.into(), icon: None }
    }

    #[test]
    fn filtru_ferestre() {
        assert!(is_app_window(&cand("Document - Word")));
        assert!(!is_app_window(&cand("")));
        assert!(!is_app_window(&cand("Program Manager")));
        assert!(!is_app_window(&Candidate { visible: false, ..cand("x") }));
        assert!(!is_app_window(&Candidate { has_owner: true, ..cand("x") }));
        assert!(!is_app_window(&Candidate { cloaked: true, ..cand("x") }));
        assert!(!is_app_window(&Candidate { is_self: true, ..cand("x") }));
        assert!(!is_app_window(&Candidate { ex_style: WS_EX_TOOLWINDOW.0, ..cand("x") }));
    }

    #[test]
    fn o_iconita_pe_aplicatie_dar_uwp_separat() {
        let out = dedupe_by_exe(vec![
            win(1, "chrome.exe"), win(2, "Chrome.exe"), win(3, "code.exe"),
            win(4, "ApplicationFrameHost.exe"), win(5, "ApplicationFrameHost.exe"),
        ]);
        let ids: Vec<isize> = out.iter().map(|w| w.hwnd).collect();
        assert_eq!(ids, vec![1, 3, 4, 5]);
    }

    #[test]
    fn bgra_in_rgba() {
        let mut px = vec![1, 2, 3, 200, 10, 20, 30, 0];
        bgra_to_rgba(&mut px);
        assert_eq!(px, vec![3, 2, 1, 200, 30, 20, 10, 0]);
        let mut opaque = vec![1, 2, 3, 0];
        bgra_to_rgba(&mut opaque);
        assert_eq!(opaque, vec![3, 2, 1, 255]); // fără canal alfa → opac
    }

    #[test]
    fn png_valid() {
        let png = encode_png(&[255, 0, 0, 255], 1, 1).unwrap();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    }
}
```

- [ ] **Step 2: Rulează testele, trebuie să pice**

Run: `cd src-tauri; cargo test windows_list`
Expected: FAIL la compilare.

- [ ] **Step 3: Implementează `src-tauri/src/windows_list.rs`**

```rust
use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HDC,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Threading::{
    GetCurrentProcessId, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Input::KeyboardAndMouse::AttachThreadInput;
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, DestroyIcon, EnumWindows, GetForegroundWindow, GetIconInfo, GetWindow, GetWindowLongW,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible,
    SetForegroundWindow, ShowWindow, GWL_EXSTYLE, GW_OWNER, HICON, ICONINFO, SW_RESTORE, WS_EX_TOOLWINDOW,
};

#[derive(Serialize, Clone, Debug)]
pub struct AppWindow {
    pub hwnd: isize,
    pub title: String,
    pub exe: String,
    pub icon: Option<String>,
}

pub struct Candidate {
    pub visible: bool,
    pub has_owner: bool,
    pub title: String,
    pub ex_style: u32,
    pub cloaked: bool,
    pub is_self: bool,
}

pub fn is_app_window(c: &Candidate) -> bool {
    c.visible
        && !c.has_owner
        && !c.cloaked
        && !c.is_self
        && c.ex_style & WS_EX_TOOLWINDOW.0 == 0
        && !c.title.trim().is_empty()
        && c.title != "Program Manager"
}

/// O singură iconiță pe aplicație (prima fereastră în ordinea Z), cu excepția aplicațiilor UWP,
/// care rulează toate sub ApplicationFrameHost.exe.
pub fn dedupe_by_exe(list: Vec<AppWindow>) -> Vec<AppWindow> {
    let mut seen = HashSet::new();
    list.into_iter()
        .filter(|w| {
            let exe = w.exe.to_lowercase();
            exe == "applicationframehost.exe" || exe.is_empty() || seen.insert(exe)
        })
        .collect()
}

pub fn bgra_to_rgba(buf: &mut [u8]) {
    let has_alpha = buf.chunks_exact(4).any(|p| p[3] != 0);
    for p in buf.chunks_exact_mut(4) {
        p.swap(0, 2);
        if !has_alpha {
            p[3] = 255;
        }
    }
}

pub fn encode_png(rgba: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(out)
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let v = &mut *(lparam.0 as *mut Vec<HWND>);
    v.push(hwnd);
    BOOL(1)
}

fn top_level_windows() -> Vec<HWND> {
    let mut v: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut v as *mut Vec<HWND> as isize));
    }
    v
}

unsafe fn candidate(h: HWND, own_pid: u32) -> (Candidate, u32) {
    let has_owner = GetWindow(h, GW_OWNER).map(|o| !o.0.is_null()).unwrap_or(false);
    let len = GetWindowTextLengthW(h).max(0) as usize;
    let mut buf = vec![0u16; len + 1];
    let n = GetWindowTextW(h, &mut buf).max(0) as usize;
    let title = String::from_utf16_lossy(&buf[..n]);
    let ex_style = GetWindowLongW(h, GWL_EXSTYLE) as u32;
    let mut cloaked: u32 = 0;
    let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut _, 4);
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, Some(&mut pid));
    let c = Candidate { visible: true, has_owner, title, ex_style, cloaked: cloaked != 0, is_self: pid == own_pid };
    (c, pid)
}

unsafe fn exe_path(pid: u32) -> Option<String> {
    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
    let _ = CloseHandle(h);
    r.ok()?;
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

unsafe fn hicon_to_png(icon: HICON) -> Option<Vec<u8>> {
    let mut ii = ICONINFO::default();
    GetIconInfo(icon, &mut ii).ok()?;
    let result = (|| {
        if ii.hbmColor.is_invalid() {
            return None;
        }
        let mut bm = BITMAP::default();
        if GetObjectW(ii.hbmColor, std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut BITMAP as *mut _)) == 0 {
            return None;
        }
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let dc = CreateCompatibleDC(HDC::default());
        let lines = GetDIBits(dc, ii.hbmColor, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        let _ = DeleteDC(dc);
        if lines == 0 {
            return None;
        }
        bgra_to_rgba(&mut buf);
        encode_png(&buf, w as u32, h as u32)
    })();
    if !ii.hbmColor.is_invalid() {
        let _ = DeleteObject(ii.hbmColor);
    }
    if !ii.hbmMask.is_invalid() {
        let _ = DeleteObject(ii.hbmMask);
    }
    result
}

unsafe fn load_icon(path: &str) -> Option<String> {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut info = SHFILEINFOW::default();
    let r = SHGetFileInfoW(
        PCWSTR(wide.as_ptr()),
        FILE_FLAGS_AND_ATTRIBUTES(0),
        Some(&mut info),
        std::mem::size_of::<SHFILEINFOW>() as u32,
        SHGFI_ICON | SHGFI_LARGEICON,
    );
    if r == 0 || info.hIcon.is_invalid() {
        return None;
    }
    let png = hicon_to_png(info.hIcon);
    let _ = DestroyIcon(info.hIcon);
    png.map(|bytes| format!("data:image/png;base64,{}", STANDARD.encode(bytes)))
}

fn icon_cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

pub fn collect_windows() -> Vec<AppWindow> {
    let mut out = Vec::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let own = GetCurrentProcessId();
        for h in top_level_windows() {
            if !IsWindowVisible(h).as_bool() {
                continue;
            }
            let (c, pid) = candidate(h, own);
            if !is_app_window(&c) {
                continue;
            }
            let path = exe_path(pid).unwrap_or_default();
            let exe = path.rsplit('\\').next().unwrap_or("").to_string();
            let icon = if path.is_empty() {
                None
            } else {
                let mut cache = icon_cache().lock().unwrap();
                cache.entry(path.clone()).or_insert_with(|| load_icon(&path)).clone()
            };
            out.push(AppWindow { hwnd: h.0 as isize, title: c.title, exe, icon });
        }
    }
    dedupe_by_exe(out)
}

#[tauri::command]
pub async fn list_windows() -> Vec<AppWindow> {
    tauri::async_runtime::spawn_blocking(collect_windows).await.unwrap_or_default()
}

#[tauri::command]
pub fn focus_window(hwnd: isize) -> Result<(), String> {
    unsafe {
        let h = HWND(hwnd as *mut core::ffi::c_void);
        if !IsWindow(h).as_bool() {
            return Err("fereastra nu mai există".into());
        }
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        // Windows permite SetForegroundWindow doar procesului din prim-plan;
        // ne „atașăm” temporar la input-ul lui ca să primim permisiunea.
        let fg = GetForegroundWindow();
        let fg_tid = GetWindowThreadProcessId(fg, None);
        let me = GetCurrentThreadId();
        let attached = fg_tid != 0 && fg_tid != me && AttachThreadInput(me, fg_tid, true).as_bool();
        let ok = SetForegroundWindow(h).as_bool();
        let _ = BringWindowToTop(h);
        if attached {
            let _ = AttachThreadInput(me, fg_tid, false);
        }
        if ok { Ok(()) } else { Err("Windows a refuzat aducerea ferestrei în față".into()) }
    }
}
```

- [ ] **Step 4: Leagă în `main.rs`**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hittest;
mod media;
mod platform;
mod windows_list;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .manage(hittest::IslandRect::default())
        .invoke_handler(tauri::generate_handler![
            hittest::set_island_rect,
            media::media_control,
            windows_list::list_windows,
            windows_list::focus_window,
        ])
        .setup(|app| {
            let win = app.get_webview_window("island").expect("fereastra island lipsește");
            platform::place_top_center(&win)?;
            platform::make_no_activate(&win);
            platform::show_no_activate(&win);
            hittest::spawn(app.handle().clone());
            media::spawn(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("eroare la pornirea aplicației");
}
```

- [ ] **Step 5: Rulează testele, trebuie să treacă**

Run: `cd src-tauri; cargo test`
Expected: PASS.

- [ ] **Step 6: Verificare manuală**

Run: `npm run tauri dev`
Expected:
- hover → rândul de jos arată iconițele reale (Chrome, Explorer, Spotify...), cu tooltip cu titlul;
- click pe o iconiță minimizată → fereastra se restaurează și vine în față, iar insula se strânge;
- insula nu apare în propria listă.

- [ ] **Step 7: Commit**

```bash
git add src-tauri
git commit -m "feat: lista aplicatiilor deschise cu iconite si focus"
```

---

### Task 7: Tray, autostart, o singură instanță, build final

**Files:**
- Create: `src-tauri/src/tray.rs`
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Consumes: tot ce e mai sus
- Produces: `tray::setup(&App) -> tauri::Result<()>`, `tray::enable_autostart_on_first_run(&App)`; installer NSIS

- [ ] **Step 1: Creează `src-tauri/src/tray.rs`**

```rust
use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, Manager};
use tauri_plugin_autostart::ManagerExt;

pub fn setup(app: &App) -> tauri::Result<()> {
    let enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(app, "autostart", "Pornește cu Windows", true, enabled, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Ieșire", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&autostart, &quit])?;
    let item = autostart.clone();

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("iconița aplicației lipsește"))
        .tooltip("Dynamic Island")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "quit" => app.exit(0),
            "autostart" => {
                let al = app.autolaunch();
                let was = al.is_enabled().unwrap_or(false);
                let _ = if was { al.disable() } else { al.enable() };
                let _ = item.set_checked(al.is_enabled().unwrap_or(!was));
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// La prima pornire a build-ului release, activează pornirea cu Windows.
pub fn enable_autostart_on_first_run(app: &App) {
    if cfg!(debug_assertions) {
        return;
    }
    let Ok(dir) = app.path().app_config_dir() else { return };
    let marker = dir.join("first-run-done");
    if marker.exists() {
        return;
    }
    let _ = app.autolaunch().enable();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(marker, b"1");
}
```

- [ ] **Step 2: Versiunea finală a `main.rs`**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hittest;
mod media;
mod platform;
mod tray;
mod windows_list;

use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .manage(hittest::IslandRect::default())
        .invoke_handler(tauri::generate_handler![
            hittest::set_island_rect,
            media::media_control,
            windows_list::list_windows,
            windows_list::focus_window,
        ])
        .setup(|app| {
            let win = app.get_webview_window("island").expect("fereastra island lipsește");
            platform::place_top_center(&win)?;
            platform::make_no_activate(&win);
            platform::show_no_activate(&win);
            hittest::spawn(app.handle().clone());
            media::spawn(app.handle().clone());
            tray::enable_autostart_on_first_run(app);
            tray::setup(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("eroare la pornirea aplicației");
}
```

- [ ] **Step 3: Toate testele**

Run: `npm test; cd src-tauri; cargo test`
Expected: PASS peste tot.

- [ ] **Step 4: Build release + installer**

Run: `npm run tauri build`
Expected: `src-tauri/target/release/dynamic-island.exe` și installer-ul în `src-tauri/target/release/bundle/nsis/`.

- [ ] **Step 5: Verificare manuală finală (checklist din spec)**

Pornește `src-tauri/target/release/dynamic-island.exe` și verifică:
- Spotify, YouTube (Chrome/Edge) și un player local apar în insulă; ⏯ ⏭ ⏮ funcționează;
- coperta apare și la schimbarea piesei în Spotify (nu rămâne placeholder);
- seek în YouTube → bara de progres sare corect, fără să fugă;
- aplicație minimizată → click pe iconiță o aduce în față;
- click pe zona goală de lângă insulă trece prin ea;
- fullscreen în YouTube ascunde insula, Esc o readuce;
- dublu-click din nou pe exe → nu apare a doua insulă;
- tray → „Pornește cu Windows” bifat; după repornirea Windows insula pornește singură;
- Task Manager: `dynamic-island.exe` < 60 MB RAM, CPU ~0% în repaus; notează separat totalul cu WebView2.

- [ ] **Step 6: Commit**

```bash
git add .
git commit -m "feat: tray, autostart, instanta unica si build release"
```
