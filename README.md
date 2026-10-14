# Dynamic Island for Windows

**[English](#english) · [Română](#română) · [Русский](#русский)**

---

## English

A MacBook-notch-style island at the top center of your screen. It shows what's playing and which apps are open.

### Features

- **Media:** Spotify, YouTube (Chrome/Edge/Firefox), VLC, Media Player and any app that reports to Windows. Shows cover art, title, artist and a draggable progress bar, with ⏮ ⏯ ⏭ controls. Click the cover or title to bring the playing app to the front.
- **Synced lyrics** from [LRCLIB](https://lrclib.net) under the track, line by line.
- **Live Activities:** a timer and stopwatch; when two things run at once (e.g. timer + music), the second one appears as a small bubble next to the island, like on iPhone.
- **Alerts:** charger plugged in/out, low battery, Bluetooth headphones and devices connecting (with battery level).
- **Volume and brightness** shown in the island. On Windows 11 the system volume pop-up stays visible too.
- **Windows notifications** mirrored in the island (read from the screen; some apps may be missed).
- **Privacy dots:** orange when an app uses the microphone, green for the camera.
- **File shelf:** drag files onto the island to keep them at hand, then drag them out into any app.
- **Mirror:** a quick look at your webcam before a call.
- **Open apps:** icons of your open windows; click one to bring it to the front.
- **Settings:** turn each module on or off, expand on hover or click, pick the monitor (or all monitors) and the size.
- **macOS-like motion:** spring animations with a slight overshoot; content fades in with blur and scale.
- **Stays out of the way:** clicks pass through the area around the island, and it hides while a video or game runs full screen.
- **Updates:** downloaded automatically from GitHub Releases; you get a notification and apply them from the tray (or the island). If you don't, they install at the next start.

Built with [Tauri 2](https://tauri.app) (Rust) and HTML/CSS/JS.

### Installation

Download `DynamicIsland_<version>_x64-setup.exe` from [Releases](https://github.com/sticiandrei9/dinamic-island-windows/releases) and run it.

The installer isn't signed with a Microsoft certificate, so Windows may show "Windows protected your PC". Click **More info → Run anyway**.

The app starts with Windows. Right-click the tray icon for the version, **Aplică update** (apply update), **Setări…** (settings), autostart and exit. The app's interface is in Romanian.

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

- **Media:** Spotify, YouTube (Chrome/Edge/Firefox), VLC, Media Player și orice aplicație care se raportează la Windows. Ai copertă, titlu, artist și o bară de progres pe care o poți trage, plus butoanele ⏮ ⏯ ⏭. Click pe copertă sau titlu aduce în față aplicația care cântă.
- **Versuri sincronizate** de pe [LRCLIB](https://lrclib.net), rând cu rând, sub piesă.
- **Activități ca pe iPhone:** timer și cronometru; când rulează două lucruri deodată (de ex. timer + muzică), al doilea apare ca o bulă mică lângă insulă.
- **Alerte:** încărcător conectat/deconectat, baterie scăzută, căști și dispozitive Bluetooth conectate (cu nivelul bateriei).
- **Volum și luminozitate** afișate în insulă. Pe Windows 11 rămâne vizibil și pop-up-ul de volum al sistemului.
- **Notificările Windows** apar și în insulă (citite de pe ecran; unele aplicații pot fi ratate).
- **Puncte de confidențialitate:** portocaliu când o aplicație folosește microfonul, verde pentru cameră.
- **Raft de fișiere:** tragi fișiere pe insulă ca să le ai la îndemână, apoi le tragi de acolo în orice aplicație.
- **Oglindă:** te vezi rapid în camera web înainte de un apel.
- **Aplicații deschise:** iconițele ferestrelor deschise; un click aduce aplicația în față.
- **Setări:** pornești sau oprești fiecare modul, alegi extinderea la hover sau la click, monitorul (sau toate) și mărimea.
- **Animații ca pe macOS:** spring cu ușor overshoot; conținutul apare cu blur, fade și scale.
- **Discretă:** click-urile trec prin zona din jurul insulei, iar insula se ascunde când un video sau un joc rulează pe tot ecranul.
- **Actualizări:** se descarcă singure din GitHub Releases; primești o notificare și le aplici din tray (sau din insulă). Dacă nu apeși, se instalează la următoarea pornire.

Construită cu [Tauri 2](https://tauri.app) (Rust) și HTML/CSS/JS.

### Instalare

Descarcă `DynamicIsland_<versiune>_x64-setup.exe` din [Releases](https://github.com/sticiandrei9/dinamic-island-windows/releases) și rulează-l.

Installerul nu e semnat cu un certificat Microsoft, așa că Windows poate afișa „Windows protected your PC”. Apasă **More info → Run anyway**.

Aplicația pornește cu Windows. Click dreapta pe iconița din tray: versiunea, **Aplică update**, **Setări…**, pornirea cu Windows și Ieșire.

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

- **Медиа:** Spotify, YouTube (Chrome/Edge/Firefox), VLC, Медиаплеер и любое приложение, которое передаёт данные в Windows. Обложка, название, исполнитель, полоса прогресса, которую можно перетаскивать, и кнопки ⏮ ⏯ ⏭. Клик по обложке или названию выводит играющее приложение на передний план.
- **Синхронизированные тексты песен** с [LRCLIB](https://lrclib.net), строка за строкой.
- **Активности как на iPhone:** таймер и секундомер; когда одновременно идут две вещи (например, таймер и музыка), вторая показывается маленьким пузырьком рядом с островком.
- **Оповещения:** подключение/отключение зарядки, низкий заряд батареи, подключение Bluetooth-наушников и устройств (с уровнем заряда).
- **Громкость и яркость** отображаются в островке. В Windows 11 системное окно громкости тоже остаётся видимым.
- **Уведомления Windows** дублируются в островке (считываются с экрана; некоторые приложения могут быть пропущены).
- **Индикаторы приватности:** оранжевая точка — приложение использует микрофон, зелёная — камеру.
- **Полка для файлов:** перетащите файлы на островок, чтобы держать их под рукой, а потом перетащите их в любое приложение.
- **Зеркало:** быстро взглянуть на себя через веб-камеру перед звонком.
- **Открытые приложения:** значки открытых окон; по клику окно выводится на передний план.
- **Настройки:** включение и выключение каждого модуля, раскрытие по наведению или по клику, выбор монитора (или всех) и размера.
- **Анимации как в macOS:** пружинные анимации с лёгким «перелётом»; содержимое появляется с размытием, затуханием и масштабированием.
- **Не мешает:** клики проходят сквозь область вокруг островка, а при полноэкранном видео или игре он скрывается.
- **Обновления:** скачиваются автоматически из GitHub Releases; вы получаете уведомление и применяете их из трея (или из островка). Если не нажать, они установятся при следующем запуске.

Сделано на [Tauri 2](https://tauri.app) (Rust) и HTML/CSS/JS.

### Установка

Скачайте `DynamicIsland_<версия>_x64-setup.exe` из [Releases](https://github.com/sticiandrei9/dinamic-island-windows/releases) и запустите.

Установщик не подписан сертификатом Microsoft, поэтому Windows может показать «Система Windows защитила ваш компьютер». Нажмите **Подробнее → Выполнить в любом случае**.

Приложение запускается вместе с Windows. Правый клик по значку в трее: версия, **Aplică update** (применить обновление), **Setări…** (настройки), автозапуск и выход. Интерфейс приложения на румынском языке.

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
