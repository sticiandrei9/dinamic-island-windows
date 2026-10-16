use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS};
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, PostThreadMessageW, MSG, WM_APP, WM_HOTKEY};

const MOD_ALT: u32 = 0x1;
const MOD_CONTROL: u32 = 0x2;
const MOD_SHIFT: u32 = 0x4;
const MOD_WIN: u32 = 0x8;
const MOD_NOREPEAT: u32 = 0x4000;

/// „Win+Shift+I” → (modificatori, cod de tastă). Cel puțin Ctrl, Alt sau Win: o tastă singură
/// sau doar cu Shift (o literă mare) ar fi furată din tot ce scrii.
pub fn parse(combo: &str) -> Option<(u32, u32)> {
    let (mut mods, mut vk) = (0, None);
    for part in combo.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "alt" => mods |= MOD_ALT,
            "shift" => mods |= MOD_SHIFT,
            "win" => mods |= MOD_WIN,
            "space" => vk = Some(0x20),
            k if k.len() == 1 && k.chars().all(|c| c.is_ascii_alphanumeric()) => vk = Some(k.to_ascii_uppercase().as_bytes()[0] as u32),
            k => vk = Some(0x6F + k.strip_prefix('f')?.parse::<u32>().ok().filter(|n| (1..=24).contains(n))?),
        }
    }
    if mods & (MOD_CONTROL | MOD_ALT | MOD_WIN) == 0 { None } else { Some((mods | MOD_NOREPEAT, vk?)) }
}

/// Id-urile scurtăturilor înregistrate pe thread-ul lor.
const ID_ISLAND: i32 = 1;
const ID_NOTES: i32 = 2;

struct Worker {
    thread: u32,
    tx: SyncSender<(i32, String, SyncSender<bool>)>,
}

static WORKER: OnceLock<Worker> = OnceLock::new();

/// Thread-ul care ține scurtăturile înregistrate (WM_HOTKEY ajunge doar la thread-ul care le-a înregistrat).
pub fn spawn(app: AppHandle) {
    let (tx, rx) = sync_channel::<(i32, String, SyncSender<bool>)>(4);
    let (ready_tx, ready_rx) = sync_channel(1);
    std::thread::spawn(move || {
        let _ = ready_tx.send(unsafe { GetCurrentThreadId() });
        let mut registered = [false; 3]; // după id (indicii 1 și 2)
        let mut msg = MSG::default();
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            if msg.message == WM_HOTKEY {
                match msg.wParam.0 as i32 {
                    ID_ISLAND => toggle(&app),
                    ID_NOTES => open_notes(&app),
                    _ => {}
                }
            } else if msg.message == WM_APP {
                while let Ok((id, combo, reply)) = rx.try_recv() {
                    let slot = &mut registered[id as usize];
                    if *slot {
                        let _ = unsafe { UnregisterHotKey(None, id) };
                    }
                    *slot = parse(&combo).is_some_and(|(m, vk)| unsafe { RegisterHotKey(None, id, HOT_KEY_MODIFIERS(m), vk) }.is_ok());
                    let _ = reply.send(*slot || combo.trim().is_empty());
                }
            }
        }
    });
    if let Ok(thread) = ready_rx.recv() {
        let _ = WORKER.set(Worker { thread, tx });
    }
}

/// Înregistrează scurtătura insulei (goală = niciuna). False dacă Windows o refuză (ocupată de alt program).
pub fn apply(combo: &str) -> bool {
    apply_id(ID_ISLAND, combo)
}

/// La fel, pentru scurtătura care deschide notițele.
pub fn apply_notes(combo: &str) -> bool {
    apply_id(ID_NOTES, combo)
}

fn apply_id(id: i32, combo: &str) -> bool {
    let Some(w) = WORKER.get() else { return false };
    let (reply_tx, reply_rx) = sync_channel(1);
    if w.tx.send((id, combo.to_string(), reply_tx)).is_err() {
        return false;
    }
    // thread-ul poate să nu aibă încă o coadă de mesaje: reîncercăm trezirea până răspunde
    for _ in 0..20 {
        let _ = unsafe { PostThreadMessageW(w.thread, WM_APP, None, None) };
        if let Ok(ok) = reply_rx.recv_timeout(Duration::from_millis(50)) {
            return ok;
        }
    }
    false
}

/// Ultima combinație refuzată de Windows pentru fiecare scurtătură (pentru Setări).
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct HotkeyErrors {
    pub island: Option<String>,
    pub notes: Option<String>,
}

static LAST_ERROR: Mutex<HotkeyErrors> = Mutex::new(HotkeyErrors { island: None, notes: None });

