use std::sync::OnceLock;
use std::time::Instant;

use tauri::{AppHandle, Emitter, Manager};
use windows::core::w;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE, WM_WTSSESSION_CHANGE,
    WNDCLASSW, WTS_SESSION_UNLOCK,
};

/// În primele secunde după pornire nu anunțăm: aplicația pornește de obicei chiar după o deblocare/autentificare.
const STARTUP_QUIET_MS: u64 = 10_000;

/// Lacătul se arată doar dacă modulul e activ și aplicația rulează de cel puțin 10 s.
pub fn should_announce(started_ms: u64, now_ms: u64, enabled: bool) -> bool {
    enabled && now_ms.saturating_sub(started_ms) >= STARTUP_QUIET_MS
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static STARTED: OnceLock<Instant> = OnceLock::new();

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_WTSSESSION_CHANGE && wparam.0 as u32 == WTS_SESSION_UNLOCK {
        if let (Some(app), Some(started)) = (APP.get(), STARTED.get()) {
            let enabled = app.state::<crate::settings::SettingsState>().get().modules.unlock;
            if should_announce(0, started.elapsed().as_millis() as u64, enabled) {
                let _ = app.emit("session-unlock", ());
            }
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Thread propriu cu fereastră doar-pentru-mesaje: WM_WTSSESSION_CHANGE ajunge la fereastra care s-a înregistrat,
/// iar bucla de mesaje trebuie să ruleze pe thread-ul care a creat-o (regula Win32 a proiectului).
pub fn spawn(app: AppHandle) {
    let _ = APP.set(app);
    let _ = STARTED.set(Instant::now());
    std::thread::spawn(|| unsafe {
        let class = w!("DynamicIslandSessionWatcher");
        let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), lpszClassName: class, ..Default::default() };
        if RegisterClassW(&wc) == 0 {
            log::warn!("lacăt: clasa ferestrei nu s-a putut înregistra");
            return;
        }
        let Ok(hwnd) = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, HWND_MESSAGE, None, None, None) else {
            log::warn!("lacăt: fereastra de mesaje nu s-a putut crea");
            return;
        };
        if WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION).is_err() {
            log::warn!("lacăt: înregistrarea notificărilor de sesiune a eșuat");
            return;
        }
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = DispatchMessageW(&msg);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignorat_la_pornire() {
        assert!(!should_announce(0, 9_999, true));
        assert!(should_announce(0, 10_000, true));
        assert!(!should_announce(0, 60_000, false));
    }
}
