use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::settings::SettingsState;

/// Programele de captură ale Windows (Win+Shift+S / Print Screen / Instrumentul de decupare).
const CAPTURE_TOOLS: &[&str] = &["ScreenClippingHost.exe", "SnippingTool.exe", "ScreenSketch.exe"];

pub fn is_capture_tool(exe: &str) -> bool {
    CAPTURE_TOOLS.iter().any(|t| t.eq_ignore_ascii_case(exe))
}

fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}

/// Imaginea din clipboard (`CF_DIB`: BITMAPINFOHEADER + pixeli) → (lățime, înălțime, RGBA).
/// Doar 24/32 biți, BI_RGB sau BI_BITFIELDS (ce produc instrumentele de captură).
pub fn dib_to_rgba(b: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let header = u32_at(b, 0)? as usize;
    let w = u32_at(b, 4)? as i32;
    let h = u32_at(b, 8)? as i32;
    let bpp = u16::from_le_bytes(b.get(14..16)?.try_into().ok()?) as usize;
    let compression = u32_at(b, 16)?;
    if header < 40 || w <= 0 || h == 0 || !(bpp == 24 || bpp == 32) || !(compression == 0 || compression == 3) {
        return None;
    }
    let (w, rows) = (w as usize, h.unsigned_abs() as usize);
    if w > 16384 || rows > 16384 {
        return None;
    }
    // la BITMAPINFOHEADER simplu, cele 3 măști BI_BITFIELDS urmează după antet
    let offset = header + if compression == 3 && header == 40 { 12 } else { 0 };
    let stride = (w * bpp).div_ceil(32) * 4;
    let data = b.get(offset..offset + stride * rows)?;
    let px = bpp / 8;
    let mut out = vec![0u8; w * rows * 4];
    for y in 0..rows {
        let src = if h > 0 { rows - 1 - y } else { y }; // pozitiv = rândurile de jos în sus
        let row = &data[src * stride..src * stride + w * px];
        for x in 0..w {
            let s = &row[x * px..x * px + px];
            let d = &mut out[(y * w + x) * 4..(y * w + x) * 4 + 4];
            d.copy_from_slice(&[s[2], s[1], s[0], if px == 4 { s[3] } else { 255 }]);
        }
    }
    // 32 biți fără canal alfa folosit: imaginea e opacă
    if px == 4 && out.chunks_exact(4).all(|p| p[3] == 0) {
        out.chunks_exact_mut(4).for_each(|p| p[3] = 255);
    }
    Some((w as u32, rows as u32, out))
}

/// Mărimea miniaturii, cu proporțiile păstrate și fără mărire.
pub fn thumb_size(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    let s = (max_w as f64 / w as f64).min(max_h as f64 / h as f64).min(1.0);
    (((w as f64 * s).round() as u32).max(1), ((h as f64 * s).round() as u32).max(1))
}

/// Micșorare prin media blocurilor (suficient pentru o miniatură).
fn downscale(rgba: &[u8], w: u32, h: u32, tw: u32, th: u32) -> Vec<u8> {
    let (w, h, tw, th) = (w as usize, h as usize, tw as usize, th as usize);
    let mut out = vec![0u8; tw * th * 4];
    for ty in 0..th {
        let (y0, y1) = (ty * h / th, ((ty + 1) * h / th).max(ty * h / th + 1));
        for tx in 0..tw {
            let (x0, x1) = (tx * w / tw, ((tx + 1) * w / tw).max(tx * w / tw + 1));
            let mut acc = [0u32; 4];
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = &rgba[(y * w + x) * 4..(y * w + x) * 4 + 4];
                    for c in 0..4 {
                        acc[c] += p[c] as u32;
                    }
                }
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            for c in 0..4 {
                out[(ty * tw + tx) * 4 + c] = (acc[c] / n) as u8;
            }
        }
    }
    out
}

