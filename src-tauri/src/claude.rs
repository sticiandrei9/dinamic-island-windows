//! Sesiunile Claude Code în insulă: urmărește folderul scris de `claude_hook` și emite
//! `claude-sessions` cu ce lucrează, ce așteaptă și ce tocmai a terminat.

use std::collections::HashSet;
use std::path::Path;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use windows::core::HSTRING;
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Storage::FileSystem::{
    FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification, FILE_NOTIFY_CHANGE_FILE_NAME,
    FILE_NOTIFY_CHANGE_LAST_WRITE,
};
use windows::Win32::System::Threading::{GetExitCodeProcess, OpenProcess, WaitForSingleObject, PROCESS_QUERY_LIMITED_INFORMATION};

use crate::claude_hook::{self, HookStatus, SessionFile};
use crate::settings::SettingsState;

/// Fără pid, o sesiune fără niciun eveniment atâta timp se consideră moartă.
const NO_PID_MAX_MS: u64 = 30 * 60_000;
/// `working` fără niciun eveniment (nici PostToolUse) atâta timp: probabil întreruptă, se ascunde (nu se șterge).
const WORKING_IDLE_MS: u64 = 15 * 60_000;
/// Fișierele mai vechi de atât se șterg oricum.
const DELETE_AFTER_MS: u64 = 24 * 3_600_000;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Session {
    pub id: String,
    pub state: String,
    pub project: String,
    pub hwnd: isize,
    pub since: u64,
}

/// Ultima listă emisă (fără `done`), pentru interfața care se încarcă după eveniment.
static LAST: Mutex<Vec<Session>> = Mutex::new(Vec::new());
/// Ultima instalare eșuată a hook-ului; rămâne vizibilă în Setări chiar dacă comutatorul s-a întors pe oprit.
static FAILED: Mutex<Option<HookStatus>> = Mutex::new(None);

// ---------- logica pură ----------

fn project_name(cwd: &str) -> String {
    let name = Path::new(cwd).file_name().and_then(|n| n.to_str()).unwrap_or(cwd);
    if name.is_empty() { "Claude".into() } else { name.into() }
}

/// Sesiunile care se mai arată (`working`/`waiting`/`done`, cu proces viu sau, fără pid, recente)
/// și id-urile ale căror fișiere se șterg (proces mort sau mai vechi de 24 h).
pub fn live_sessions(files: Vec<(String, SessionFile)>, now: u64, alive: impl Fn(u32) -> bool) -> (Vec<Session>, Vec<String>) {
    let (mut shown, mut stale) = (Vec::new(), Vec::new());
    for (id, f) in files {
        let age = now.saturating_sub(f.at);
        if age > DELETE_AFTER_MS || (f.pid != 0 && !alive(f.pid)) {
            stale.push(id);
            continue;
        }
        let known = matches!(f.state.as_str(), "working" | "waiting" | "done");
        let fresh = if f.state == "working" { age <= WORKING_IDLE_MS } else { f.pid != 0 || age <= NO_PID_MAX_MS };
        if known && fresh {
            shown.push(Session { project: project_name(&f.cwd), id, state: f.state, hwnd: f.hwnd, since: f.at });
        }
    }
    (shown, stale)
}

/// `done` ajunge la interfață o singură dată (ca să vadă tranziția), apoi sesiunea dispare din listă.
fn emit_list(live: Vec<Session>, announced: &mut HashSet<(String, u64)>) -> Vec<Session> {
    announced.retain(|(id, at)| live.iter().any(|s| &s.id == id && s.since == *at && s.state == "done"));
    live.into_iter().filter(|s| s.state != "done" || announced.insert((s.id.clone(), s.since))).collect()
}

/// Build de dezvoltare: hook-ul ar indica spre un exe temporar.
fn is_dev_exe(exe: &str) -> bool {
    exe.to_lowercase().contains(r"\.cargo-target\")
}

// ---------- Win32 ----------

fn pid_alive(pid: u32) -> bool {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else { return false };
        let mut code = 0u32;
        let ok = GetExitCodeProcess(h, &mut code).is_ok();
        let _ = CloseHandle(h);
        ok && code == 259 // STILL_ACTIVE
    }
}

/// Notificare la orice schimbare în folder; fără ea (eroare) thread-ul doar reverifică la 2 s.
struct Watch(HANDLE);

