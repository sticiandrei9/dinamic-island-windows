use tauri::WebviewWindow;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, QUNS_ACCEPTS_NOTIFICATIONS, QUNS_BUSY, QUNS_PRESENTATION_MODE,
    QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetCursorPos, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, IsWindow,
    SetForegroundWindow, SetWindowLongPtrW, GWL_EXSTYLE,
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

/// Înainte de o lansare sau aducere în față pornită dintr-un click: insula are `WS_EX_NOACTIVATE`, deci procesul
/// nostru nu e în prim-plan, iar aplicația deschisă (explorer, browserul, fereastra țintă) s-ar deschide în spate.
/// Merge pentru că procesul nostru a primit ultimul eveniment de input (click-ul).
pub fn allow_foreground() {
    use windows::Win32::UI::WindowsAndMessaging::{AllowSetForegroundWindow, ASFW_ANY};
    let _ = unsafe { AllowSetForegroundWindow(ASFW_ANY) };
}

/// HWND-ul ferestrei Tauri, convertit în tipul din crate-ul nostru `windows`.
pub fn hwnd_of(win: &WebviewWindow) -> Option<HWND> {
    win.hwnd().ok().map(|h| HWND(h.0 as *mut core::ffi::c_void))
}

/// `WS_EX_NOACTIVATE` pe insulă (click-urile n-o activează, deci nu fură focusul aplicației active) sau scos.
/// Trece prin `set_focusable` al lui tao, nu prin `SetWindowLongPtrW`: tao recalculează tot stilul extins din
/// steagurile lui la orice schimbare (de ex. `set_ignore_cursor_events`, care comută la fiecare intrare/ieșire
/// a cursorului) și ar șterge un bit pus de mână. Stilul rămâne cel al ferestrei fără margini, deci fără bară de titlu.
pub fn set_no_activate(win: &WebviewWindow, on: bool) {
    let _ = win.set_focusable(!on);
}

/// Dă insulei focusul de tastatură pentru câmpul de notițe: scoate `WS_EX_NOACTIVATE`, se atașează la
/// input-ul ferestrei din prim-plan (altfel Windows refuză `SetForegroundWindow`, ca în `focus_hwnd`),
/// activează insula și îi dă focusul. Întoarce fereastra din prim-plan de dinainte. Doar pe thread-ul
/// principal: `SetFocus` merge doar pe thread-ul care deține fereastra.
pub fn activate_for_typing(win: &WebviewWindow) -> Option<isize> {
    use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    let me = hwnd_of(win)?;
    set_no_activate(win, false);
    unsafe {
        let fg = GetForegroundWindow();
        let prev = (!fg.0.is_null() && fg != me).then_some(fg.0 as isize);
        let fg_tid = if fg.0.is_null() { 0 } else { GetWindowThreadProcessId(fg, None) };
        let me_tid = GetCurrentThreadId();
        let attached = fg_tid != 0 && fg_tid != me_tid && AttachThreadInput(me_tid, fg_tid, true).as_bool();
        let _ = SetForegroundWindow(me);
        let _ = SetFocus(me);
        if attached {
            let _ = AttachThreadInput(me_tid, fg_tid, false);
        }
        // fereastra exterioară activă nu ajunge: tastele merg în pagină doar după MoveFocus în WebView2
        let _ = AsRef::<tauri::Webview>::as_ref(win).set_focus();
        prev
    }
}

/// Fereastra de dinainte există încă și insula e în continuare în prim-plan (altfel utilizatorul a ales
/// între timp altă fereastră și nu i-o luăm).
pub fn can_refocus(prev: isize, win: &WebviewWindow) -> bool {
    unsafe { IsWindow(HWND(prev as *mut core::ffi::c_void)).as_bool() && Some(GetForegroundWindow()) == hwnd_of(win) }
}

/// Insula e fereastra din prim-plan.
pub fn is_foreground(win: &WebviewWindow) -> bool {
    unsafe { Some(GetForegroundWindow()) == hwnd_of(win) }
}

/// Dă prim-planul ferestrei desktopului (când nu mai există o fereastră de dinainte).
pub fn focus_shell() {
    use windows::Win32::UI::WindowsAndMessaging::GetShellWindow;
    unsafe {
        let shell = GetShellWindow();
        if !shell.0.is_null() {
            let _ = SetForegroundWindow(shell);
        }
    }
}

/// Redă focusul ferestrei de dinainte (insula e în prim-plan, deci Windows permite).
pub fn focus_previous(prev: isize) {
    unsafe {
        let _ = SetForegroundWindow(HWND(prev as *mut core::ffi::c_void));
    }
}

/// O fereastră Win32 ascunsă, proprietarul insulelor: ferestrele cu proprietar nu apar în Alt+Tab
/// (`WS_EX_TOOLWINDOW` ar face același lucru, dar adăugat după creare desenează o bară de titlu).
/// Se creează pe thread-ul principal, care are bucla de mesaje.
pub fn create_hidden_owner() -> Option<isize> {
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{CreateWindowExW, WINDOW_EX_STYLE, WS_POPUP};
    unsafe {
        CreateWindowExW(WINDOW_EX_STYLE(0), w!("STATIC"), w!("Dynamic Island"), WS_POPUP, 0, 0, 0, 0, None, None, None, None)
            .ok()
            .map(|h| h.0 as isize)
    }
}