/// Fișierul pe care Instrumentul de decupare l-a salvat singur pentru această captură, dacă există:
/// un .png modificat între 3 s înainte și 15 s după captură, cel mai apropiat de ea.
pub fn find_recent(files: &[(PathBuf, u64)], captured_ms: u64) -> Option<PathBuf> {
    files
        .iter()
        .filter(|(p, _)| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")))
        .filter(|(_, m)| *m + 3_000 >= captured_ms && *m <= captured_ms + 15_000)
        .min_by_key(|(_, m)| m.abs_diff(captured_ms))
        .map(|(p, _)| p.clone())
}

/// Un nume liber în folder: `nume.png`, apoi `nume (2).png`…
pub fn unique_name(dir: &Path, base: &str, exists: impl Fn(&Path) -> bool) -> PathBuf {
    let first = dir.join(format!("{base}.png"));
    if !exists(&first) {
        return first;
    }
    (2..).map(|i| dir.join(format!("{base} ({i}).png"))).find(|p| !exists(p)).unwrap()
}

struct Shot {
    id: u64,
    w: u32,
    h: u32,
    rgba: Vec<u8>,
    at_ms: u64,
    saved: Option<PathBuf>,
}

/// Doar ultima captură rămâne în memorie.
#[derive(Default)]
pub struct ScreenshotState(Mutex<Option<Shot>>);

#[derive(Serialize, Clone)]
struct ShotEvent {
    id: u64,
    thumb: String,
    width: u32,
    height: u32,
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

mod win {
    use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, GetClipboardData, GetClipboardOwner, GetClipboardSequenceNumber, IsClipboardFormatAvailable,
        OpenClipboard,
    };
    use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    const CF_DIB: u32 = 8;

    pub fn sequence() -> u32 {
        unsafe { GetClipboardSequenceNumber() }
    }

    pub fn has_image() -> bool {
        unsafe { IsClipboardFormatAvailable(CF_DIB).is_ok() }
    }

    pub fn owner_exe() -> String {
        unsafe {
            let Ok(h) = GetClipboardOwner() else { return String::new() };
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, Some(&mut pid));
            crate::windows_list::exe_name(pid)
        }
    }

    /// Octeții `CF_DIB` din clipboard (clipboard-ul poate fi ocupat o clipă de alt program: reîncercăm).
    pub fn read_dib() -> Option<Vec<u8>> {
        unsafe {
            let mut opened = false;
            for _ in 0..10 {
                if OpenClipboard(HWND::default()).is_ok() {
                    opened = true;
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(30));
            }
            if !opened {
                return None;
            }
            let bytes = (|| {
                let h: HANDLE = GetClipboardData(CF_DIB).ok()?;
                let g = HGLOBAL(h.0);
                let ptr = GlobalLock(g) as *const u8;
                if ptr.is_null() {
                    return None;
                }
                let v = std::slice::from_raw_parts(ptr, GlobalSize(g)).to_vec();
                let _ = GlobalUnlock(g);
                Some(v)
            })();
            let _ = CloseClipboard();
            bytes
        }
    }
}

/// Urmărește clipboard-ul (numărul de secvență, la 400 ms) și emite `screenshot` pentru capturile de ecran.
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut seq = win::sequence();
        let mut next_id = 1u64;
        loop {
            std::thread::sleep(Duration::from_millis(400));
            let s = win::sequence();
            if s == seq {
                continue;
            }
            seq = s;
            if !app.state::<SettingsState>().get().modules.screenshots || !win::has_image() {
                continue;
            }
            let owner = win::owner_exe();
            if !is_capture_tool(&owner) {
                log::debug!("clipboard: imagine de la „{owner}”, nu e o captură de ecran");
                continue;
            }
            let Some((w, h, rgba)) = win::read_dib().as_deref().and_then(dib_to_rgba) else {
                log::warn!("captură: nu pot citi imaginea din clipboard");
                continue;
            };
            let (tw, th) = thumb_size(w, h, 240, 135);
            let Some(png) = crate::windows_list::encode_png(&downscale(&rgba, w, h, tw, th), tw, th) else { continue };
            let id = next_id;
            next_id += 1;
            *app.state::<ScreenshotState>().0.lock().unwrap() = Some(Shot { id, w, h, rgba, at_ms: now_ms(), saved: None });
            let thumb = format!("data:image/png;base64,{}", STANDARD.encode(png));
            let _ = app.emit("screenshot", ShotEvent { id, thumb, width: w, height: h });
        }
    });
}

fn screenshots_dir() -> Option<PathBuf> {
    use windows::Win32::UI::Shell::{FOLDERID_Screenshots, SHGetKnownFolderPath, KF_FLAG_CREATE};
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Screenshots, KF_FLAG_CREATE, None).ok()?;
        let s = p.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from)
    }
}