impl Watch {
    fn new(dir: &Path) -> Option<Self> {
        let path = HSTRING::from(dir.to_string_lossy().as_ref());
        unsafe { FindFirstChangeNotificationW(&path, false, FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_LAST_WRITE) }.ok().map(Watch)
    }

    /// Așteaptă o schimbare sau cel mult `ms`. După o schimbare mai așteaptă 100 ms (tmp + rename vin la rând),
    /// apoi rearmează, înainte de scanare, ca nicio schimbare să nu se piardă.
    /// `false` = mânerul nu mai merge (eroare): apelantul îl recreează, ca bucla să nu se rotească în gol.
    fn wait(&self, ms: u32) -> bool {
        unsafe {
            match WaitForSingleObject(self.0, ms) {
                WAIT_OBJECT_0 => {
                    std::thread::sleep(Duration::from_millis(100));
                    FindNextChangeNotification(self.0).is_ok()
                }
                WAIT_TIMEOUT => true,
                _ => false,
            }
        }
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        unsafe {
            let _ = FindCloseChangeNotification(self.0);
        }
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Doar `*.json`: scrierea atomică a hook-ului trece prin `<id>.json.tmp`.
fn read_sessions(dir: &Path) -> Vec<(String, SessionFile)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .filter_map(|e| {
            let p = e.path();
            if !p.extension().is_some_and(|x| x == "json") {
                return None;
            }
            let id = p.file_stem()?.to_str()?.to_string();
            let f = serde_json::from_slice(&std::fs::read(&p).ok()?).ok()?;
            Some((id, f))
        })
        .collect()
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let dir = claude_hook::sessions_dir();
        let mut announced = HashSet::new();
        let mut last: Option<Vec<Session>> = None; // None = modulul era oprit
        let mut watch: Option<Watch> = None;
        loop {
            if !app.state::<SettingsState>().get().modules.claude {
                watch = None;
                if last.take().is_some() {
                    LAST.lock().unwrap().clear();
                    let _ = app.emit("claude-sessions", Vec::<Session>::new());
                }
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
            let _ = std::fs::create_dir_all(&dir);
            if watch.is_none() {
                watch = Watch::new(&dir);
            }
            let (live, stale) = live_sessions(read_sessions(&dir), now_ms(), pid_alive);
            for id in stale {
                let _ = std::fs::remove_file(dir.join(format!("{id}.json")));
            }
            let list = emit_list(live, &mut announced);
            if last.as_ref() != Some(&list) {
                *LAST.lock().unwrap() = list.iter().filter(|s| s.state != "done").cloned().collect();
                let _ = app.emit("claude-sessions", &list);
                last = Some(list);
            }
            // mâner stricat sau lipsă: îl recreăm la următoarea tură (cel vechi se închide prin Drop)
            if !watch.as_ref().is_some_and(|w| w.wait(2000)) {
                watch = None;
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    });
}

// ---------- hook-ul în ~/.claude/settings.json ----------

/// Instalează hook-ul pentru exe-ul curent; reține eșecul pentru Setări.
pub fn install_hook() -> HookStatus {
    let st = match std::env::current_exe() {
        Err(e) => HookStatus::Invalid(e.to_string()),
        Ok(p) => {
            let exe = p.to_string_lossy().into_owned();
            if is_dev_exe(&exe) {
                HookStatus::Dev
            } else {
                claude_hook::install(&exe).map_or_else(HookStatus::Invalid, |_| HookStatus::Installed)
            }
        }
    };
    *FAILED.lock().unwrap() = (st != HookStatus::Installed).then(|| st.clone());
    st
}

pub fn remove_hook() {
    if let Err(e) = claude_hook::uninstall() {
        log::warn!("hook-ul Claude nu s-a putut scoate: {e}");
    }
    *FAILED.lock().unwrap() = None;
}

// ---------- comenzi ----------

#[tauri::command]
pub fn claude_sessions() -> Vec<Session> {
    LAST.lock().unwrap().clone()
}

/// Aduce în față fereastra sesiunii; hwnd-ul vine din fișierul ei (id-ul din interfață nu devine cale).
#[tauri::command]
pub async fn claude_focus(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let name = claude_hook::session_file_name(&id).ok_or("id de sesiune invalid")?;
        let bytes = std::fs::read(claude_hook::sessions_dir().join(name)).map_err(|_| "sesiunea s-a încheiat".to_string())?;
        let f: SessionFile = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if f.hwnd == 0 {
            return Err("sesiunea nu are fereastră".into());
        }
        crate::windows_list::focus_hwnd(f.hwnd)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn claude_hook_status(state: tauri::State<SettingsState>) -> HookStatus {
    if let Some(s) = FAILED.lock().unwrap().clone() {
        return s;
    }
    if !state.get().modules.claude {
        return HookStatus::Missing; // oprit: Setările nu arată nimic
    }
    match claude_hook::status() {
        HookStatus::Missing if std::env::current_exe().is_ok_and(|p| is_dev_exe(&p.to_string_lossy())) => HookStatus::Dev,
        s => s,
    }
}

#[tauri::command]
pub fn claude_hook_install() -> Result<(), String> {
    match install_hook() {
        HookStatus::Installed => Ok(()),
        HookStatus::Invalid(e) => Err(e),
        _ => Err("build de dezvoltare".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(state: &str, pid: u32, at: u64) -> SessionFile {
        SessionFile { state: state.into(), cwd: r"C:\a\proiect".into(), hwnd: 1, pid, at }
    }

    #[test]
    fn sesiuni_moarte_si_vechi() {
        let f = file;
        let now = 100 * 3_600_000;
        let (vis, del) = live_sessions(
            vec![
                ("viu".into(), f("working", 10, now - 60_000)),
                ("mort".into(), f("working", 11, now - 60_000)),
                ("fara_pid".into(), f("waiting", 0, now - 31 * 60_000)),
                ("vechi".into(), f("done", 10, now - 25 * 3_600_000)),
            ],
            now,
            |pid| pid == 10,
        );
        assert_eq!(vis.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["viu"]);
        assert_eq!(vis[0].project, "proiect");
        assert!(del.contains(&"mort".to_string()) && del.contains(&"vechi".to_string()));
    }

    #[test]
    fn lucreaza_vechi_se_ascunde_dar_asteptarea_ramane() {
        let now = 100 * 3_600_000;
        let (vis, del) = live_sessions(
            vec![
                ("lucreaza_vechi".into(), file("working", 10, now - 16 * 60_000)),
                ("lucreaza".into(), file("working", 10, now - 14 * 60_000)),
                ("asteapta".into(), file("waiting", 10, now - 5 * 3_600_000)),
            ],
            now,
            |_| true,
        );
        assert_eq!(vis.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), ["lucreaza", "asteapta"]);
        assert!(del.is_empty()); // ascunsă, nu ștearsă: următorul eveniment o readuce
    }

    #[test]
    fn citeste_doar_json_nu_si_tmp() {
        let d = std::env::temp_dir().join(format!("di-claude-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        let body = serde_json::to_vec(&file("working", 10, 5)).unwrap();
        std::fs::write(d.join("a.json"), &body).unwrap();
        std::fs::write(d.join("x.json.tmp"), &body).unwrap();
        let got = read_sessions(&d);
        std::fs::remove_dir_all(&d).unwrap();
        assert_eq!(got.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["a"]);
    }

    fn sess(id: &str, state: &str, since: u64) -> Session {
        Session { id: id.into(), state: state.into(), project: "p".into(), hwnd: 1, since }
    }

    #[test]
    fn done_ajunge_la_interfata_o_singura_data() {
        let mut seen = HashSet::new();
        let live = || vec![sess("a", "done", 5), sess("b", "working", 6)];
        let ids = |v: Vec<Session>| v.into_iter().map(|s| s.id).collect::<Vec<_>>();
        assert_eq!(ids(emit_list(live(), &mut seen)), ["a", "b"]);
        assert_eq!(ids(emit_list(live(), &mut seen)), ["b"]);
        // aceeași sesiune, o nouă tură terminată → se anunță din nou
        assert_eq!(ids(emit_list(vec![sess("a", "working", 7)], &mut seen)), ["a"]);
        assert_eq!(ids(emit_list(vec![sess("a", "done", 9)], &mut seen)), ["a"]);
    }

    #[test]
    fn build_de_dezvoltare_recunoscut_dupa_cale() {
        assert!(is_dev_exe(r"C:\Users\Andre\.cargo-target\dynamic-island\debug\dynamic-island.exe"));
        assert!(is_dev_exe(r"c:\users\andre\.CARGO-TARGET\x\y.exe"));
        assert!(!is_dev_exe(r"C:\Users\Andre\AppData\Local\Dynamic Island\dynamic-island.exe"));
    }
}
