# v0.2.0 · A. Media — Specificație de design

Data: 2026-10-14 · Parte din `2026-10-14-v020-overview.md` · Depinde de: D, B (file)

## 1. Bara de progres trasabilă

- Bara din insula extinsă devine interactivă:
  - la hover se îngroașă de la 5 px la 8 px, cu spring, ca în Control Center pe macOS;
  - la apăsare și tragere, poziția urmează mouse-ul, iar timpii se actualizează live;
  - la eliberare, se trimite `media_seek(seconds)`.
- În Rust, `media_seek` apelează `TryChangePlaybackPositionAsync(ticks)` pe sesiunea afișată (AUMID-ul din `LastMedia`), prin `await_op`, cu timeout. După apel se cere resincronizarea, ca la play/pauză.
- Dacă sesiunea nu permite seek (`GetPlaybackInfo().Controls().IsPlaybackPositionEnabled() == false`), bara rămâne doar de afișare, fără hover. `MediaInfo` primește câmpul `can_seek`.
- Funcția pură `seekFromPointer(x, trackLeft, trackWidth, duration) -> seconds` (limitată la [0, duration]) e testată.
- În timpul tragerii, actualizările `media-update` nu mută bara; după eliberare, poziția e preluată optimist.

## 2. Click pe piesă deschide aplicația sursă

- Click pe copertă sau pe titlu în insula extinsă (și pe copertă în pastila activă) → `media_open_source()`.
- **Găsirea ferestrei:**
  1. AUMID → exe: `Spotify.exe` → `spotify.exe`; `Chrome` → `chrome.exe`; `MSEdge` → `msedge.exe`; `308046B0AF4A39CB` → `firefox.exe`; un AUMID care se termină în `.exe` e luat ca atare. Pentru aplicațiile din Store (`Familie!App`), se compară AUMID-ul ferestrei (`SHGetPropertyStoreForWindow` → `PKEY_AppUserModel_ID`) sau familia pachetului procesului.
  2. Dintre ferestrele de aplicații (lista din `windows_list`), se aleg cele ale acelui exe. Are prioritate fereastra al cărei titlu conține titlul piesei, ca la YouTube, unde titlul tab-ului e titlul video-ului. Altfel se ia prima în ordinea Z.
  3. Fereastra aleasă e adusă în față cu `focus_window` (rulat pe un thread separat, ceea ce rezolvă și deficiența minoră „focus pe thread-ul principal”).
- Funcția pură `pick_source_window(aumid, title, windows) -> Option<hwnd>` e testată.
- Insula se strânge după click.
- **Limitare:** în browser se aduce fereastra în față, nu un tab anume. Dacă video-ul e în alt tab decât cel activ, se vede tab-ul activ.

## 3. Versuri sincronizate (LRCLIB)

- **Sursa:** `https://lrclib.net/api/get?artist_name=…&track_name=…&album_name=…&duration=…`.
  - Dacă nu găsește, se încearcă `/api/search?track_name=…&artist_name=…` și se ia primul rezultat cu durata la ±3 s.
  - Header-ul `User-Agent: dynamic-island-windows v<versiune> (https://github.com/sticiandrei9/dinamic-island-windows)`, cum cere LRCLIB.
  - Timeout de 8 s.
- Se cere doar la schimbarea piesei, doar dacă modulul `lyrics` e pornit și doar pentru sesiuni cu artist și durată (exclude, de exemplu, video-urile fără metadate).
- **Cache:** ultimele 50 de piese, în memorie (rezultatele „nu există” sunt și ele păstrate, ca să nu se ceară din nou).
- **Parsare:** funcția pură `parse_lrc(text) -> Vec<(seconds, line)>`, testată. Suportă `[mm:ss.xx]`, mai multe timestamp-uri pe linie, linii goale (pauze instrumentale, afișate ca „♪”) și ignoră etichetele `[ar:]`, `[ti:]`.
  Dacă există doar versuri nesincronizate (`plainLyrics`), nu se afișează nimic (spațiul e prea mic pentru text static lung).
- **Afișare în fila Acasă a insulei extinse,** sub rândul titlului:
  - rândul curent în alb, 14 px semibold, și rândul următor în gri, 12 px;
  - la schimbarea rândului, cel curent urcă și se estompează, iar următorul vine în locul lui (tranziție cu spring);
  - rândurile prea lungi se taie cu ellipsis;
  - insula extinsă crește cu ~44 px doar când există versuri pentru piesa curentă.
- Funcția pură `currentLine(lines, positionSeconds) -> index` e testată în JS.
- Rândul se calculează din poziția extrapolată (`currentPosition`), la 250 ms.
- **Fără rețea, piesă negăsită sau eroare:** nu se afișează nimic, iar insula are înălțimea obișnuită.

## 4. Testare

- **JS:** `seekFromPointer`, `currentLine`.
- **Rust:** `parse_lrc`, `pick_source_window`, maparea AUMID → exe, construirea URL-ului LRCLIB (cu caractere speciale și diacritice).
- **Manual:** seek în Spotify și YouTube; click pe piesă cu Spotify minimizat și cu YouTube într-o fereastră din spate; versuri pentru o piesă cunoscută, în Spotify; o piesă fără versuri.
