use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

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

struct Worker {
    thread: u32,
    tx: SyncSender<(String, SyncSender<bool>)>,
}

static WORKER: OnceLock<Worker> = OnceLock::new();

/// Thread-ul care ține scurtătura înregistrată (WM_HOTKEY ajunge doar la thread-ul care a înregistrat-o).
pub fn spawn(app: AppHandle) {
    let (tx, rx) = sync_channel::<(String, SyncSender<bool>)>(4);
    let (ready_tx, ready_rx) = sync_channel(1);
    std::thread::spawn(move || {
        let _ = ready_tx.send(unsafe { GetCurrentThreadId() });
        let mut registered = false;
        let mut msg = MSG::default();
        while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
            if msg.message == WM_HOTKEY {
                toggle(&app);
            } else if msg.message == WM_APP {
                while let Ok((combo, reply)) = rx.try_recv() {
                    if registered {
                        let _ = unsafe { UnregisterHotKey(None, 1) };
                    }
                    registered = parse(&combo).is_some_and(|(m, vk)| unsafe { RegisterHotKey(None, 1, HOT_KEY_MODIFIERS(m), vk) }.is_ok());
                    let _ = reply.send(registered || combo.trim().is_empty());
                }
            }
        }
    });
    if let Ok(thread) = ready_rx.recv() {
        let _ = WORKER.set(Worker { thread, tx });
    }
}

/// Înregistrează scurtătura (goală = niciuna). False dacă Windows o refuză (ocupată de alt program).
pub fn apply(combo: &str) -> bool {
    let Some(w) = WORKER.get() else { return false };
    let (reply_tx, reply_rx) = sync_channel(1);
    if w.tx.send((combo.to_string(), reply_tx)).is_err() {
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

static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

/// Ultima scurtătură refuzată de Windows (pentru Setări).
#[tauri::command]
pub fn hotkey_error() -> Option<String> {
    LAST_ERROR.lock().unwrap().clone()
}

pub fn apply_and_report(combo: &str) {
    let ok = apply(combo);
    if !ok {
        log::warn!("scurtătura {combo} n-a putut fi înregistrată (ocupată de alt program?)");
    }
    *LAST_ERROR.lock().unwrap() = (!ok).then(|| combo.to_string());
}

/// Deschide/strânge insula de pe monitorul cu cursorul. Doar Win32 aici: API-urile Tauri de monitor
/// așteaptă thread-ul principal, care poate aștepta chiar acum după acest thread (`apply`).
fn toggle(app: &AppHandle) {
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
    if let Some(w) = on_cursor.or(windows.first()) {
        let _ = app.emit_to(w.label(), "island-toggle", ());
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
    fn combinatii_invalide() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("I"), None); // fără modificator
        assert_eq!(parse("Shift+A"), None); // ar fura litera A mare
        assert_eq!(parse("Ctrl+Shift"), None); // fără tastă
        assert_eq!(parse("Ctrl+F25"), None);
        assert_eq!(parse("Ctrl+Esc"), None);
    }
}
