# v0.2.0 · B. Activități — Specificație de design

Data: 2026-10-14 · Parte din `2026-10-14-v020-overview.md` · Depinde de: D (setări, log)

## 1. Cadrul de activități (ca Live Activities)

O **activitate** e orice informație pe care insula o arată în starea compactă. Fiecare activitate are:
```
{ id, kind: "media"|"timer"|"privacy"|"alert"|"volume"|"notification"|"update",
  priority, transient: bool, expires_at?, compact: {left, right}, bubble? }
```

- **Activitățile tranzitorii** (alertă, volum, notificare, anunț de update) apar peste orice, pentru o durată fixă (volum 1,5 s, alertă 3 s, notificare 5 s, update 4 s). Insula se umflă cu spring, ca pe iPhone, apoi revine. Dacă apar mai multe, se afișează pe rând, cu excepția volumului, care se actualizează pe loc.
- **Activitățile persistente** (media, timer, confidențialitate) concurează pentru pastilă după prioritate: timer care rulează > media > confidențialitate.
  - Când sunt cel puțin două persistente, cea mai importantă ocupă pastila, iar a doua apare ca **bulă**: un cerc negru separat, de 32 px, la 8 px în dreapta pastilei. Bula apare cu spring, „desprinzându-se” din pastilă, exact ca pe iPhone.
  - Indicatorul de confidențialitate nu ocupă niciodată pastila sau bula: e un punct colorat în colțul din dreapta al pastilei (vezi §4).
- Funcția pură `layout(activities, now) -> { pill: Activity|null, bubble: Activity|null, transient: Activity|null }` decide afișarea și e testată în JS (`src/lib/activities.js`).
- Click pe bulă extinde insula direct pe fila activității din bulă.
- Hit-test: dreptunghiul raportat către Rust include și bula.

## 2. File în insula extinsă

Insula extinsă primește un rând de file sus, cu pictograme mici de 22 px, gri, iar fila activă e albă (ca la NotchNook):
`Acasă` (media + aplicații, ca acum) · `Timer` · `Raft` (partea C) · `Oglindă` (partea C).

- Filele modulelor oprite din setări nu apar.
- Între file, conținutul trece prin crossfade cu blur, iar înălțimea insulei se adaptează cu spring.
- La extindere, se deschide fila activității din pastilă (timer → Timer, altfel Acasă). Ultima filă aleasă e ținută minte doar cât e extinsă.

## 3. Timer și cronometru

- **Fila Timer:**
  - presetări 1 / 5 / 10 / 25 min, plus o roată de minute (−/+ cu pasul de 1 min, sau rotița mouse-ului);
  - butoanele Start / Pauză / Anulează;
  - sub ele, un cronometru cu Start / Pauză / Tur / Resetare și lista ultimelor 5 tururi.
- Logica stă în JS (`src/lib/timer.js`, funcții pure pe timestamp-uri, testate). Starea e salvată în `localStorage`, ca un timer să supraviețuiască unei reîncărcări a paginii, dar nu și repornirii aplicației.
- **Pastila**, cât rulează timerul: în stânga o pictogramă de clepsidră portocalie, în dreapta timpul rămas (`4:59`, cifre tabulare, portocaliu `#ff9f0a`). Cronometrul apare la fel, cu verde.
- **La final:** activitate tranzitorie de alertă „Timer terminat” + sunetul de sistem (`MessageBeep`). Pastila pulsează portocaliu până la un click sau timp de 30 s.
- Cu mai multe ferestre-insulă, timerul trăiește în Rust (`timer.rs`, un singur ceas, emite `timer-state`), ca să fie același peste tot. JS doar afișează. Funcțiile pure din `timer.js` sunt folosite pentru afișare.

## 4. Indicator microfon și cameră (confidențialitate)

- **Sursa:** registrul `HKCU\Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\{microphone,webcam}`, citit la fiecare 1 s.
  - Se citesc subcheile pentru aplicațiile din Store și `NonPackaged\<cale exe cu # în loc de \>` pentru celelalte.
  - O aplicație folosește dispozitivul acum dacă `LastUsedTimeStart > 0` și `LastUsedTimeStop == 0`.
  - Dynamic Island însăși e exclusă (folosește camera pentru oglindă).
- Funcția pură `in_use(entries) -> Vec<String>` e testată.
- **Afișare:**
  - un punct de 6 px în dreapta pastilei: **portocaliu** `#ff9f0a` pentru microfon, **verde** `#30d158` pentru cameră, ambele dacă sunt ambele (ca pe iPhone);
  - în insula extinsă, fila Acasă primește un rând „Microfon: Discord · Cameră: Teams”.

