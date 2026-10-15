use std::time::Duration;

use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Globalization::{GetLocaleInfoEx, LCIDToLocaleName, LOCALE_SISO639LANGNAME, LOCALE_SNATIVELANGUAGENAME};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, GetKeyboardLayout, VK_CAPITAL};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

use crate::alerts::Alert;
use crate::i18n::t;
use crate::settings::SettingsState;

/// Limba s-a schimbat în aceeași fereastră? Cu „altă limbă pentru fiecare aplicație” (setare Windows),
/// trecerea la altă fereastră schimbă și ea limba; atunci nu anunțăm.
pub fn layout_changed(prev: Option<(isize, u16)>, cur: (isize, u16)) -> bool {
    matches!(prev, Some((hwnd, lang)) if hwnd == cur.0 && lang != cur.1)
}

/// Prima literă mare („русский” → „Русский”).
pub fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_uppercase().chain(c).collect()).unwrap_or_default()
}

fn locale_info(langid: u16, kind: u32) -> String {
    let mut name = [0u16; 85];
    let n = unsafe { LCIDToLocaleName(langid as u32, Some(&mut name), 0) };
    if n <= 0 {
        return String::new();
    }
    let mut buf = [0u16; 85];
    let len = unsafe { GetLocaleInfoEx(windows::core::PCWSTR(name.as_ptr()), kind, Some(&mut buf)) };
    String::from_utf16_lossy(&buf[..(len.max(1) - 1) as usize])
}

/// „Русский · RU” pentru limba de tastatură dată.
pub fn layout_alert(langid: u16) -> Alert {
    Alert {
        icon: "keyboard".into(),
        title: capitalize(&locale_info(langid, LOCALE_SNATIVELANGUAGENAME)),
        detail: locale_info(langid, LOCALE_SISO639LANGNAME).to_uppercase(),
        color: "var(--fg)".into(),
        level: None,
    }
}

pub fn caps_alert(on: bool) -> Alert {
    Alert {
        icon: "capslock".into(),
        title: "Caps Lock".into(),
        detail: t(if on { "kb.on" } else { "kb.off" }),
        color: if on { "var(--green)" } else { "var(--fg2)" }.into(),
        level: None,
    }
}

/// Anunțurile proprii ale unor laptopuri, afișate sus pe mijloc, peste insulă (GIGABYTE Control Center).
pub fn is_vendor_osd(exe: &str) -> bool {
    exe.eq_ignore_ascii_case("OSDwindow.exe")
}

/// Ascunde anunțul de Caps Lock al producătorului (insula îl arată pe al ei). Îl căutăm 1,5 s, cât apare;
/// celelalte anunțuri ale lui (touchpad, mod avion) rămân, fiindcă ascundem doar după Caps Lock.
fn hide_vendor_osd() {
    use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, IsWindowVisible, ShowWindow, SW_HIDE};
    std::thread::spawn(|| {
        for _ in 0..50 {
            let mut list: Vec<windows::Win32::Foundation::HWND> = Vec::new();
            unsafe {
                let _ = EnumWindows(Some(crate::platform::collect_hwnd), windows::Win32::Foundation::LPARAM(&mut list as *mut _ as isize));
                for h in list {
                    let mut pid = 0u32;
                    GetWindowThreadProcessId(h, Some(&mut pid));
                    if IsWindowVisible(h).as_bool() && is_vendor_osd(&crate::windows_list::exe_name(pid)) {
                        let _ = ShowWindow(h, SW_HIDE);
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(30));
        }
    });
}

/// Fereastra activă și limba ei de tastatură.
fn foreground_layout() -> (isize, u16) {
    unsafe {
        let hwnd = GetForegroundWindow();
        let thread = GetWindowThreadProcessId(hwnd, None);
        (hwnd.0 as isize, (GetKeyboardLayout(thread).0 as usize & 0xffff) as u16)
    }
}

/// La 150 ms: limba de tastatură a ferestrei active și Caps Lock; anunță doar schimbările.
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut layout: Option<(isize, u16)> = None;
        let mut caps: Option<bool> = None;
        loop {
            std::thread::sleep(Duration::from_millis(150));
            if !app.state::<SettingsState>().get().modules.keyboard {
                (layout, caps) = (None, None);
                continue;
            }
            let cur = foreground_layout();
            if cur.0 != 0 {
                if layout_changed(layout, cur) {
                    let _ = app.emit("kb-alert", layout_alert(cur.1));
                }
                layout = Some(cur);
            }
            let on = unsafe { GetKeyState(VK_CAPITAL.0 as i32) } & 1 == 1;
            if caps.is_some_and(|c| c != on) {
                hide_vendor_osd();
                let _ = app.emit("kb-alert", caps_alert(on));
            }
            caps = Some(on);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limba_schimbata_doar_in_aceeasi_fereastra() {
        assert!(layout_changed(Some((1, 0x418)), (1, 0x419)));
        assert!(!layout_changed(Some((1, 0x418)), (2, 0x419))); // altă fereastră
        assert!(!layout_changed(Some((1, 0x418)), (1, 0x418)));
        assert!(!layout_changed(None, (1, 0x419))); // la pornire nu se anunță
    }

    #[test]
    fn anuntul_gigabyte() {
        assert!(is_vendor_osd("OSDwindow.exe") && is_vendor_osd("osdwindow.EXE"));
        assert!(!is_vendor_osd("explorer.exe"));
    }

    #[test]
    fn numele_limbii() {
        assert_eq!(capitalize("русский"), "Русский");
        assert_eq!(capitalize(""), "");
        let a = layout_alert(0x419);
        assert_eq!((a.title.as_str(), a.detail.as_str()), ("Русский", "RU"));
        assert_eq!(layout_alert(0x418).detail, "RO");
    }
}

#[cfg(test)]
mod live {
    use super::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::{keybd_event, KEYEVENTF_KEYUP};

    /// Manual: Caps Lock citit dintr-un thread fără fereastră (ca thread-ul modulului), apăsat de 2 ori.
    #[test]
    #[ignore]
    fn caps_vazut_din_thread_de_fundal() {
        let h = std::thread::spawn(|| {
            let mut seen = Vec::new();
            for _ in 0..40 {
                let on = unsafe { GetKeyState(VK_CAPITAL.0 as i32) } & 1 == 1;
                if seen.last() != Some(&on) {
                    seen.push(on);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            seen
        });
        for _ in 0..2 {
            std::thread::sleep(Duration::from_millis(500));
            unsafe {
                keybd_event(0x14, 0, Default::default(), 0);
                keybd_event(0x14, 0, KEYEVENTF_KEYUP, 0);
            }
        }
        let seen = h.join().unwrap();
        println!("stări văzute: {seen:?}");
        assert_eq!(seen.len(), 3);
    }
}
