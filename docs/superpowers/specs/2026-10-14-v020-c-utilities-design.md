# v0.2.0 · C. Utilitare — Specificație de design

Data: 2026-10-14 · Parte din `2026-10-14-v020-overview.md` · Depinde de: D, B (file)

## 1. Raft de fișiere (ca la NotchNook „Tray”)

### Punerea fișierelor pe raft
- Utilizatorul trage unul sau mai multe fișiere sau foldere din Explorer peste insulă.
- Hit-test-ul face fereastra „solidă” când cursorul e deasupra insulei, deci fereastra primește drag-ul.
- La `DragEnter` (evenimentul Tauri `onDragDropEvent`), insula se extinde imediat pe fila **Raft**, cu chenarul punctat animat și textul „Lasă aici”.
- La `Drop`, căile sunt adăugate pe raft. La `Leave`, insula se strânge după 400 ms.
- **Pe raft se țin referințe, nu copii.** Lista (cale, nume, dată adăugare) e salvată în `<app_config_dir>/shelf.json` și supraviețuiește repornirii. Maximum 30 de elemente; cele mai vechi ies primele. Duplicatele (aceeași cale) nu se adaugă de două ori.

### Fila Raft
- O grilă orizontală, cu scroll, de carduri de 64×76: iconița fișierului (prin extractorul existent de iconițe, cu cache după extensie), iar sub ea numele, pe 2 rânduri, cu ellipsis.
- Pe fiecare card:
  - **tragere afară**: pornește un drag nativ cu fișierul (`tauri-plugin-drag`, CF_HDROP), care poate fi lăsat în orice aplicație (Explorer, Discord, browser);
  - **click**: deschide fișierul cu aplicația implicită (`ShellExecuteW`);
  - **click dreapta**: un meniu mic, desenat în insulă, cu „Arată în Explorer” (`explorer /select,`) și „Scoate de pe raft”;
  - o mică „×” la hover, care scoate cardul de pe raft.
- Butonul „Golește raftul” e în colțul din dreapta-sus al filei.
- Fișierele care nu mai există la cale apar estompate, cu „Fișier mutat sau șters”. Click pe ele le scoate de pe raft.
- Când raftul are elemente, fila Raft arată numărul lor ca badge.

### Logica testată
Funcțiile pure `shelf_add(list, paths, now) -> list` (deduplicare, limită de 30, ordine) și `shelf_remove` sunt în Rust (`shelf.rs`).

## 2. Oglindă cameră

- Fila **Oglindă** conține un dreptunghi de 300×170 cu colțuri de 18 px, în care rulează camera web în timp real, în oglindă (`transform: scaleX(-1)`).
- Camera pornește doar când fila e deschisă (`getUserMedia({ video: { width: 640, height: 360 } })`) și se oprește (`track.stop()`, deci se stinge și ledul) când:
  - fila se schimbă;
  - insula se strânge;
  - au trecut 60 s fără activitate.
- **Permisiunea:** la prima utilizare, WebView2 cere permisiunea de cameră.
  - Dacă se poate, permisiunea e acordată automat doar pentru originea aplicației (`tauri://localhost`), prin handler-ul `PermissionRequested` al WebView2, accesat cu `with_webview`. Altfel apare prompt-ul WebView2, iar alegerea e ținută minte de WebView2.
  - Dacă accesul e refuzat sau nu există cameră, apare mesajul „Camera nu e disponibilă” cu pictograma tăiată.
- Mai multe camere: un buton mic în colț le comută (`enumerateDevices`). Alegerea e ținută minte în setări (`mirror_camera`).
- Dynamic Island e exclusă din indicatorul de cameră (B §4), ca oglinda să nu-și aprindă singură punctul verde.

## 3. Testare

- **Rust:** `shelf_add` / `shelf_remove` (deduplicare, limită, ordine), serializarea `shelf.json`.
- **Manual:**
  - drag cu 3 fișiere din Explorer pe insulă;
  - drag afară pe desktop și într-un chat Discord;
  - fișier șters de pe disc → card estompat;
  - repornirea aplicației → raftul rămâne;
  - oglinda pornește, ledul camerei se stinge la strângerea insulei, fără cameră → mesaj.
