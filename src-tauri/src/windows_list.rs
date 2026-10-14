use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, OnceLock};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, GetObjectW, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HDC,
};
use windows::Win32::Storage::FileSystem::FILE_FLAGS_AND_ATTRIBUTES;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Threading::{
    AttachThreadInput, GetCurrentProcessId, GetCurrentThreadId, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::Shell::{SHGetFileInfoW, SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, DestroyIcon, EnumWindows, GetForegroundWindow, GetIconInfo, GetWindow, GetWindowLongW,
    GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible,
    SetForegroundWindow, ShowWindow, GWL_EXSTYLE, GW_OWNER, HICON, ICONINFO, SW_RESTORE, WS_EX_TOOLWINDOW,
};

#[derive(Serialize, Clone, Debug)]
pub struct AppWindow {
    pub hwnd: isize,
    pub title: String,
    pub exe: String,
    pub icon: Option<String>,
}

pub struct Candidate {
    pub visible: bool,
    pub has_owner: bool,
    pub title: String,
    pub ex_style: u32,
    pub cloaked: bool,
    pub is_self: bool,
}

pub fn is_app_window(c: &Candidate) -> bool {
    c.visible
        && !c.has_owner
        && !c.cloaked
        && !c.is_self
        && c.ex_style & WS_EX_TOOLWINDOW.0 == 0
        && !c.title.trim().is_empty()
        && c.title != "Program Manager"
}

/// O singură iconiță pe aplicație (prima fereastră în ordinea Z), cu excepția aplicațiilor UWP,
/// care rulează toate sub ApplicationFrameHost.exe.
pub fn dedupe_by_exe(list: Vec<AppWindow>) -> Vec<AppWindow> {
    let mut seen = HashSet::new();
    list.into_iter()
        .filter(|w| {
            let exe = w.exe.to_lowercase();
            exe == "applicationframehost.exe" || exe.is_empty() || seen.insert(exe)
        })
        .collect()
}

pub fn bgra_to_rgba(buf: &mut [u8]) {
    let has_alpha = buf.chunks_exact(4).any(|p| p[3] != 0);
    for p in buf.chunks_exact_mut(4) {
        p.swap(0, 2);
        if !has_alpha {
            p[3] = 255;
        }
    }
}

pub fn encode_png(rgba: &[u8], w: u32, h: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(rgba).ok()?;
    }
    Some(out)
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let v = &mut *(lparam.0 as *mut Vec<HWND>);
    v.push(hwnd);
    BOOL(1)
}

fn top_level_windows() -> Vec<HWND> {
    let mut v: Vec<HWND> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut v as *mut Vec<HWND> as isize));
    }
    v
}

unsafe fn candidate(h: HWND, own_pid: u32) -> (Candidate, u32) {
    let has_owner = GetWindow(h, GW_OWNER).map(|o| !o.0.is_null()).unwrap_or(false);
    let len = GetWindowTextLengthW(h).max(0) as usize;
    let mut buf = vec![0u16; len + 1];
    let n = GetWindowTextW(h, &mut buf).max(0) as usize;
    let title = String::from_utf16_lossy(&buf[..n]);
    let ex_style = GetWindowLongW(h, GWL_EXSTYLE) as u32;
    let mut cloaked: u32 = 0;
    let _ = DwmGetWindowAttribute(h, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut _, 4);
    let mut pid = 0u32;
    GetWindowThreadProcessId(h, Some(&mut pid));
    let c = Candidate { visible: true, has_owner, title, ex_style, cloaked: cloaked != 0, is_self: pid == own_pid };
    (c, pid)
}

unsafe fn exe_path(pid: u32) -> Option<String> {
    let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len);
    let _ = CloseHandle(h);
    r.ok()?;
    Some(String::from_utf16_lossy(&buf[..len as usize]))
}

