# Auto-updater — Specificație de design

Data: 2026-10-14
Depinde de: `2026-10-14-dynamic-island-design.md` (aplicația Tauri existentă)

## 1. Scop

Când Andrei publică o versiune îmbunătățită, aplicația instalată pe calculator o găsește, o descarcă, o verifică și se reinstalează singură, fără click-uri.

Criterii de succes:
- o versiune publicată pe GitHub Releases ajunge automat pe calculator în cel mult ~6 ore, sau imediat la următoarea pornire;
- se instalează doar update-urile semnate cu cheia lui Andrei;
- dacă nu există internet, release-ul lipsește sau semnătura e greșită, aplicația merge normal și reîncearcă mai târziu;
- publicarea unei versiuni noi se face cu o singură comandă.

În afara scopului (YAGNI): canale beta, rollback, note de versiune afișate în insulă, actualizări din repo privat.

## 2. Hosting

- Repo: `https://github.com/sticiandrei9/dinamic-island-windows`. Acum e privat; va deveni public, cu licență MIT.
- Endpoint-ul de verificare: `https://github.com/sticiandrei9/dinamic-island-windows/releases/latest/download/latest.json`.
- **Limitare acceptată:** cât timp repo-ul e privat, endpoint-ul cere autentificare, deci verificarea eșuează (în tăcere). Actualizările automate pornesc efectiv după ce repo-ul devine public. Nu punem token GitHub în aplicație.

## 3. Semnare

- Pereche de chei Tauri (minisign), generată cu `tauri signer generate`.
- Cheia privată stă în `C:\Users\Andre\.tauri\dynamic-island.key`, iar parola în `C:\Users\Andre\.tauri\dynamic-island.password`. Ambele sunt în afara repo-ului și nu sunt urcate nicăieri. Fără cheie nu se mai pot publica update-uri, deci Andrei trebuie să le salveze și într-un backup.
- Cheia publică e pusă în `tauri.conf.json` (`plugins.updater.pubkey`).

## 4. În aplicație

- `tauri-plugin-updater` 2, configurat cu endpoint-ul de mai sus și `windows.installMode = "quiet"` (installer per-user, fără ferestre).
- Build-ul produce artefacte de update (`bundle.createUpdaterArtifacts = true`): installerul NSIS și fișierul `.sig`.
- Modulul `updater.rs` pornește un thread care verifică prima dată la ~30 s după pornire, apoi la fiecare 6 ore. Dacă găsește o versiune mai nouă:
  1. trimite evenimentul `update-installing` cu versiunea;
  2. insula afișează în starea compactă „Actualizare la vX.Y.Z”, timp de ~2 s;
  3. `download_and_install` descarcă, verifică semnătura și rulează installerul. Aplicația se închide, iar installerul o repornește.
- Orice eroare (rețea, 404, semnătură) e logată și ignorată până la următoarea verificare.
- Verificarea nu rulează în build-ul debug.

## 5. Publicare (`scripts/release.mjs`)

`npm run release -- 0.2.0 "note opționale"`:
1. validează versiunea (semver, mai mare decât cea curentă) și că working tree-ul e curat;
2. scrie versiunea în `package.json`, `src-tauri/Cargo.toml` și `src-tauri/tauri.conf.json`;
3. rulează `tauri build` cu `TAURI_SIGNING_PRIVATE_KEY` și `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` citite din fișierele din §3;
4. copiază installerul sub un nume fără spații (`DynamicIsland_<v>_x64-setup.exe`), pentru că GitHub înlocuiește spațiile din numele fișierelor;
5. generează `latest.json` (`version`, `notes`, `pub_date`, `platforms.windows-x86_64.{signature, url}`);
6. face commit „release: v<v>”, tag `v<v>`, push, apoi `gh release create v<v>` cu installerul și `latest.json`.

## 6. Repo

- Branch-ul `feat/island` e unit în `main` (redenumit din `master`).
- `LICENSE` (MIT, „Copyright (c) 2026 Andrei”), plus `README.md` cu instalare, utilizare și publicare.
- Push pe `origin` = repo-ul de mai sus. Push-ul și primul release se fac doar după confirmarea lui Andrei.

## 7. Testare

- Node: testele pentru `release.mjs` (validare versiune, actualizare fișiere, nume asset, `latest.json`).
- Rust: testul pentru calculul următoarei verificări.
- End-to-end, local, înainte de primul release real: un build vechi (0.1.0) care citește `latest.json` de pe un server local (override de config cu `dangerousInsecureTransportProtocol`, doar pentru test) se actualizează singur la 0.1.1. Testul înlocuiește copia instalată a lui Andrei, deci cere confirmarea lui.
