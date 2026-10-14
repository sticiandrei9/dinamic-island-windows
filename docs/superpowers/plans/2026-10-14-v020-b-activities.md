# v0.2.0 · B. Activități — Plan de implementare

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans.

**Goal:** cadrul de activități (pastilă + bulă + tranzitorii), file în insula extinsă, timer/cronometru, indicator microfon/cameră, alerte de sistem, volum/luminozitate cu OSD-ul Windows ascuns, notificări Windows prin UI Automation.

**Spec:** `docs/superpowers/specs/2026-10-14-v020-b-activities-design.md`

## Global Constraints
- Fiecare modul Rust pornește doar dacă e activ în setări. Dacă e oprit din setări, thread-ul lui se oprește singur la următoarea iterație, citind setările. Dacă un API eșuează, modulul scrie în log și se dezactivează.
- Sondări: confidențialitate 1 s, baterie 2 s, luminozitate 500 ms; volumul și notificările merg prin callback-uri.
- Evenimente Rust → JS: `timer-state`, `privacy`, `alert`, `volume`, `notification`.
- Culori: portocaliu `#ff9f0a`, verde `#30d158`, roșu `#ff453a`.

## Review Focus
1. Timer + muzică + microfon activ în același timp → timerul în pastilă, muzica în bulă, punctul portocaliu vizibil.
2. Volum schimbat rapid de 10 ori → o singură activitate tranzitorie, actualizată pe loc, nu 10 la rând.
3. Mai multe alerte simultane (încărcător + Bluetooth) → afișate pe rând, nu suprapuse.
4. Modul oprit în timp ce rulează → dispare imediat din insulă; OSD-ul Windows e restaurat când volumul e oprit.
5. Notificare cu text foarte lung sau fără corp → afișată pe un rând, cu ellipsis, fără să strice insula.

## Tasks
- **B1** `src/lib/activities.js`: `layout(activities, now)` și `queueTransient(queue, activity, now)`. Teste: prioritate, bulă, tranzitorii pe rând, volum actualizat pe loc, expirare.
- **B2** Restructurarea `island.js` / `index.html` / `styles.css`:
  - compactul randat din activitatea din pastilă (sloturi stânga/dreapta);
  - bula (al doilea path SVG, cerc, spring separat);
  - stratul tranzitoriu;
  - fila-bar în extins (Acasă / Timer / Raft / Oglindă) cu crossfade.
  Media devine o activitate.
- **B3** Timer:
  - `src-tauri/src/timer.rs`: ceas unic, comenzile `timer_start(seconds)` / `timer_pause` / `timer_resume` / `timer_cancel` și `stopwatch_*`, emite `timer-state`, sunetul de alarmă de 3 ori la final;
  - funcțiile pure `remaining(state, now)` și `elapsed(state, now)`, testate;
  - `src/lib/timer.js` (`formatClock`, testat);
  - fila Timer.
- **B4** `privacy.rs`: citirea din registru a `ConsentStore`; `in_use(entries, self_exe)`, testat; emite `privacy {mic: [app], cam: [app]}`; punctele din UI.
- **B5** `alerts.rs`:
  - baterie: `GetSystemPowerStatus` la 2 s, `power_transition`, testat;
  - Bluetooth: DeviceWatcher WinRT (AEP, conectat) + proprietatea de baterie, `bt_transition`, testat;
  - emite `alert {kind, title, detail, level?}`.
- **B6** `volume.rs`:
  - `IAudioEndpointVolume` + `IAudioEndpointVolumeCallback` și `IMMNotificationClient` pentru dispozitivul implicit;
  - luminozitatea prin WMI (`WmiMonitorBrightness`), dacă există;
  - ascunderea și restaurarea OSD-ului Windows;
  - `wave_level(level, muted) -> 0..3`, testat;
  - emite `volume {kind: "volume"|"brightness", level, muted}`.
- **B7** `notifications.rs`: UI Automation `WindowOpened`, filtrul pentru toast, `parse_toast(texts)`, testat; emite `notification {app, title, body, icon?}`.
- **B8** Verificare manuală: scenariile din Review Focus și testarea modulelor pe calculatorul lui Andrei.
