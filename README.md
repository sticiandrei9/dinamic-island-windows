# Dynamic Island pentru Windows

O insulă în stilul notch-ului de MacBook, sus pe centrul ecranului. Din ea vezi ce se aude și ce aplicații ai deschise.

- **Media:** Spotify, YouTube (Chrome/Edge/Firefox), VLC, Media Player și orice aplicație care se raportează la Windows. Ai copertă, titlu, artist, bară de progres și butoanele ⏮ ⏯ ⏭.
- **Aplicații deschise:** iconițele ferestrelor deschise; un click aduce aplicația în față.
- **Animații:** spring cu ușor overshoot, ca pe macOS; conținutul apare cu blur, fade și scale.
- **Discretă:** click-urile trec prin zona din jurul insulei, iar insula se ascunde când un video sau un joc rulează pe tot ecranul.
- **Actualizări automate** din GitHub Releases.

Construită cu [Tauri 2](https://tauri.app) (Rust) și HTML/CSS/JS.

## Instalare

Descarcă `DynamicIsland_<versiune>_x64-setup.exe` din [Releases](https://github.com/sticiandrei9/dinamic-island-windows/releases) și rulează-l.

Installerul nu e semnat cu un certificat Microsoft, așa că Windows poate afișa „Windows protected your PC”. Apasă **More info → Run anyway**.

După instalare, aplicația pornește cu Windows. Poți dezactiva asta din iconița din tray, de unde poți și închide aplicația.

## Dezvoltare

Ai nevoie de Rust (MSVC), Node.js și Visual Studio Build Tools.

```powershell
npm install
npm run tauri dev     # rulează aplicația
npm test              # testele JS
cd src-tauri; cargo test   # testele Rust
```

Ca să vezi designul fără Rust, servește folderul `src/` și deschide `index.html?demo`.

## Publicarea unei versiuni

```powershell
npm run release -- 0.2.0 "ce s-a schimbat"
```

Scriptul crește versiunea, construiește și semnează installerul, generează `latest.json` și creează release-ul pe GitHub. Aplicațiile instalate îl preiau singure: la următoarea pornire sau în cel mult 6 ore.

### Cheia de semnare

Update-urile sunt acceptate doar dacă sunt semnate cu cheia privată din `%USERPROFILE%\.tauri\dynamic-island.key`. Parola ei e în `dynamic-island.password`, în același folder. Cele două fișiere nu sunt în repo.

**Fă-le backup.** Dacă le pierzi, nu mai poți publica update-uri pe care instalările existente să le accepte.

## Licență

[MIT](LICENSE)