## 5. Alerte de sistem

Toate sunt activități tranzitorii, care se umflă cu animație:

| Alertă | Sursă | Afișare |
|---|---|---|
| Încărcător conectat / deconectat | `GetSystemPowerStatus` la 2 s (doar dacă există baterie) | pictogramă de fulger + „Se încarcă · 64%” (verde) / „Pe baterie · 64%” |
| Baterie scăzută | același; prag 20% și 10%, o singură dată pe descărcare | pictogramă de baterie roșie + „Baterie scăzută · 10%” |
| Dispozitiv Bluetooth conectat / deconectat | `DeviceInformation.CreateWatcher` WinRT, cu AQS pentru dispozitive Bluetooth conectate; bateria din proprietatea `{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2` dacă există | pictogramă după tipul dispozitivului (căști / boxă / mouse / tastatură) + nume + baterie („AirPods Pro · 80%”), cu o mică bară circulară de baterie în dreapta, ca pe iPhone |

- Funcțiile pure `power_transition(prev, cur) -> Option<Alert>` și `bt_transition(prev_set, cur_set) -> Vec<Alert>` sunt testate.
- Laptop fără baterie sau desktop: alertele de încărcare nu apar.

## 6. Volum și luminozitate

- **Volumul:** `IAudioEndpointVolume::RegisterControlChangeNotify` pe dispozitivul implicit de redare. Dispozitivul e reînregistrat la schimbarea celui implicit (`IMMNotificationClient`). Fiecare schimbare emite `volume` `{level, muted}`.
- **Luminozitatea:** doar pentru ecranul intern al laptopului, prin WMI `WmiMonitorBrightness` citit la 500 ms, doar când există (monitoarele externe nu sunt suportate).
- **Afișare:** activitate tranzitorie de 1,5 s, cu pastila lățită. În stânga o pictogramă de difuzor (cu 0–3 unde după nivel, tăiată la mute) sau de soare, în dreapta o bară de 120 px care se animă cu spring și procentul.
- **Ascunderea pop-up-ului Windows:**
  - se caută fereastra OSD a sistemului (clasa `NativeHWNDHost` cu un copil `DirectUIHWND`, metoda folosită de aplicații ca ModernFlyouts / HideVolumeOSD) și se minimizează la pornire;
  - dacă nu e găsită (versiuni de Windows 11 care desenează OSD-ul altfel), se scrie în log „OSD-ul Windows nu poate fi ascuns” și se afișează ambele.
  - Când modulul e oprit sau aplicația se închide, fereastra OSD e restaurată.

## 7. Notificări Windows (varianta limitată)

- **Sursa:** UI Automation (`IUIAutomation::AddAutomationEventHandler` pe `UIA_Window_WindowOpenedEventId`).
  - Se filtrează ferestrele de toast: fereastra cu clasa `Windows.UI.Core.CoreWindow` din procesul `ShellExperienceHost.exe`, cu un element-text identificabil ca toast.
  - Se citesc textele descendenților: numele aplicației, titlul, corpul.
- Funcția pură `parse_toast(texts: Vec<String>) -> Option<Toast>` e testată pe exemple înregistrate.
- **Afișare:** activitate tranzitorie de 5 s, cu insula extinsă la ~380×80. Conține pictograma aplicației (dacă se poate găsi exe-ul), numele aplicației gri, titlul alb și corpul pe un rând, cu ellipsis.
- Click pe notificare: insula se strânge. Nu deschide aplicația, pentru că nu avem acces la acțiunea toast-ului.
- **Limitări, documentate în setări:**
  - notificările care apar cât e activ „Nu deranja” sau în fullscreen nu sunt prinse (Windows nu le afișează);
  - unele aplicații pot fi ratate;
  - pop-up-ul Windows rămâne vizibil, insula doar îl dublează.

## 8. Testare

- **JS:** `layout` (prioritate, bulă, tranzitorii, expirare), `timer.js` (pauză/reluare, tururi, final).
- **Rust:** `in_use`, `power_transition`, `bt_transition`, `parse_toast`, maparea nivel → număr de unde.
- **Manual:**
  - timer + muzică (bula);
  - Discord sau Recorder pentru microfon, aplicația Cameră pentru cameră;
  - căști Bluetooth conectate/deconectate;
  - schimbarea volumului de la tastatură (doar insula, fără pop-up Windows);
  - o notificare de test trimisă din PowerShell.