pub fn set_owner(win: &WebviewWindow, owner: isize) {
    use windows::Win32::UI::WindowsAndMessaging::GWLP_HWNDPARENT;
    if let Some(h) = hwnd_of(win) {
        unsafe {
            SetWindowLongPtrW(h, GWLP_HWNDPARENT, owner);
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

pub(crate) unsafe extern "system" fn collect_hwnd(hwnd: HWND, lparam: windows::Win32::Foundation::LPARAM) -> windows::Win32::Foundation::BOOL {
    (*(lparam.0 as *mut Vec<HWND>)).push(hwnd);
    windows::Win32::Foundation::BOOL(1)
}

/// Monitoarele (left, top, right, bottom) pe care rulează ceva pe tot ecranul (video, joc).
/// Pentru fiecare monitor contează fereastra cea mai de sus (ordinea Z) de pe el; ferestrele noastre,
/// ascunse, de tip „tool” și desktop-ul/taskbar-ul sunt sărite.
pub fn fullscreen_monitors() -> Vec<(i32, i32, i32, i32)> {
    use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowLongW, IsWindowVisible, GWL_STYLE, WS_CAPTION, WS_EX_TOOLWINDOW};
    let mut out = Vec::new();
    let mut seen: Vec<isize> = Vec::new();
    unsafe {
        let mut list: Vec<HWND> = Vec::new();
        let _ = EnumWindows(Some(collect_hwnd), windows::Win32::Foundation::LPARAM(&mut list as *mut Vec<HWND> as isize));
        let fg = GetForegroundWindow();
        let own = GetCurrentProcessId();
        let state = SHQueryUserNotificationState().map(|s| s.0).unwrap_or(QUNS_ACCEPTS_NOTIFICATIONS.0);
        for h in list {
            if !IsWindowVisible(h).as_bool() || GetWindowLongW(h, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW.0 != 0 {
                continue;
            }
            let mut cloaked = 0u32;
            let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut _, 4);
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, Some(&mut pid));
            let mut wr = RECT::default();
            if cloaked != 0 || pid == own || GetWindowRect(h, &mut wr).is_err() || wr.right <= wr.left || wr.bottom <= wr.top {
                continue;
            }
            let mon = MonitorFromWindow(h, MONITOR_DEFAULTTONULL);
            if mon.is_invalid() || seen.contains(&(mon.0 as isize)) {
                continue;
            }
            seen.push(mon.0 as isize);
            let mut cls = [0u16; 64];
            let n = GetClassNameW(h, &mut cls).max(0) as usize;
            if matches!(String::from_utf16_lossy(&cls[..n]).as_str(), "Progman" | "WorkerW" | "Shell_TrayWnd" | "Shell_SecondaryTrayWnd") {
                continue;
            }
            let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
            if !GetMonitorInfoW(mon, &mut mi).as_bool() {
                continue;
            }
            let m = (mi.rcMonitor.left, mi.rcMonitor.top, mi.rcMonitor.right, mi.rcMonitor.bottom);
            let covers = covers_monitor((wr.left, wr.top, wr.right, wr.bottom), m);
            let has_caption = GetWindowLongW(h, GWL_STYLE) as u32 & WS_CAPTION.0 == WS_CAPTION.0;
            if window_fullscreen(covers, h == fg, state, has_caption) {
                out.push(m);
            }
        }
    }
    out
}

/// Fereastra de sus a unui monitor rulează pe tot ecranul? Cea activă: starea Windows (ca la notificări).
/// Una inactivă (alt monitor): acoperă monitorul și n-are bară de titlu (video F11, joc); o fereastră
/// maximizată obișnuită are bară de titlu.
pub fn window_fullscreen(covers: bool, foreground: bool, notification_state: i32, has_caption: bool) -> bool {
    if foreground { is_fullscreen(covers, notification_state) } else { covers && !has_caption }
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
    fn fullscreen_pe_alt_monitor_decat_cel_activ() {
        // fereastra activă: decide starea Windows (QUNS), ca înainte
        assert!(window_fullscreen(true, true, QUNS_BUSY.0, true));
        assert!(!window_fullscreen(true, true, QUNS_ACCEPTS_NOTIFICATIONS.0, false));
        // pe alt monitor, inactivă: video F11 / joc fără bară de titlu care acoperă monitorul
        assert!(window_fullscreen(true, false, QUNS_ACCEPTS_NOTIFICATIONS.0, false));
        // o fereastră maximizată obișnuită (are bară de titlu) nu e fullscreen
        assert!(!window_fullscreen(true, false, QUNS_ACCEPTS_NOTIFICATIONS.0, true));
        assert!(!window_fullscreen(false, false, QUNS_BUSY.0, false));
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
