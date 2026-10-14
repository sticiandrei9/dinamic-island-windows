# Dynamic Island for Windows

**[English](#english) · [Română](#română) · [Русский](#русский)**

---

## English

A MacBook-notch-style island at the top center of your screen. It shows what's playing and which apps are open.

### Features

- **Media:** Spotify, YouTube (Chrome/Edge/Firefox), VLC, Media Player and any app that reports to Windows. Shows cover art, title, artist and a progress bar, with ⏮ ⏯ ⏭ controls.
- **Open apps:** icons of your open windows; click one to bring it to the front.
- **macOS-like motion:** spring animations with a slight overshoot; content fades in with blur and scale.
- **Stays out of the way:** clicks pass through the area around the island, and it hides while a video or game runs full screen.
- **Automatic updates** from GitHub Releases.

Built with [Tauri 2](https://tauri.app) (Rust) and HTML/CSS/JS.

### Installation

Download `DynamicIsland_<version>_x64-setup.exe` from [Releases](https://github.com/sticiandrei9/dinamic-island-windows/releases) and run it.

The installer isn't signed with a Microsoft certificate, so Windows may show "Windows protected your PC". Click **More info → Run anyway**.

The app starts with Windows. You can turn that off from the tray icon, which also has an Exit option. Updates install by themselves: at the next start, or within 6 hours.

### Development

Requirements: Rust (MSVC), Node.js, Visual Studio Build Tools.

```powershell
npm install
npm run tauri dev          # run the app
npm test                   # JS tests
cd src-tauri; cargo test   # Rust tests
```

To preview the design without Rust, serve the `src/` folder and open `index.html?demo`.

### Publishing a release

```powershell
npm run release -- 0.2.0 "what changed"
```

This bumps the version, builds and signs the installer, writes `latest.json` and creates the GitHub release. Installed copies pick it up automatically.

Updates are only accepted when signed with the private key in `%USERPROFILE%\.tauri\dynamic-island.key` (password in `dynamic-island.password`). These files are not in the repository. **Back them up:** without them you can't publish updates that existing installs will accept.

---

## Română

O insulă în stilul notch-ului de MacBook, sus pe centrul ecranului. Din ea vezi ce se aude și ce aplicații ai deschise.

### Funcții

- **Media:** Spotify, YouTube (Chrome/Edge/Firefox), VLC, Media Player și orice aplicație care se raportează la Windows. Ai copertă, titlu, artist și bară de progres, plus butoanele ⏮ ⏯ ⏭.
- **Aplicații deschise:** iconițele ferestrelor deschise; un click aduce aplicația în față.
- **Animații ca pe macOS:** spring cu ușor overshoot; conținutul apare cu blur, fade și scale.
- **Discretă:** click-urile trec prin zona din jurul insulei, iar insula se ascunde când un video sau un joc rulează pe tot ecranul.
- **Actualizări automate** din GitHub Releases.

Construită cu [Tauri 2](https://tauri.app) (Rust) și HTML/CSS/JS.

### Instalare

Descarcă `DynamicIsland_<versiune>_x64-setup.exe` din [Releases](https://github.com/sticiandrei9/dinamic-island-windows/releases) și rulează-l.

Installerul nu e semnat cu un certificat Microsoft, așa că Windows poate afișa „Windows protected your PC”. Apasă **More info → Run anyway**.

Aplicația pornește cu Windows. Poți dezactiva asta din iconița din tray, de unde o poți și închide. Actualizările se instalează singure: la următoarea pornire sau în cel mult 6 ore.

### Dezvoltare

Ai nevoie de Rust (MSVC), Node.js și Visual Studio Build Tools.

```powershell
npm install
npm run tauri dev          # rulează aplicația
npm test                   # testele JS
cd src-tauri; cargo test   # testele Rust
```

Ca să vezi designul fără Rust, servește folderul `src/` și deschide `index.html?demo`.

### Publicarea unei versiuni

```powershell
npm run release -- 0.2.0 "ce s-a schimbat"
```

Scriptul crește versiunea, construiește și semnează installerul, scrie `latest.json` și creează release-ul pe GitHub. Copiile instalate îl preiau automat.

Update-urile sunt acceptate doar dacă sunt semnate cu cheia privată din `%USERPROFILE%\.tauri\dynamic-island.key` (parola e în `dynamic-island.password`). Fișierele nu sunt în repo. **Fă-le backup:** fără ele nu mai poți publica update-uri pe care instalările existente să le accepte.

---

## Русский

Островок в стиле «чёлки» MacBook в верхней части экрана по центру. Показывает, что сейчас играет и какие приложения открыты.

### Возможности

- **Медиа:** Spotify, YouTube (Chrome/Edge/Firefox), VLC, Медиаплеер и любое приложение, которое передаёт данные в Windows. Обложка, название, исполнитель, полоса прогресса и кнопки ⏮ ⏯ ⏭.
- **Открытые приложения:** значки открытых окон; по клику окно выводится на передний план.
- **Анимации как в macOS:** пружинные анимации с лёгким «перелётом»; содержимое появляется с размытием, затуханием и масштабированием.
- **Не мешает:** клики проходят сквозь область вокруг островка, а при полноэкранном видео или игре он скрывается.
- **Автоматические обновления** из GitHub Releases.

Сделано на [Tauri 2](https://tauri.app) (Rust) и HTML/CSS/JS.

### Установка

Скачайте `DynamicIsland_<версия>_x64-setup.exe` из [Releases](https://github.com/sticiandrei9/dinamic-island-windows/releases) и запустите.

Установщик не подписан сертификатом Microsoft, поэтому Windows может показать «Система Windows защитила ваш компьютер». Нажмите **Подробнее → Выполнить в любом случае**.

Приложение запускается вместе с Windows. Это можно отключить через значок в трее, там же есть выход из приложения. Обновления устанавливаются сами: при следующем запуске или в течение 6 часов.

### Разработка

Нужны Rust (MSVC), Node.js и Visual Studio Build Tools.

```powershell
npm install
npm run tauri dev          # запустить приложение
npm test                   # тесты JS
cd src-tauri; cargo test   # тесты Rust
```

Чтобы посмотреть дизайн без Rust, раздайте папку `src/` и откройте `index.html?demo`.

### Публикация версии

```powershell
npm run release -- 0.2.0 "что изменилось"
```

Скрипт повышает версию, собирает и подписывает установщик, создаёт `latest.json` и релиз на GitHub. Установленные копии получают его автоматически.

Обновления принимаются, только если они подписаны закрытым ключом из `%USERPROFILE%\.tauri\dynamic-island.key` (пароль в `dynamic-island.password`). Этих файлов нет в репозитории. **Сделайте резервную копию:** без них нельзя выпустить обновления, которые примут уже установленные копии.

---

## License / Licență / Лицензия

[MIT](LICENSE)
