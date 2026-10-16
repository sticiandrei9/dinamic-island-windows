//! Istoricul clipboard-ului: ultimele 5 texte, doar în memorie. Textul nu ajunge niciodată în log sau pe disc.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use tauri::{AppHandle, Emitter, Listener, Manager};
use windows::core::w;
use windows::Win32::Foundation::{HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardFormatNameW, OpenClipboard,
    RegisterClipboardFormatW, RemoveClipboardFormatListener,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, PostMessageW, RegisterClassW, HWND_MESSAGE, MSG, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_APP, WM_CLIPBOARDUPDATE, WNDCLASSW,
};

const MAX_ITEMS: usize = 5;
/// shortcut: textele peste ~200 KB nu intră în istoric (ar fi trimise întregi spre fereastră la fiecare eveniment); dacă e nevoie, se trimite doar un fragment.
const MAX_BYTES: usize = 200_000;
const CF_UNICODETEXT: u32 = 13;
/// Mesaj trimis ferestrei de ascultare când se schimbă setările (pornit/oprit).
const WM_SYNC: u32 = WM_APP + 1;

/// Pune `text` (fără spații la capete) în capul listei; gol → neschimbat; existent → mutat; tăiat la `max`.
pub fn push_history(mut list: Vec<String>, text: &str, max: usize) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return list;
    }
    list.retain(|x| x != text);
    list.insert(0, text.to_string());
    list.truncate(max);
    list
}

/// Managerele de parole și aplicațiile bancare își marchează copierile cu aceste formate.
pub fn excluded(formats: &[String], can_include: Option<u32>) -> bool {
    can_include == Some(0)
        || formats.iter().any(|f| {
            f.eq_ignore_ascii_case("ExcludeClipboardContentFromMonitorProcessing") || f.eq_ignore_ascii_case("Clipboard Viewer Ignore")
        })
}

static APP: OnceLock<AppHandle> = OnceLock::new();
static ITEMS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static LISTENING: AtomicBool = AtomicBool::new(false);
static WND: AtomicIsize = AtomicIsize::new(0);

fn publish(items: Vec<String>, new: bool) {
    if let Some(app) = APP.get() {
        let _ = app.emit("clipboard", serde_json::json!({ "items": items, "new": new }));
    }
}

#[tauri::command]
pub fn clipboard_list() -> Vec<String> {
    ITEMS.lock().unwrap().clone()
}

#[tauri::command]
pub fn clipboard_clear() {
    ITEMS.lock().unwrap().clear();
    publish(vec![], false);
}

/// Pune în clipboard textul de la `index` și îl mută în capul listei, fără „Copiat” (new = false).
#[tauri::command]
pub fn clipboard_copy(index: usize) -> Result<(), String> {
    let text = ITEMS.lock().unwrap().get(index).cloned().ok_or("element inexistent")?;
    // mutarea se face înainte de scriere: WM_CLIPBOARDUPDATE-ul scrierii noastre găsește textul deja în cap și nu mai face nimic
    let next = {
        let mut items = ITEMS.lock().unwrap();
        *items = push_history(std::mem::take(&mut *items), &text, MAX_ITEMS);
        items.clone()
    };
    publish(next, false);
    crate::shelf::shelf_copy_path(text) // scrie text Unicode în clipboard, cu reîncercări
}

/// Octeții formatului `fmt` din clipboard-ul deschis.
unsafe fn with_data<T>(fmt: u32, f: impl FnOnce(&[u8]) -> T) -> Option<T> {
    let g = HGLOBAL(GetClipboardData(fmt).ok()?.0);
    let ptr = GlobalLock(g) as *const u8;
    if ptr.is_null() {
        return None;
    }
    let out = f(std::slice::from_raw_parts(ptr, GlobalSize(g)));
    let _ = GlobalUnlock(g);
    Some(out)
}

/// Textul din clipboard, sau None dacă nu e text, nu se poate deschide ori poartă un format de excludere.
unsafe fn read_text(hwnd: HWND) -> Option<String> {
    let mut opened = false;
    for _ in 0..5 {
        if OpenClipboard(hwnd).is_ok() {
            opened = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20)); // alt program îl ține deschis o clipă
    }
    if !opened {
        return None;
    }
    let text = (|| {
        let can_include_id = RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory"));
        let (mut names, mut can_include, mut fmt) = (Vec::new(), None, 0);
        loop {
            fmt = EnumClipboardFormats(fmt);
            if fmt == 0 {
                break;
            }
            if fmt < 0xC000 {
                continue; // formatele standard n-au nume înregistrat
            }
            let mut buf = [0u16; 128];
            let n = GetClipboardFormatNameW(fmt, &mut buf);
            if n > 0 {
                names.push(String::from_utf16_lossy(&buf[..n as usize]));
            }
            if fmt == can_include_id {
                can_include = with_data(fmt, |b| b.get(..4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))).flatten();
            }
        }
        if excluded(&names, can_include) {
            return None;
        }
        with_data(CF_UNICODETEXT, |b| {
            let units: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
            String::from_utf16_lossy(&units)
        })
    })();
    let _ = CloseClipboard();
    text
}

