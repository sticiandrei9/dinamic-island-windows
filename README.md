# Dynamic Island for Windows

**[English](#english) · [Română](#română) · [Русский](#русский)**

---

## English

A MacBook-notch-style island at the top center of your screen. It shows what's playing and which apps are open.

### Features

- **Media:** Spotify, YouTube (Chrome/Edge/Firefox), VLC, Media Player and any app that reports to Windows. Shows cover art, title, artist and a draggable progress bar, with ⏮ ⏯ ⏭ controls. Click the cover or title to bring the playing app to the front.
- **Synced lyrics** from [LRCLIB](https://lrclib.net) under the track, line by line. The title and artist of each track (including YouTube video titles) are sent to lrclib.net; turn off the Lyrics module to stop that.
- **Pomodoro** in the Timer tab: 25 min focus, 5 min break, a 15 min break after the 4th round; phases switch on their own, with a sound.
- **Calendar:** paste your Google Calendar secret iCal link in Settings; the island shows today's next event and, 10 minutes before a meeting, a countdown in the pill. The link stays on your PC.
- **Keyboard:** switching the input language shows “Русский · RU” for a moment; Caps Lock shows On/Off.
- **Shortcut:** set a key combination in Settings to open or close the island without the mouse.
- **Audio output:** pick speakers or headphones from the Home tab (shown when you have more than one).
- **Network:** internet lost or back, the Wi-Fi name, VPN connected or disconnected.
- **Live Activities:** a timer and stopwatch; when two things run at once (e.g. timer + music), the second one appears as a small bubble next to the island, like on iPhone.
- **Alerts:** charger plugged in/out, low battery, Bluetooth headphones and devices connecting (with battery level).
- **Volume and brightness** shown in the island. On Windows 11 the system volume pop-up stays visible too.
- **Windows notifications** mirrored in the island (read from the screen; some apps may be missed).
- **Privacy dots:** orange when an app uses the microphone, green for the camera.
- **Calls:** while an app uses the microphone (Teams, Zoom, Discord, Meet…), the island shows the app and the call duration, with a button that mutes the Windows microphone.
- **Screenshots:** after Win+Shift+S a thumbnail appears in the island with Save and To shelf (if Snipping Tool already saved the file, that file is used).
- **File shelf:** drag files onto the island to keep them at hand, then drag them out into any app. Right-click a file to copy its path; old files can leave the shelf on their own after 1, 7 or 30 days.
- **Mirror:** a quick look at your webcam before a call.
- **Open apps:** icons of your open windows; click one to bring it to the front.
- **Claude Code:** turn on the Claude module and the island adds a small hook to Claude Code's settings (removed when you turn the module off or uninstall). The pill shows while a session works, turns orange when Claude waits for your approval or answer, and a green “Claude finished” appears at the end; click it to bring that window to the front. Works in the terminal and in the Code tab of Claude Desktop.
- **Downloads:** while Chrome, Edge or Firefox downloads a file, the pill shows a spinning ring with the size and speed; when it's done, the file appears with Open and Show in folder, and you can drag it into any app.
- **Unlock:** after you unlock Windows with your PIN or Windows Hello, a small padlock opens in the island.
- **System tab:** CPU, memory and GPU usage, GPU temperature (NVIDIA, only while the GPU is already busy), current download/upload speed and ping to 1.1.1.1. Sensors are read only while the tab is open.
- **Clipboard** (off by default): the last 5 copied texts, kept in memory only; each new copy shows its first characters for 2 s. Copies from password managers are skipped.
- **Sleep timer:** 15, 30 or 60 min from the Timer tab; the volume fades out over the last 10 s, the music pauses and the volume comes back.
- **Notes:** a quick checklist in its own tab, with an optional global shortcut to start typing.
- **Real visualizer:** the bars in the pill and next to the cover move with the sound coming out of your speakers, in the cover's color; they rest when the music is silent or muted.
- **Control center** in the Home tab: Wi-Fi, Bluetooth, dark mode, Do Not Disturb, microphone, lock and sleep (sleep asks for a second click). A button that doesn't work on your system is hidden.
- **Bluetooth batteries:** the battery of your connected headphones, mouse or controller next to the clock (red below 20 %).
- **CI and deploys** (off by default): GitHub Actions runs from your own repos (through the `gh` CLI) and Cloudflare Pages / Workers builds (with a read-only token). A spinning ring while it runs, then a green ✓ or red ✗ for 5 s; click to open the run.
- The Claude module shows the real Claude icon, taken from the Claude Desktop app installed on your PC.
- **Settings:** turn each module on or off, expand on hover or click, pick the monitor (or all monitors), the size and the language (English, Română, Русский; automatic follows Windows).
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
- **Versuri sincronizate** de pe [LRCLIB](https://lrclib.net), rând cu rând, sub piesă. Titlul și artistul fiecărei piese (inclusiv titlurile video-urilor YouTube) sunt trimise la lrclib.net; oprește modulul Versuri dacă nu vrei asta.
- **Activități ca pe iPhone:** timer și cronometru; când rulează două lucruri deodată (de ex. timer + muzică), al doilea apare ca o bulă mică lângă insulă.
- **Alerte:** încărcător conectat/deconectat, baterie scăzută, căști și dispozitive Bluetooth conectate (cu nivelul bateriei).
- **Volum și luminozitate** afișate în insulă. Pe Windows 11 rămâne vizibil și pop-up-ul de volum al sistemului.
- **Notificările Windows** apar și în insulă (citite de pe ecran; unele aplicații pot fi ratate).
- **Puncte de confidențialitate:** portocaliu când o aplicație folosește microfonul, verde pentru cameră.
- **Apeluri:** cât o aplicație folosește microfonul (Teams, Zoom, Discord, Meet…), insula arată aplicația și durata apelului, cu un buton care oprește microfonul Windows.
- **Pomodoro** în fila Timer: 25 min lucru, 5 min pauză, iar după a 4-a rundă o pauză de 15 min; etapele se schimbă singure, cu sunet.
- **Calendar:** pui în Setări linkul secret iCal din Google Calendar; insula arată următorul eveniment de azi și, cu 10 minute înainte de o întâlnire, numărătoarea în pastilă. Linkul rămâne doar pe PC-ul tău.
- **Tastatură:** la schimbarea limbii apare „Русский · RU” pentru o clipă; Caps Lock arată Pornit/Oprit.
- **Scurtătură:** alegi în Setări o combinație de taste care deschide sau strânge insula fără mouse.
- **Ieșire audio:** alegi difuzoarele sau căștile din fila Acasă (apare când ai mai mult de un dispozitiv).
- **Rețea:** internet pierdut sau revenit, numele Wi-Fi-ului, VPN conectat sau deconectat.
- **Capturi de ecran:** după Win+Shift+S apare în insulă o miniatură cu Salvează și Pe raft (dacă Instrumentul de decupare a salvat deja fișierul, se folosește acela).
- **Raft de fișiere:** tragi fișiere pe insulă ca să le ai la îndemână, apoi le tragi de acolo în orice aplicație. Click dreapta pe un fișier îi copiază calea; fișierele vechi pot ieși singure după 1, 7 sau 30 de zile.
- **Oglindă:** te vezi rapid în camera web înainte de un apel.
- **Aplicații deschise:** iconițele ferestrelor deschise; un click aduce aplicația în față.
- **Claude Code:** pornești modulul Claude și insula adaugă un mic hook în setările Claude Code (îl scoate când oprești modulul sau dezinstalezi aplicația). Pastila arată cât lucrează o sesiune, devine portocalie când Claude îți așteaptă aprobarea sau răspunsul, iar la final apare „Claude a terminat”, cu verde; un click aduce fereastra aceea în față. Merge în terminal și în fila Code din Claude Desktop.
- **Descărcări:** cât Chrome, Edge sau Firefox descarcă un fișier, pastila arată un inel care se rotește, cu mărimea și viteza; la final apare fișierul, cu Deschide și Arată în folder, și îl poți trage în orice aplicație.
- **Deblocare:** după ce deblochezi Windows cu PIN-ul sau Windows Hello, în insulă se deschide un mic lacăt.
- **Fila Sistem:** procesorul, memoria și placa video, temperatura plăcii video (NVIDIA, doar când placa lucrează deja), viteza de descărcare/încărcare de acum și ping-ul spre 1.1.1.1. Senzorii se citesc doar cât fila e deschisă.
- **Clipboard** (oprit implicit): ultimele 5 texte copiate, păstrate doar în memorie; la fiecare copiere apar 2 s primele caractere. Ce copiezi din managerele de parole e sărit.
- **Timer de somn:** 15, 30 sau 60 de minute, din fila Timer; în ultimele 10 s volumul scade treptat, muzica se oprește, iar volumul revine.
- **Notițe:** o listă rapidă cu bifă, în fila ei, cu o scurtătură globală opțională ca să începi să scrii.
- **Vizualizator real:** barele din pastilă și de lângă copertă se mișcă după sunetul care iese din difuzoare, în culoarea copertei; stau jos când muzica e pe liniște sau pe mute.
- **Centru de control** în fila Acasă: Wi-Fi, Bluetooth, mod întunecat, Nu deranja, microfon, blocare și sleep (sleep cere al doilea click). Un buton care nu merge pe sistemul tău nu apare.
- **Baterii Bluetooth:** bateria căștilor, mouse-ului sau controller-ului conectat, lângă ceas (roșu sub 20 %).
- **CI și deploy-uri** (oprit implicit): rulările GitHub Actions din repo-urile tale (prin `gh`) și build-urile Cloudflare Pages / Workers (cu un token doar de citire). Un inel cât rulează, apoi 5 s un ✓ verde sau un ✗ roșu; un click deschide rularea.
- Modulul Claude arată iconița reală Claude, luată din aplicația Claude Desktop instalată pe PC.
- **Setări:** pornești sau oprești fiecare modul, alegi extinderea la hover sau la click, monitorul (sau toate), mărimea și limba (Română, English, Русский; „Automat” urmează limba Windows).
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
- **Синхронизированные тексты песен** с [LRCLIB](https://lrclib.net), строка за строкой. Название и исполнитель каждой песни (включая названия видео YouTube) отправляются на lrclib.net; отключите модуль «Тексты песен», если не хотите этого.
- **Активности как на iPhone:** таймер и секундомер; когда одновременно идут две вещи (например, таймер и музыка), вторая показывается маленьким пузырьком рядом с островком.
- **Оповещения:** подключение/отключение зарядки, низкий заряд батареи, подключение Bluetooth-наушников и устройств (с уровнем заряда).
- **Громкость и яркость** отображаются в островке. В Windows 11 системное окно громкости тоже остаётся видимым.
- **Уведомления Windows** дублируются в островке (считываются с экрана; некоторые приложения могут быть пропущены).
- **Индикаторы приватности:** оранжевая точка — приложение использует микрофон, зелёная — камеру.
- **Звонки:** пока приложение использует микрофон (Teams, Zoom, Discord, Meet…), островок показывает приложение и длительность звонка, с кнопкой, которая выключает микрофон Windows.
- **Помодоро** на вкладке «Таймер»: 25 мин работы, 5 мин перерыва, после 4-го раунда перерыв 15 мин; этапы сменяются сами, со звуком.
- **Календарь:** вставьте в настройках закрытую ссылку iCal из Google Календаря; островок показывает следующее событие на сегодня, а за 10 минут до встречи — обратный отсчёт. Ссылка хранится только на вашем ПК.
- **Клавиатура:** при смене раскладки на мгновение появляется «Русский · RU»; Caps Lock показывает Вкл/Выкл.
- **Сочетание клавиш:** задайте в настройках сочетание, которое открывает и сворачивает островок без мыши.
- **Аудиовыход:** выбор динамиков или наушников на вкладке «Главная» (если устройств больше одного).
- **Сеть:** потеря и возврат интернета, имя Wi-Fi, подключение и отключение VPN.
- **Снимки экрана:** после Win+Shift+S в островке появляется миниатюра с кнопками «Сохранить» и «На полку» (если «Ножницы» уже сохранили файл, используется он).
- **Полка для файлов:** перетащите файлы на островок, чтобы держать их под рукой, а потом перетащите их в любое приложение. Правый клик по файлу копирует путь; старые файлы могут сами убираться через 1, 7 или 30 дней.
- **Зеркало:** быстро взглянуть на себя через веб-камеру перед звонком.
- **Открытые приложения:** значки открытых окон; по клику окно выводится на передний план.
- **Claude Code:** включите модуль Claude, и остров добавит небольшой хук в настройки Claude Code (уберёт его, когда модуль выключат или программу удалят). Пилюля показывает, пока сессия работает, становится оранжевой, когда Claude ждёт вашего разрешения или ответа, а в конце появляется зелёное «Claude закончил»; по клику это окно выводится на передний план. Работает в терминале и во вкладке Code в Claude Desktop.
- **Загрузки:** пока Chrome, Edge или Firefox скачивает файл, в пилюле крутится кольцо с размером и скоростью; в конце появляется файл с кнопками «Открыть» и «Показать в папке», его можно перетащить в любое приложение.
- **Разблокировка:** после разблокировки Windows PIN-кодом или Windows Hello на острове открывается маленький замок.
- **Вкладка «Система»:** загрузка процессора, памяти и видеокарты, температура видеокарты (NVIDIA, только когда она уже работает), текущая скорость загрузки/отдачи и пинг до 1.1.1.1. Датчики читаются только пока вкладка открыта.
- **Буфер обмена** (по умолчанию выключен): последние 5 скопированных текстов, только в памяти; при каждом копировании на 2 с видны первые символы. Копии из менеджеров паролей пропускаются.
- **Таймер сна:** 15, 30 или 60 минут на вкладке «Таймер»; за последние 10 с громкость плавно стихает, музыка ставится на паузу, громкость возвращается.
- **Заметки:** быстрый список с галочками на своей вкладке и необязательное глобальное сочетание клавиш, чтобы сразу начать писать.
- **Настоящий визуализатор:** полоски в пилюле и рядом с обложкой двигаются в такт звуку из динамиков, в цвете обложки; в тишине или без звука они замирают.
- **Центр управления** на вкладке «Главная»: Wi-Fi, Bluetooth, тёмная тема, «Не беспокоить», микрофон, блокировка и сон (сон требует второго нажатия). Кнопка, которая не работает в вашей системе, не показывается.
- **Батареи Bluetooth:** заряд подключённых наушников, мыши или геймпада рядом с часами (красным ниже 20 %).
- **CI и деплои** (по умолчанию выключено): запуски GitHub Actions из ваших репозиториев (через `gh`) и сборки Cloudflare Pages / Workers (с токеном только для чтения). Пока идёт — крутится кольцо, в конце на 5 с зелёная ✓ или красная ✗; по клику открывается запуск.
- Модуль Claude показывает настоящий значок Claude, взятый из установленного приложения Claude Desktop.
- **Настройки:** включение и выключение каждого модуля, раскрытие по наведению или по клику, выбор монитора (или всех), размера и языка (Русский, English, Română; «Автоматически» — как в Windows).
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
