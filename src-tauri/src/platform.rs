use tauri::WebviewWindow;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUNS_ACCEPTS_NOTIFICATIONS, QUNS_BUSY, QUNS_PRESENTATION_MODE,
    QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, GetWindowThreadProcessId,
    SetForegroundWindow, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
};

/// COM inițializat pe thread-ul curent cât trăiește garda; `CoUninitialize` doar dacă inițializarea a reușit
/// (inclusiv S_FALSE = era deja inițializat în același mod).
pub struct ComGuard(bool);

impl ComGuard {
    pub fn new(mode: windows::Win32::System::Com::COINIT) -> Self {
        Self(unsafe { windows::Win32::System::Com::CoInitializeEx(None, mode) }.is_ok())
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { windows::Win32::System::Com::CoUninitialize() };
        }
    }
}

/// HWND-ul ferestrei Tauri, convertit în tipul din crate-ul nostru `windows`.
pub fn hwnd_of(win: &WebviewWindow) -> Option<HWND> {
    win.hwnd().ok().map(|h| HWND(h.0 as *mut core::ffi::c_void))
}

/// Click-urile pe insulă nu fură focusul aplicației active.
pub fn make_no_activate(win: &WebviewWindow) {
    if let Some(h) = hwnd_of(win) {
        unsafe {
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            let extra = WS_EX_NOACTIVATE.0 as isize;
            SetWindowLongPtrW(h, GWL_EXSTYLE, ex | extra);
        }
    }
}

/// Afișează fereastra prin Tauri (ShowWindow direct strică rama fără margini a lui tao),
/// apoi redă focusul ferestrei care îl avea înainte.
pub fn show_keep_focus(win: &WebviewWindow) -> tauri::Result<()> {
    let prev = unsafe { GetForegroundWindow() };
    win.show()?;
    if !prev.0.is_null() && Some(prev) != hwnd_of(win) {
        unsafe {
            let _ = SetForegroundWindow(prev);
        }
    }
    Ok(())
}


pub fn cursor_pos() -> Option<(i32, i32)> {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p).ok()? };
    Some((p.x, p.y))
}

/// (left, top, right, bottom): fereastra acoperă tot monitorul.
pub fn covers_monitor(win: (i32, i32, i32, i32), mon: (i32, i32, i32, i32)) -> bool {
    win.0 <= mon.0 && win.1 <= mon.1 && win.2 >= mon.2 && win.3 >= mon.3
}

/// Dreptunghiul (left, top, right, bottom) al monitorului pe care aplicația din prim-plan
/// rulează pe tot ecranul (video, joc), dacă există. Ferestrele procesului nostru sunt ignorate.
pub fn fullscreen_monitor() -> Option<(i32, i32, i32, i32)> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return None;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(fg, Some(&mut pid));
        if pid == GetCurrentProcessId() {
            return None;
        }
        let mut cls = [0u16; 64];
        let n = GetClassNameW(fg, &mut cls).max(0) as usize;
        let class = String::from_utf16_lossy(&cls[..n]);
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
            return None;
        }
        let mut wr = RECT::default();
        GetWindowRect(fg, &mut wr).ok()?;
        let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONULL);
        if mon.is_invalid() {
            return None;
        }
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() {
            return None;
        }
        let m = mi.rcMonitor;
        let m = (m.left, m.top, m.right, m.bottom);
        let covers = covers_monitor((wr.left, wr.top, wr.right, wr.bottom), m);
        let state = SHQueryUserNotificationState().map(|s| s.0).unwrap_or(QUNS_ACCEPTS_NOTIFICATIONS.0);
        is_fullscreen(covers, state).then_some(m)
    }
}

/// Fereastra acoperă monitorul ȘI Windows o consideră pe tot ecranul (aceeași stare cu care
/// suprimă notificările). Doar acoperirea nu ajunge: cu taskbar-ul pe auto-hide,
/// orice fereastră maximizată acoperă monitorul.
pub fn is_fullscreen(covers: bool, notification_state: i32) -> bool {
    covers
        && [QUNS_BUSY.0, QUNS_RUNNING_D3D_FULL_SCREEN.0, QUNS_PRESENTATION_MODE.0].contains(&notification_state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fullscreen_detectat() {
        let mon = (0, 0, 1920, 1080);
        assert!(covers_monitor((0, 0, 1920, 1080), mon));
        assert!(covers_monitor((-8, -8, 1928, 1088), mon)); // ferestre cu margini în afara ecranului
        assert!(!covers_monitor((0, 0, 1920, 1040), mon)); // maximizat, cu taskbar vizibil
        assert!(!covers_monitor((100, 100, 800, 600), mon));
    }

    #[test]
    fn maximizat_cu_taskbar_ascuns_nu_e_fullscreen() {
        // fereastra maximizată acoperă monitorul (taskbar auto-hide), dar Windows acceptă notificări
        assert!(!is_fullscreen(true, QUNS_ACCEPTS_NOTIFICATIONS.0));
        assert!(is_fullscreen(true, QUNS_BUSY.0)); // video F11 / aplicație pe tot ecranul
        assert!(is_fullscreen(true, QUNS_RUNNING_D3D_FULL_SCREEN.0)); // joc
        assert!(is_fullscreen(true, QUNS_PRESENTATION_MODE.0));
        assert!(!is_fullscreen(false, QUNS_BUSY.0)); // fullscreen pe alt monitor
    }
}
