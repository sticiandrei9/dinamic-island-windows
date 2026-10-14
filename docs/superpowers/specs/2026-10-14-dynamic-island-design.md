# Dynamic Island pentru Windows — Specificație de design

Data: 2026-10-14
Stack: Tauri 2 (Rust) + HTML/CSS/JS (fără framework)

## 1. Scop

O insulă în stilul notch-ului de MacBook / Dynamic Island, așezată sus pe centrul ecranului principal, mereu deasupra tuturor ferestrelor. Din ea vezi ce se aude (YouTube din browser, Spotify, playere) și ce aplicații sunt deschise, și le poți controla.

Criterii de succes:
- arată și se mișcă aproape identic cu insula de pe MacBook (formă, negru pur, animații spring);
- detectează media din orice aplicație care se raportează la Windows (Spotify, Chrome/Edge/Firefox cu YouTube, VLC, Groove, Apple Music etc.);
- play/pauză/next/prev funcționează din insulă;
- click pe o aplicație din insulă o aduce în față;
- consum mic: sub ~60 MB RAM, CPU ~0% în repaus;
- pornește odată cu Windows (dezactivabil din tray).

În afara scopului (YAGNI): volum, notificări, mai multe monitoare, setări avansate, teme.

## 2. Design vizual (referință: notch MacBook)

**Formă:** lipită de marginea de sus a ecranului (fără spațiu deasupra), negru pur `#000`. Colțurile de jos sunt rotunjite (raza ~12px compactă, ~32px extinsă). Sus are „urechile” concave caracteristice notch-ului: colțurile de sus se curbează spre exterior, în marginea ecranului. Forma e desenată ca path SVG / `clip-path`, ca să rămână continuă în timpul animației.

**Stări:**
1. **Compactă (repaus):** ~185×32 px. Doar negru, ca un notch.
2. **Activă (cântă ceva):** se lățește la ~300×32. În stânga apare coperta mică (20px, colțuri 5px), iar în dreapta un vizualizator cu 4–5 bare animate, colorate cu culoarea dominantă a copertei.
3. **Extinsă (hover ~150ms sau click):** ~420×190, colțuri jos ~32px. Conține:
   - coperta 56px (colțuri 12px), titlu (SF Pro / Segoe UI Variable semibold, alb), artist (gri `#8e8e93`), vizualizatorul în dreapta;
   - bara de progres cu timpul curent și durata, când sursa le oferă;
   - butoanele ⏮ ⏯ ⏭ (stil SF Symbols ca SVG, alb, cu feedback scale 0.85 la apăsare);
   - un rând cu iconițele aplicațiilor deschise (28px), cu tooltip cu numele; click = adu în față.
   Când nu cântă nimic, se vede doar rândul de aplicații și ora/data.

**Animații:**
- schimbarea dimensiunii folosește un spring cu răspuns ~0.45s și amortizare ~0.75, cu o ușoară depășire (overshoot), ca pe macOS. E implementat ca integrator spring în JS (requestAnimationFrame), nu ca ease CSS;
- conținutul intră cu fade + blur (8px → 0) + scale (0.9 → 1), decalat cu ~40ms față de formă; la ieșire, invers și mai rapid;
- la schimbarea piesei, coperta face crossfade, iar textul alunecă scurt pe verticală;
- barele vizualizatorului sunt animate procedural (nu analizăm audio real) și se opresc lin la pauză.

## 3. Arhitectură

```
dynamic-island/
  src-tauri/
    src/main.rs        — pornire, fereastră, tray, autostart, comenzi Tauri
    src/media.rs       — GSMTC: citește sesiunea curentă, emite evenimente, comenzi play/next/prev
    src/windows_list.rs— enumerare ferestre vizibile + iconițe + aducere în față
    src/hittest.rs     — click-through: fereastra ignoră mouse-ul în afara formei insulei
  src/
    index.html, styles.css
    island.js          — mașina de stări (compact/activ/extins) + spring
    media-view.js, apps-view.js
```

**Fereastra:** transparentă, fără margini, mereu deasupra, fără iconiță în taskbar, de mărimea maximă a insulei extinse (+ marjă pentru umbră), poziționată la y=0, centrată pe monitorul principal. Click-through: un thread Rust verifică poziția cursorului (~60Hz, doar cât cursorul e aproape) față de dreptunghiul curent al insulei, raportat de JS, și comută `set_ignore_cursor_events`.

**Media (`media.rs`):** crate-ul `windows` cu `GlobalSystemMediaTransportControlsSessionManager`. Se abonează la `CurrentSessionChanged`, `MediaPropertiesChanged`, `PlaybackInfoChanged` și `TimelinePropertiesChanged` și trimite către frontend evenimentul `media-update { title, artist, album, thumbnail (data URL base64), playing, position, duration, source_app }`. Comenzile `media_play_pause`, `media_next`, `media_prev`. Culoarea dominantă a copertei se calculează în JS pe canvas.

**Aplicații (`windows_list.rs`):** `EnumWindows` filtrat (vizibile, fără owner, cu titlu, fără ferestre-unealtă, fără insula însăși). Pentru fiecare: hwnd, titlu, numele exe-ului și iconița (extrasă din exe, convertită în PNG base64, cu cache per exe). Se reîmprospătează la deschiderea stării extinse și la fiecare 2s cât e extinsă. Comanda `focus_window(hwnd)` folosește `ShowWindow(SW_RESTORE)` dacă fereastra e minimizată, apoi `SetForegroundWindow` (cu trucul AttachThreadInput dacă e nevoie).

**Tray + autostart:** iconiță în tray cu „Pornește cu Windows ✓” și „Ieșire”. Autostart prin `tauri-plugin-autostart` (cheia Run din registru), activat implicit la prima pornire.

## 4. Erori și cazuri limită

- nu există nicio sesiune media → starea activă dispare și insula revine la compactă;
- sursa nu oferă copertă → placeholder (notă muzicală pe gri închis);
- sursa nu oferă timeline (unele browsere) → bara de progres e ascunsă;
- o comandă media eșuează → se ignoră în tăcere, fără crash;
- erorile GSMTC sunt logate, iar managerul e re-creat după 5s;
- o fereastră închisă între listare și click → `focus_window` returnează eroare și lista se reîmprospătează.

## 5. Testare

- Rust: teste unitare pentru filtrul de ferestre și conversia datelor media (funcții pure);
- manual: Spotify, YouTube în Chrome/Edge, un player local, pauză/next din insulă, aplicație minimizată adusă în față, click-through pe zona goală, autostart după repornire, RAM în Task Manager.