#[tauri::command]
pub fn hotkey_error() -> HotkeyErrors {
    LAST_ERROR.lock().unwrap().clone()
}

fn record(errors: &mut HotkeyErrors, id: i32, combo: &str, ok: bool) {
    let slot = if id == ID_NOTES { &mut errors.notes } else { &mut errors.island };
    *slot = (!ok).then(|| combo.to_string());
}

fn report(id: i32, combo: &str, ok: bool) {
    if !ok {
        log::warn!("scurtătura {combo} n-a putut fi înregistrată (ocupată de alt program?)");
    }
    record(&mut LAST_ERROR.lock().unwrap(), id, combo, ok);
}

pub fn apply_and_report(combo: &str) {
    report(ID_ISLAND, combo, apply(combo));
}

pub fn apply_notes_and_report(combo: &str) {
    report(ID_NOTES, combo, apply_notes(combo));
}

/// Insula de pe monitorul cu cursorul (sau prima). Doar Win32 aici: API-urile Tauri de monitor
/// așteaptă thread-ul principal, care poate aștepta chiar acum după acest thread (`apply`).
fn cursor_island(app: &AppHandle) -> Option<String> {
    use windows::Win32::Foundation::{POINT, RECT};
    use windows::Win32::Graphics::Gdi::{MonitorFromPoint, MonitorFromRect, MONITOR_DEFAULTTONEAREST};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
    let windows: Vec<_> = app.webview_windows().into_iter().filter(|(l, _)| l.starts_with("island-")).map(|(_, w)| w).collect();
    let cursor_mon = crate::platform::cursor_pos().map(|(x, y)| unsafe { MonitorFromPoint(POINT { x, y }, MONITOR_DEFAULTTONEAREST) });
    let on_cursor = windows.iter().find(|w| {
        let (Some(mon), Some(hwnd)) = (cursor_mon, crate::platform::hwnd_of(w)) else { return false };
        let mut r = RECT::default();
        unsafe { GetWindowRect(hwnd, &mut r).is_ok() && MonitorFromRect(&r, MONITOR_DEFAULTTONEAREST) == mon }
    });
    on_cursor.or(windows.first()).map(|w| w.label().to_string())
}

/// Deschide/strânge insula de pe monitorul cu cursorul.
fn toggle(app: &AppHandle) {
    if let Some(label) = cursor_island(app) {
        let _ = app.emit_to(label.as_str(), "island-toggle", ());
    }
}

/// Scurtătura notițelor: insula se deschide pe fila Notițe, cu câmpul de scris activ (JS cere focusul).
fn open_notes(app: &AppHandle) {
    if let Some(label) = cursor_island(app) {
        let _ = app.emit_to(label.as_str(), "island-notes", ());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combinatii() {
        assert_eq!(parse("Win+Shift+I"), Some((MOD_WIN | MOD_SHIFT | MOD_NOREPEAT, 'I' as u32)));
        assert_eq!(parse("Ctrl+Alt+Space"), Some((MOD_CONTROL | MOD_ALT | MOD_NOREPEAT, 0x20)));
        assert_eq!(parse("ctrl+f5"), Some((MOD_CONTROL | MOD_NOREPEAT, 0x74)));
        assert_eq!(parse("Alt+7"), Some((MOD_ALT | MOD_NOREPEAT, '7' as u32)));
    }

    #[test]
    fn doua_scurtaturi_distincte() {
        assert_eq!(parse("Win+Shift+N"), Some((MOD_WIN | MOD_SHIFT | MOD_NOREPEAT, b'N' as u32)));
        assert_ne!(ID_ISLAND, ID_NOTES);
    }

    #[test]
    fn erorile_celor_doua_scurtaturi_sunt_separate() {
        let mut e = HotkeyErrors::default();
        record(&mut e, ID_NOTES, "Win+Shift+N", false);
        assert_eq!(e, HotkeyErrors { island: None, notes: Some("Win+Shift+N".into()) });
        record(&mut e, ID_ISLAND, "Win+Shift+I", false);
        record(&mut e, ID_NOTES, "Win+Shift+N", true); // înregistrată acum: eroarea ei dispare
        assert_eq!(e, HotkeyErrors { island: Some("Win+Shift+I".into()), notes: None });
    }

    #[test]
    fn combinatii_invalide() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("I"), None); // fără modificator
        assert_eq!(parse("Shift+A"), None); // ar fura litera A mare
        assert_eq!(parse("Ctrl+Shift"), None); // fără tastă
        assert_eq!(parse("Ctrl+F25"), None);
        assert_eq!(parse("Ctrl+Esc"), None);
    }
}
