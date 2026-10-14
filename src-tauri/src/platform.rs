use tauri::{PhysicalPosition, WebviewWindow};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUNS_ACCEPTS_NOTIFICATIONS, QUNS_BUSY, QUNS_PRESENTATION_MODE,
    QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW, GetWindowRect, SetForegroundWindow, SetWindowLongPtrW, GWL_EXSTYLE,
    MONITORINFOF_PRIMARY, WS_EX_NOACTIVATE,
};

/// HWND-ul ferestrei Tauri, convertit în tipul din crate-ul nostru `windows`.
pub fn hwnd_of(win: &WebviewWindow) -> Option<HWND> {
    win.hwnd().ok().map(|h| HWND(h.0 as *mut core::ffi::c_void))
}

/// Așază fereastra lipită de marginea de sus, centrată pe monitorul principal.
pub fn place_top_center(win: &WebviewWindow) -> tauri::Result<()> {
    let monitor = match win.primary_monitor()? {
        Some(m) => Some(m),
        None => win.current_monitor()?,
    };
    if let Some(m) = monitor {
        let size = win.outer_size()?;
        let x = m.position().x + (m.size().width as i32 - size.width as i32) / 2;
        win.set_position(PhysicalPosition::new(x, m.position().y))?;
    }
    Ok(())
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

/// Aplicația din prim-plan rulează pe tot ecranul principal (video, joc)?
pub fn foreground_is_fullscreen(own: Option<HWND>) -> bool {
    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() || Some(fg) == own {
            return false;
        }
        let mut cls = [0u16; 64];
        let n = GetClassNameW(fg, &mut cls).max(0) as usize;
        let class = String::from_utf16_lossy(&cls[..n]);
        if matches!(class.as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd") {
            return false;
        }
        let mut wr = RECT::default();
        if GetWindowRect(fg, &mut wr).is_err() {
            return false;
        }
        let mon = MonitorFromWindow(fg, MONITOR_DEFAULTTONULL);
        if mon.is_invalid() {
            return false;
        }
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        if !GetMonitorInfoW(mon, &mut mi).as_bool() || mi.dwFlags & MONITORINFOF_PRIMARY == 0 {
            return false;
        }
        let m = mi.rcMonitor;
        let covers = covers_monitor((wr.left, wr.top, wr.right, wr.bottom), (m.left, m.top, m.right, m.bottom));
        let state = SHQueryUserNotificationState().map(|s| s.0).unwrap_or(QUNS_ACCEPTS_NOTIFICATIONS.0);
        is_fullscreen(covers, state)
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
