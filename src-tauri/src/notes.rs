use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

/// O notiță din listă; `at` = secunde Unix la creare.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Note {
    pub id: u64,
    pub text: String,
    pub done: bool,
    pub at: u64,
}

const MAX_CHARS: usize = 500;

/// Adaugă o notiță: textul se taie la capete; gol sau peste 500 de caractere = eroare.
pub fn add(mut list: Vec<Note>, text: &str, id: u64, at: u64) -> Result<Vec<Note>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("notița e goală".into());
    }
    if text.chars().count() > MAX_CHARS {
        return Err(format!("notița are peste {MAX_CHARS} de caractere"));
    }
    list.push(Note { id, text: text.to_string(), done: false, at });
    Ok(list)
}

pub fn toggle(mut list: Vec<Note>, id: u64) -> Vec<Note> {
    if let Some(n) = list.iter_mut().find(|n| n.id == id) {
        n.done = !n.done;
    }
    list
}

pub fn remove(list: Vec<Note>, id: u64) -> Vec<Note> {
    list.into_iter().filter(|n| n.id != id).collect()
}

pub fn clear_done(list: Vec<Note>) -> Vec<Note> {
    list.into_iter().filter(|n| !n.done).collect()
}

pub fn next_id(list: &[Note]) -> u64 {
    list.iter().map(|n| n.id).max().unwrap_or(0) + 1
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum LoadNote {
    Loaded,
    Missing,
    /// JSON stricat: fișierul a fost mutat în `notes.corrupt.json`, nu rămâne sub numele în care se scrie
    Corrupt,
    /// citire imposibilă (blocat de OneDrive / antivirus): nu scriem peste el
    Unreadable,
}

/// Mută un `notes.json` stricat lângă el, fără să calce un fișier stricat mai vechi. False dacă mutarea eșuează.
fn set_aside(path: &Path, now: u64) -> bool {
    let mut target = path.with_file_name("notes.corrupt.json");
    if target.exists() {
        target = path.with_file_name(format!("notes.corrupt-{now}.json"));
    }
    match std::fs::rename(path, &target) {
        Ok(()) => {
            log::warn!("notes.json corupt; mutat în {}", target.display());
            true
        }
        Err(e) => {
            log::warn!("notes.json corupt și nu poate fi mutat ({e}); nu scriu peste el");
            false
        }
    }
}

pub fn load(path: &Path) -> (Vec<Note>, LoadNote) {
    load_at(path, now_secs())
}

/// Citește lista; niciodată nu pierde în tăcere un fișier existent (vezi `LoadNote`). `Corrupt` doar dacă
/// fișierul stricat a fost mutat; dacă mutarea eșuează (blocat), e `Unreadable` și scrierile rămân blocate.
fn load_at(path: &Path, now: u64) -> (Vec<Note>, LoadNote) {
    match std::fs::read_to_string(path) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(list) => (list, LoadNote::Loaded),
            Err(_) if set_aside(path, now) => (Vec::new(), LoadNote::Corrupt),
            Err(_) => (Vec::new(), LoadNote::Unreadable),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), LoadNote::Missing),
        Err(_) => (Vec::new(), LoadNote::Unreadable),
    }
}

/// Scriere atomică: fișier temporar, apoi redenumire peste cel vechi.
pub fn save(path: &Path, list: &[Note]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(list).map_err(std::io::Error::other)?)?;
    std::fs::rename(tmp, path)
}

pub struct NotesState {
    items: Mutex<Vec<Note>>,
    path: PathBuf,
    /// fișierul exista, dar n-a putut fi citit: nu scriem până nu se citește cu succes
    blocked: AtomicBool,
}

impl NotesState {
    pub fn load(path: PathBuf) -> Self {
        let (mut items, mut note) = load(&path);
        if note == LoadNote::Unreadable {
            std::thread::sleep(std::time::Duration::from_millis(250)); // blocaje scurte (sincronizare, antivirus)
            (items, note) = load(&path);
        }
        match note {
            LoadNote::Unreadable => log::warn!("notes.json nu poate fi citit; nu scriu peste el până nu se poate citi"),
            _ => {}
        }
        Self { items: Mutex::new(items), path, blocked: AtomicBool::new(note == LoadNote::Unreadable) }
    }