unsafe fn hicon_to_png(icon: HICON) -> Option<Vec<u8>> {
    let mut ii = ICONINFO::default();
    GetIconInfo(icon, &mut ii).ok()?;
    let result = (|| {
        if ii.hbmColor.is_invalid() {
            return None;
        }
        let mut bm = BITMAP::default();
        if GetObjectW(ii.hbmColor, std::mem::size_of::<BITMAP>() as i32, Some(&mut bm as *mut BITMAP as *mut _)) == 0 {
            return None;
        }
        let (w, h) = (bm.bmWidth, bm.bmHeight);
        let mut bi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let dc = CreateCompatibleDC(HDC::default());
        let lines = GetDIBits(dc, ii.hbmColor, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
        let _ = DeleteDC(dc);
        if lines == 0 {
            return None;
        }
        bgra_to_rgba(&mut buf);
        encode_png(&buf, w as u32, h as u32)
    })();
    if !ii.hbmColor.is_invalid() {
        let _ = DeleteObject(ii.hbmColor);
    }
    if !ii.hbmMask.is_invalid() {
        let _ = DeleteObject(ii.hbmMask);
    }
    result
}

unsafe fn load_icon(path: &str) -> Option<String> {
    let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
    let mut info = SHFILEINFOW::default();
    let r = SHGetFileInfoW(
        PCWSTR(wide.as_ptr()),
        FILE_FLAGS_AND_ATTRIBUTES(0),
        Some(&mut info),
        std::mem::size_of::<SHFILEINFOW>() as u32,
        SHGFI_ICON | SHGFI_LARGEICON,
    );
    if r == 0 || info.hIcon.is_invalid() {
        return None;
    }
    let png = hicon_to_png(info.hIcon);
    let _ = DestroyIcon(info.hIcon);
    png.map(|bytes| format!("data:image/png;base64,{}", STANDARD.encode(bytes)))
}

/// Numele exe-ului unui proces („ShellExperienceHost.exe”), sau gol dacă nu poate fi citit.
pub fn exe_name(pid: u32) -> String {
    unsafe { exe_path(pid) }.and_then(|p| p.rsplit('\\').next().map(str::to_string)).unwrap_or_default()
}

fn icon_cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Toate ferestrele de aplicații, câte una pe fereastră (fără gruparea pe exe).
pub fn collect_all_windows() -> Vec<AppWindow> {
    let mut out = Vec::new();
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let own = GetCurrentProcessId();
        for h in top_level_windows() {
            if !IsWindowVisible(h).as_bool() {
                continue;
            }
            let (c, pid) = candidate(h, own);
            if !is_app_window(&c) {
                continue;
            }
            let path = exe_path(pid).unwrap_or_default();
            let exe = path.rsplit('\\').next().unwrap_or("").to_string();
            let icon = if path.is_empty() {
                None
            } else {
                let mut cache = icon_cache().lock().unwrap();
                cache.entry(path.clone()).or_insert_with(|| load_icon(&path)).clone()
            };
            out.push(AppWindow { hwnd: h.0 as isize, title: c.title, exe, icon });
        }
    }
    out
}

pub fn collect_windows() -> Vec<AppWindow> {
    dedupe_by_exe(collect_all_windows())
}

#[tauri::command]
pub async fn list_windows() -> Vec<AppWindow> {
    tauri::async_runtime::spawn_blocking(collect_windows).await.unwrap_or_default()
}

#[tauri::command]
pub fn focus_window(hwnd: isize) -> Result<(), String> {
    unsafe {
        let h = HWND(hwnd as *mut core::ffi::c_void);
        if !IsWindow(h).as_bool() {
            return Err("fereastra nu mai există".into());
        }
        if IsIconic(h).as_bool() {
            let _ = ShowWindow(h, SW_RESTORE);
        }
        // Windows permite SetForegroundWindow doar procesului din prim-plan;
        // ne atașăm temporar la input-ul lui ca să primim permisiunea.
        let fg = GetForegroundWindow();
        let fg_tid = GetWindowThreadProcessId(fg, None);
        let me = GetCurrentThreadId();
        let attached = fg_tid != 0 && fg_tid != me && AttachThreadInput(me, fg_tid, true).as_bool();
        let ok = SetForegroundWindow(h).as_bool();
        let _ = BringWindowToTop(h);
        if attached {
            let _ = AttachThreadInput(me, fg_tid, false);
        }
        if ok { Ok(()) } else { Err("Windows a refuzat aducerea ferestrei în față".into()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(title: &str) -> Candidate {
        Candidate { visible: true, has_owner: false, title: title.into(), ex_style: 0, cloaked: false, is_self: false }
    }

    fn win(hwnd: isize, exe: &str) -> AppWindow {
        AppWindow { hwnd, title: format!("w{hwnd}"), exe: exe.into(), icon: None }
    }

    #[test]
    fn filtru_ferestre() {
        assert!(is_app_window(&cand("Document - Word")));
        assert!(!is_app_window(&cand("")));
        assert!(!is_app_window(&cand("Program Manager")));
        assert!(!is_app_window(&Candidate { visible: false, ..cand("x") }));
        assert!(!is_app_window(&Candidate { has_owner: true, ..cand("x") }));
        assert!(!is_app_window(&Candidate { cloaked: true, ..cand("x") }));
        assert!(!is_app_window(&Candidate { is_self: true, ..cand("x") }));
        assert!(!is_app_window(&Candidate { ex_style: WS_EX_TOOLWINDOW.0, ..cand("x") }));
    }

    #[test]
    fn o_iconita_pe_aplicatie_dar_uwp_separat() {
        let out = dedupe_by_exe(vec![
            win(1, "chrome.exe"), win(2, "Chrome.exe"), win(3, "code.exe"),
            win(4, "ApplicationFrameHost.exe"), win(5, "ApplicationFrameHost.exe"),
        ]);
        let ids: Vec<isize> = out.iter().map(|w| w.hwnd).collect();
        assert_eq!(ids, vec![1, 3, 4, 5]);
    }

    #[test]
    fn bgra_in_rgba() {
        let mut px = vec![1, 2, 3, 200, 10, 20, 30, 0];
        bgra_to_rgba(&mut px);
        assert_eq!(px, vec![3, 2, 1, 200, 30, 20, 10, 0]);
        let mut opaque = vec![1, 2, 3, 0];
        bgra_to_rgba(&mut opaque);
        assert_eq!(opaque, vec![3, 2, 1, 255]); // fără canal alfa → opac
    }

    #[test]
    fn png_valid() {
        let png = encode_png(&[255, 0, 0, 255], 1, 1).unwrap();
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]);
    }
}
