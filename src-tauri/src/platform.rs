use tauri::{PhysicalPosition, WebviewWindow};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowLongPtrW, SetForegroundWindow, SetWindowLongPtrW, GWL_EXSTYLE,
    WS_EX_NOACTIVATE,
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