    /// Cât fișierul e blocat, încercăm din nou citirea; true dacă putem lucra.
    fn unblock(&self, items: &mut Vec<Note>) -> bool {
        if !self.blocked.load(Ordering::Relaxed) {
            return true;
        }
        let (list, note) = load(&self.path);
        if note == LoadNote::Unreadable {
            return false;
        }
        *items = list;
        self.blocked.store(false, Ordering::Relaxed);
        true
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Aplică o schimbare, o salvează și anunță TOATE insulele (fiecare monitor are fereastra lui).
fn update(app: &AppHandle, f: impl FnOnce(Vec<Note>) -> Result<Vec<Note>, String>) -> Result<(), String> {
    let state = app.state::<NotesState>();
    let mut items = state.items.lock().unwrap();
    if !state.unblock(&mut items) {
        return Err("notes.json nu poate fi citit".into());
    }
    // lista rămâne neatinsă (în memorie și pe ecran) dacă `f` refuză (notiță goală / prea lungă) sau salvarea eșuează
    let next = f(items.clone())?;
    save(&state.path, &next).map_err(|e| e.to_string())?;
    *items = next.clone();
    drop(items);
    let _ = app.emit("notes-changed", next);
    Ok(())
}

#[tauri::command]
pub fn notes_list(state: tauri::State<NotesState>) -> Vec<Note> {
    let mut items = state.items.lock().unwrap();
    state.unblock(&mut items);
    items.clone()
}

#[tauri::command]
pub fn notes_add(app: AppHandle, text: String) -> Result<(), String> {
    update(&app, |l| {
        let id = next_id(&l);
        add(l, &text, id, now_secs())
    })
}

#[tauri::command]
pub fn notes_toggle(app: AppHandle, id: u64) -> Result<(), String> {
    update(&app, |l| Ok(toggle(l, id)))
}

#[tauri::command]
pub fn notes_remove(app: AppHandle, id: u64) -> Result<(), String> {
    update(&app, |l| Ok(remove(l, id)))
}

#[tauri::command]
pub fn notes_clear_done(app: AppHandle) -> Result<(), String> {
    update(&app, |l| Ok(clear_done(l)))
}

// ---------------- focusul câmpului de scris ----------------

/// Ce are de făcut `notes_focus(false)`.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum FocusStep {
    Nothing,
    RestoreStyle,
    RestoreStyleAndFocus(isize),
}

/// `had_focus`: am activat insula pentru scris și n-am predat-o încă; `prev`: fereastra din prim-plan de
/// dinainte; `prev_alive`: fereastra aceea există și insula e încă în prim-plan (altfel utilizatorul a
/// ales deja altă fereastră și nu i-o luăm).
pub fn focus_restore_plan(had_focus: bool, prev: Option<isize>, prev_alive: bool) -> FocusStep {
    match (had_focus, prev) {
        (false, _) => FocusStep::Nothing,
        (true, Some(p)) if prev_alive => FocusStep::RestoreStyleAndFocus(p),
        (true, _) => FocusStep::RestoreStyle,
    }
}

#[derive(Default)]
struct FocusState {
    active: bool,
    prev: Option<isize>,
    /// insula activată (cu mai multe monitoare, doar ea poate preda focusul)
    label: String,
}

static FOCUS: Mutex<FocusState> = Mutex::new(FocusState { active: false, prev: None, label: String::new() });

/// Activează insula pentru scris (`on`) sau o face la loc nefocusabilă și redă focusul. Win32 doar pe
/// thread-ul principal, care deține fereastra (`SetFocus` merge doar pe thread-ul ferestrei).
#[tauri::command]
pub fn notes_focus(app: AppHandle, window: WebviewWindow, on: bool) -> Result<(), String> {
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let mut st = FOCUS.lock().unwrap();
        if on {
            if st.active && st.label != window.label() {
                // câmpul altei insule (alt monitor): cea veche își pierde activarea
                if let Some(old) = handle.get_webview_window(&st.label) {
                    crate::platform::set_no_activate(&old, true);
                }
            }
            let prev = crate::platform::activate_for_typing(&window);
            // dacă era deja activă (focus + island-notes, sau altă insulă), "prim-planul de dinainte" e deja ținut minte
            st.prev = if st.active { st.prev } else { prev };
            st.active = true;
            st.label = window.label().to_string();
        } else if st.active && st.label != window.label() {
            // altă insulă (alt monitor) își ascunde fila Notițe, dar focusul e al celei activate: nu e treaba ei
        } else {
            let alive = st.prev.is_some_and(|p| crate::platform::can_refocus(p, &window));
            match focus_restore_plan(st.active, st.prev, alive) {
                FocusStep::Nothing => {}
                FocusStep::RestoreStyle => {
                    let still_ours = crate::platform::is_foreground(&window);
                    crate::platform::set_no_activate(&window, true);
                    // fără fereastră de dinainte, insula nu păstrează tastatura (scrisul ar ajunge în ea): o dăm desktopului;
                    // dacă utilizatorul a ales deja altă fereastră, n-o atingem
                    if still_ours {
                        crate::platform::focus_shell();
                    }
                }
                FocusStep::RestoreStyleAndFocus(p) => {
                    crate::platform::set_no_activate(&window, true);
                    crate::platform::focus_previous(p);
                }
            }
            *st = FocusState::default();
        }
    })
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("di-notes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn notite_adaugate_bifate_sterse() {
        let l = add(vec![], "  lapte  ", 1, 10).unwrap();
        assert_eq!(l[0].text, "lapte");
        assert!(add(l.clone(), "   ", 2, 11).is_err());
        let l = toggle(l, 1);
        assert!(l[0].done);
        assert!(clear_done(l).is_empty());
    }

    #[test]
    fn toggle_si_remove_dupa_id() {
        let l = add(add(vec![], "a", 1, 1).unwrap(), "b", 2, 2).unwrap();
        let l = toggle(l, 2);
        assert!(!l[0].done && l[1].done);
        let l = toggle(l, 2); // a doua oară revine
        assert!(!l[1].done);
        assert_eq!(remove(l.clone(), 1).len(), 1);
        assert_eq!(remove(l, 99).len(), 2); // id inexistent: nimic de șters
    }

    #[test]
    fn clear_done_pastreaza_nebifatele() {
        let l = add(add(vec![], "a", 1, 1).unwrap(), "b", 2, 2).unwrap();
        let l = clear_done(toggle(l, 1));
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].id, 2);
    }

    #[test]
    fn lungime_maxima_500_de_caractere() {
        assert!(add(vec![], &"ă".repeat(500), 1, 1).is_ok()); // se numără caracterele, nu octeții
        assert!(add(vec![], &"ă".repeat(501), 1, 1).is_err());
        // spațiile de la capete nu intră în număr
        assert!(add(vec![], &format!("  {}  ", "a".repeat(500)), 1, 1).is_ok());
    }

    #[test]
    fn id_urmator_e_maximul_plus_unu() {
        assert_eq!(next_id(&[]), 1);
        let l = add(add(vec![], "a", 5, 1).unwrap(), "b", 9, 2).unwrap();
        assert_eq!(next_id(&remove(l, 5)), 10);
    }

    #[test]
    fn salvare_atomica_si_recitire() {
        let d = temp_dir("salvare");
        let p = d.join("notes.json");
        let l = add(vec![], "lapte", 1, 10).unwrap();
        save(&p, &l).unwrap();
        save(&p, &toggle(l.clone(), 1)).unwrap();
        let (back, note) = load(&p);
        assert_eq!(note, LoadNote::Loaded);
        assert!(back[0].done && back[0].text == "lapte" && back[0].at == 10);
        assert!(!d.join("notes.json.tmp").exists());
        let (empty, note) = load(&d.join("lipsa.json"));
        assert!(empty.is_empty() && note == LoadNote::Missing);
    }

    #[test]
    fn fisier_stricat_mutat_nu_suprascris() {
        let d = temp_dir("stricat");
        let p = d.join("notes.json");
        std::fs::write(&p, "{ nu e json").unwrap();
        let (l, note) = load(&p);
        assert!(l.is_empty() && note == LoadNote::Corrupt);
        assert!(!p.exists());
        assert_eq!(std::fs::read_to_string(d.join("notes.corrupt.json")).unwrap(), "{ nu e json");
        // al doilea fișier stricat nu-l calcă pe primul
        std::fs::write(&p, "alt gunoi").unwrap();
        assert_eq!(load(&p).1, LoadNote::Corrupt);
        assert_eq!(std::fs::read_to_string(d.join("notes.corrupt.json")).unwrap(), "{ nu e json");
        let altul = std::fs::read_dir(&d).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().starts_with("notes.corrupt-")).count();
        assert_eq!(altul, 1);
    }

    #[test]
    fn mutare_esuata_ramane_blocata_nu_corupta() {
        let d = temp_dir("mutare-esuata");
        let p = d.join("notes.json");
        std::fs::write(&p, "{ nu e json").unwrap();
        // ambele nume posibile ocupate de directoare nevide: redenumirea eșuează determinist
        for name in ["notes.corrupt.json", "notes.corrupt-1000.json"] {
            std::fs::create_dir(d.join(name)).unwrap();
            std::fs::write(d.join(name).join("x"), "x").unwrap();
        }
        let (l, note) = load_at(&p, 1000);
        assert!(l.is_empty());
        assert_eq!(note, LoadNote::Unreadable); // nu Corrupt: altfel următoarea salvare ar suprascrie fișierul stricat
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{ nu e json"); // neatins
    }

    #[test]
    fn citire_imposibila_nu_e_lipsa() {
        let d = temp_dir("nelizibil");
        // un director în locul fișierului: citirea eșuează cu altceva decât NotFound
        let p = d.join("notes.json");
        std::fs::create_dir(&p).unwrap();
        let (l, note) = load(&p);
        assert!(l.is_empty() && note == LoadNote::Unreadable);
        assert!(p.is_dir()); // neatins
    }

    #[test]
    fn plan_de_restaurare_a_focusului() {
        assert_eq!(focus_restore_plan(false, None, false), FocusStep::Nothing);
        assert_eq!(focus_restore_plan(true, Some(42), true), FocusStep::RestoreStyleAndFocus(42));
        assert_eq!(focus_restore_plan(true, Some(42), false), FocusStep::RestoreStyle);
        assert_eq!(focus_restore_plan(true, None, true), FocusStep::RestoreStyle);
    }
}