fn on_update(hwnd: HWND) {
    if !LISTENING.load(Ordering::Relaxed) {
        return;
    }
    let Some(text) = (unsafe { read_text(hwnd) }) else { return };
    if text.len() > MAX_BYTES {
        return;
    }
    let next = {
        let mut items = ITEMS.lock().unwrap();
        let next = push_history(items.clone(), &text, MAX_ITEMS);
        if next == *items {
            return; // gol sau identic cu primul (inclusiv ecoul copierii făcute din insulă)
        }
        *items = next.clone();
        next
    };
    publish(next, true);
}

/// Pornește sau oprește ascultarea după `modules.clipboard`; oprit → istoricul se golește.
fn sync(hwnd: HWND) {
    let Some(app) = APP.get() else { return };
    let on = app.state::<crate::settings::SettingsState>().get().modules.clipboard;
    let listening = LISTENING.load(Ordering::Relaxed);
    if on && !listening {
        match unsafe { AddClipboardFormatListener(hwnd) } {
            Ok(()) => LISTENING.store(true, Ordering::Relaxed),
            Err(_) => log::warn!("clipboard: ascultătorul nu s-a putut înregistra"),
        }
    } else if !on && listening {
        let _ = unsafe { RemoveClipboardFormatListener(hwnd) };
        LISTENING.store(false, Ordering::Relaxed);
        ITEMS.lock().unwrap().clear();
        publish(vec![], false);
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CLIPBOARDUPDATE => on_update(hwnd),
        WM_SYNC => sync(hwnd),
        _ => return unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
    LRESULT(0)
}

/// Thread propriu cu fereastră doar-pentru-mesaje: ascultătorul de clipboard trimite mesajele ferestrei care l-a înregistrat,
/// iar bucla de mesaje rulează pe thread-ul care a creat-o (regula Win32 a proiectului). Fără sondaj.
pub fn spawn(app: AppHandle) {
    let _ = APP.set(app.clone());
    // setările se schimbă pe alt thread: doar anunțăm fereastra, Win32 rămâne pe thread-ul ei
    app.listen("settings-changed", |_| {
        let h = WND.load(Ordering::Relaxed);
        if h != 0 {
            let _ = unsafe { PostMessageW(HWND(h as *mut _), WM_SYNC, WPARAM(0), LPARAM(0)) };
        }
    });
    std::thread::spawn(|| unsafe {
        let class = w!("DynamicIslandClipboardWatcher");
        let wc = WNDCLASSW { lpfnWndProc: Some(wndproc), lpszClassName: class, ..Default::default() };
        if RegisterClassW(&wc) == 0 {
            log::warn!("clipboard: clasa ferestrei nu s-a putut înregistra");
            return;
        }
        let Ok(hwnd) = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, HWND_MESSAGE, None, None, None) else {
            log::warn!("clipboard: fereastra de mesaje nu s-a putut crea");
            return;
        };
        WND.store(hwnd.0 as isize, Ordering::Relaxed);
        sync(hwnd); // starea de la pornire
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
    fn istoric_de_5() {
        let mut l = vec![];
        for t in ["a", "b", "c", "d", "e", "f"] {
            l = push_history(l, t, 5);
        }
        assert_eq!(l, ["f", "e", "d", "c", "b"]);
        assert_eq!(push_history(l.clone(), "  d  ", 5), ["d", "f", "e", "c", "b"]);
        assert_eq!(push_history(l.clone(), "   ", 5), l);
    }

    #[test]
    fn parole_excluse() {
        assert!(excluded(&["ExcludeClipboardContentFromMonitorProcessing".into()], None));
        assert!(excluded(&["Clipboard Viewer Ignore".into()], None));
        // numele formatelor înregistrate nu țin cont de majuscule (RegisterClipboardFormatW)
        assert!(excluded(&["clipboard viewer ignore".into()], None));
        assert!(excluded(&["EXCLUDECLIPBOARDCONTENTFROMMONITORPROCESSING".into()], None));
        assert!(excluded(&[], Some(0)));
        assert!(!excluded(&["CF_UNICODETEXT".into()], Some(1)));
        assert!(!excluded(&[], None));
    }
}