fn local_stamp() -> String {
    let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
    format!("{:04}-{:02}-{:02} {:02}{:02}{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
}

fn modified_ms(p: &Path) -> Option<u64> {
    let m = std::fs::metadata(p).ok()?.modified().ok()?;
    Some(m.duration_since(UNIX_EPOCH).ok()?.as_millis() as u64)
}

/// Fișierul capturii: cel salvat deja de Instrumentul de decupare sau unul nou scris de noi.
fn save_shot(app: &AppHandle, id: u64) -> Result<PathBuf, String> {
    let state = app.state::<ScreenshotState>();
    let mut guard = state.0.lock().unwrap();
    let shot = guard.as_mut().filter(|s| s.id == id).ok_or("captura nu mai e disponibilă")?;
    if let Some(p) = shot.saved.as_ref().filter(|p| p.exists()) {
        return Ok(p.clone());
    }
    let dir = screenshots_dir().ok_or("folderul Capturi de ecran nu e disponibil")?;
    let files: Vec<(PathBuf, u64)> = std::fs::read_dir(&dir)
        .map(|rd| rd.flatten().filter_map(|e| Some((e.path(), modified_ms(&e.path())?))).collect())
        .unwrap_or_default();
    let path = match find_recent(&files, shot.at_ms) {
        Some(p) => p,
        None => {
            let png = crate::windows_list::encode_png(&shot.rgba, shot.w, shot.h).ok_or("nu pot coda imaginea")?;
            let path = unique_name(&dir, &format!("{} {}", crate::i18n::t("shot.file"), local_stamp()), |p| p.exists());
            std::fs::write(&path, png).map_err(|e| e.to_string())?;
            path
        }
    };
    shot.saved = Some(path.clone());
    Ok(path)
}

#[tauri::command]
pub async fn screenshot_save(app: AppHandle, id: u64) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || save_shot(&app, id).map(|p| p.to_string_lossy().into_owned()))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn screenshot_to_shelf(app: AppHandle, id: u64) -> Result<String, String> {
    let path = screenshot_save(app.clone(), id).await?;
    crate::shelf::shelf_add_paths(app, vec![path.clone()])?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(w: i32, h: i32, bpp: u16, compression: u32) -> Vec<u8> {
        let mut b = vec![0u8; 40];
        b[0..4].copy_from_slice(&40u32.to_le_bytes());
        b[4..8].copy_from_slice(&w.to_le_bytes());
        b[8..12].copy_from_slice(&h.to_le_bytes());
        b[12..14].copy_from_slice(&1u16.to_le_bytes());
        b[14..16].copy_from_slice(&bpp.to_le_bytes());
        b[16..20].copy_from_slice(&compression.to_le_bytes());
        b
    }

    #[test]
    fn dib_24_biti_de_jos_in_sus_cu_umplutura() {
        // 2×2, rândul are 6 octeți + 2 de umplutură; primul rând din fișier e cel de jos
        let mut b = header(2, 2, 24, 0);
        b.extend([0, 0, 255, 0, 255, 0, 0, 0]); // jos: roșu, verde
        b.extend([255, 0, 0, 255, 255, 255, 0, 0]); // sus: albastru, alb
        let (w, h, px) = dib_to_rgba(&b).unwrap();
        assert_eq!((w, h), (2, 2));
        assert_eq!(&px[0..4], &[0, 0, 255, 255]); // sus-stânga: albastru
        assert_eq!(&px[4..8], &[255, 255, 255, 255]);
        assert_eq!(&px[8..12], &[255, 0, 0, 255]); // jos-stânga: roșu
    }

    #[test]
    fn dib_32_biti_de_sus_in_jos_fara_alfa_devine_opac() {
        let mut b = header(1, -2, 32, 3);
        b.extend([0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0, 0]); // cele 3 măști BI_BITFIELDS
        b.extend([10, 20, 30, 0, 40, 50, 60, 0]);
        let (_, _, px) = dib_to_rgba(&b).unwrap();
        assert_eq!(px, vec![30, 20, 10, 255, 60, 50, 40, 255]);
    }

    #[test]
    fn dib_invalid_sau_trunchiat() {
        assert!(dib_to_rgba(&[]).is_none());
        assert!(dib_to_rgba(&header(2, 2, 8, 0)).is_none()); // paletă: nu e captură
        assert!(dib_to_rgba(&header(100, 100, 24, 0)).is_none()); // lipsesc pixelii
    }

    #[test]
    fn miniatura_pastreaza_proportiile() {
        assert_eq!(thumb_size(1920, 1080, 240, 135), (240, 135));
        assert_eq!(thumb_size(1000, 2000, 240, 135), (68, 135));
        assert_eq!(thumb_size(100, 50, 240, 135), (100, 50)); // fără mărire
    }

    #[test]
    fn micsorare_prin_medie() {
        let px = [0, 0, 0, 255, 200, 100, 50, 255];
        assert_eq!(downscale(&px, 2, 1, 1, 1), vec![100, 50, 25, 255]);
    }

    #[test]
    fn fisierul_salvat_deja_de_instrumentul_de_decupare() {
        let f = |n: &str, m: u64| (PathBuf::from(n), m);
        let files = vec![f("vechi.png", 1_000), f("captura.png", 10_500), f("alta.jpg", 10_400), f("viitor.png", 40_000)];
        assert_eq!(find_recent(&files, 10_000), Some(PathBuf::from("captura.png")));
        assert_eq!(find_recent(&files, 100_000), None);
    }

    #[test]
    fn nume_liber_in_folder() {
        let dir = Path::new("C:\\S");
        let taken = [dir.join("Captură.png"), dir.join("Captură (2).png")];
        assert_eq!(unique_name(dir, "Captură", |p| taken.iter().any(|t| t == p)), dir.join("Captură (3).png"));
        assert_eq!(unique_name(dir, "Nou", |_| false), dir.join("Nou.png"));
    }

    #[test]
    fn doar_instrumentele_de_captura() {
        assert!(is_capture_tool("ScreenClippingHost.exe"));
        assert!(is_capture_tool("snippingtool.exe"));
        assert!(!is_capture_tool("chrome.exe"));
        assert!(!is_capture_tool(""));
    }
}
