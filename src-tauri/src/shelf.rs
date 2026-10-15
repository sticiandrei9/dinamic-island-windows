use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

/// Un fișier sau folder ținut pe raft (doar referința, nu o copie).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ShelfItem {
    pub path: String,
    pub name: String,
    pub added_at: u64,
}

/// Ce vede interfața: elementul + dacă mai există pe disc + iconița.
#[derive(Serialize, Clone)]
pub struct ShelfView {
    pub path: String,
    pub name: String,
    pub exists: bool,
    pub icon: Option<String>,
}

const MAX_ITEMS: usize = 30;

fn display_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['\\', '/']);
    trimmed.rsplit(['\\', '/']).next().unwrap_or(trimmed).to_string()
}

/// Fișierele trase intră în față, în ordinea în care au fost trase; o cale deja prezentă
/// urcă în față; peste 30 se taie de la coadă (cele mai vechi).
pub fn shelf_add(list: Vec<ShelfItem>, paths: &[String], now: u64) -> Vec<ShelfItem> {
    let mut batch: Vec<ShelfItem> = Vec::new();
    for p in paths {
        if !batch.iter().any(|i| i.path.eq_ignore_ascii_case(p)) {
            batch.push(ShelfItem { path: p.clone(), name: display_name(p), added_at: now });
        }
    }
    let rest: Vec<ShelfItem> =
        list.into_iter().filter(|i| !batch.iter().any(|b| b.path.eq_ignore_ascii_case(&i.path))).collect();
    let mut out = batch;
    out.extend(rest);
    out.truncate(MAX_ITEMS);
    out
}

pub fn shelf_remove(list: Vec<ShelfItem>, path: &str) -> Vec<ShelfItem> {
    list.into_iter().filter(|i| !i.path.eq_ignore_ascii_case(path)).collect()
}

/// Scoate fișierele puse pe raft acum mai mult de `days` zile (0 = le păstrează).
pub fn shelf_prune(list: Vec<ShelfItem>, days: u32, now: u64) -> Vec<ShelfItem> {
    if days == 0 {
        return list;
    }
    list.into_iter().filter(|i| now.saturating_sub(i.added_at) < days as u64 * 86_400).collect()
}

pub fn load(path: &Path) -> Vec<ShelfItem> {
    std::fs::read_to_string(path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

pub fn save(path: &Path, list: &[ShelfItem]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(list).map_err(std::io::Error::other)?)?;
    std::fs::rename(tmp, path)
}

pub struct ShelfState {
    pub items: Mutex<Vec<ShelfItem>>,
    pub path: PathBuf,
}

impl ShelfState {
    pub fn load(path: PathBuf) -> Self {
        Self { items: Mutex::new(load(&path)), path }
    }
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn update(app: &AppHandle, f: impl FnOnce(Vec<ShelfItem>) -> Vec<ShelfItem>) -> Result<(), String> {
    let state = app.state::<ShelfState>();
    let mut items = state.items.lock().unwrap();
    *items = f(std::mem::take(&mut *items));
    save(&state.path, &items).map_err(|e| e.to_string())?;
    drop(items);
    let _ = app.emit("shelf-changed", ());
    Ok(())
}

#[tauri::command]
pub async fn shelf_list(app: AppHandle) -> Vec<ShelfView> {
    let days = app.state::<crate::settings::SettingsState>().get().shelf_days;
    let state = app.state::<ShelfState>();
    let stale = {
        let items = state.items.lock().unwrap();
        shelf_prune(items.clone(), days, now_secs()).len() != items.len()
    };
    if stale {
        let _ = update(&app, |l| shelf_prune(l, days, now_secs()));
    }
    let items = app.state::<ShelfState>().items.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || {
        // SHGetFileInfo (iconițele) cere COM pe thread
        let _com = crate::platform::ComGuard::new(windows::Win32::System::Com::COINIT_APARTMENTTHREADED);
        items
            .into_iter()
            .map(|i| {
                let exists = Path::new(&i.path).exists();
                let icon = if exists { crate::windows_list::file_icon(&i.path) } else { None };
                ShelfView { path: i.path, name: i.name, exists, icon }
            })
            .collect()
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
pub fn shelf_add_paths(app: AppHandle, paths: Vec<String>) -> Result<(), String> {
    update(&app, |l| shelf_add(l, &paths, now_secs()))
}

#[tauri::command]
pub fn shelf_remove_path(app: AppHandle, path: String) -> Result<(), String> {
    update(&app, |l| shelf_remove(l, &path))
}

#[tauri::command]
pub fn shelf_clear(app: AppHandle) -> Result<(), String> {
    update(&app, |_| Vec::new())
}

/// Pune calea fișierului în clipboard, ca text.
#[tauri::command]
pub fn shelf_copy_path(path: String) -> Result<(), String> {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData};
    use windows::Win32::Foundation::{GlobalFree, HGLOBAL};
    use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    let text: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    unsafe {
        // alt program (un manager de clipboard, RDP) îl poate ține deschis o clipă
        let mut opened = OpenClipboard(None);
        for _ in 0..10 {
            if opened.is_ok() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            opened = OpenClipboard(None);
        }
        opened.map_err(|e| e.to_string())?;
        let result = (|| {
            EmptyClipboard()?;
            let mem: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, text.len() * 2)?;
            let dst = GlobalLock(mem) as *mut u16;
            if dst.is_null() {
                let _ = GlobalFree(mem);
                return Err(windows::core::Error::from_win32());
            }
            std::ptr::copy_nonoverlapping(text.as_ptr(), dst, text.len());
            let _ = GlobalUnlock(mem);
            // CF_UNICODETEXT; după succes memoria e a clipboard-ului, altfel o eliberăm noi
            SetClipboardData(13, HANDLE(mem.0)).map(|_| ()).inspect_err(|_| {
                let _ = GlobalFree(mem);
            })
        })();
        let _ = CloseClipboard();
        result.map_err(|e| e.to_string())
    }
}

/// Deschide fișierul cu aplicația implicită.
#[tauri::command]
pub fn shelf_open(path: String) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("explorer").raw_arg(open_arg(&path)).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Calea între ghilimele (Command le pune doar când calea are spații; explorer desparte și la virgulă).
pub fn open_arg(path: &str) -> String {
    format!("\"{path}\"")
}

/// Arată fișierul selectat în Explorer.
#[tauri::command]
pub fn shelf_reveal(path: String) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    // explorer nu înțelege ghilimelele puse automat de Command; o cale cu virgulă trebuie pusă între ghilimele de noi
    std::process::Command::new("explorer").raw_arg(select_arg(&path)).spawn().map(|_| ()).map_err(|e| e.to_string())
}

pub fn select_arg(path: &str) -> String {
    format!("/select,\"{path}\"")
}

/// Trage fișierul de pe raft în altă aplicație (drag nativ OLE, pe thread-ul principal).
#[tauri::command]
pub fn shelf_drag_out(window: WebviewWindow, path: String) -> Result<(), String> {
    let png = crate::windows_list::file_icon_png(&path).unwrap_or_default();
    let image = if png.is_empty() { drag::Image::Raw(Vec::new()) } else { drag::Image::Raw(png) };
    log::debug!("raft: încep tragerea în afară");
    drag::start_drag(
        &window,
        drag::DragItem::Files(vec![PathBuf::from(&path)]),
        image,
        |result, _| log::debug!("raft: tragere terminată: {result:?}"),
        drag::Options::default(),
    )
    .map_err(|e| e.to_string())
}

// Tragerea fișierelor PE insulă. wry pune ținta de drop doar pe ferestrele copil existente la crearea
// webview-ului; WebView2 își face ferestrele (în procesul lui) mai târziu, iar OLE nu urcă până la ținta
// lui wry, deci drop-ul nu ajungea nicăieri. Punem noi ținta pe ferestrele care n-au una.

#[derive(Serialize, Clone)]
struct FileDrop {
    #[serde(rename = "type")]
    kind: &'static str,
    paths: Vec<String>,
}

#[windows::core::implement(windows::Win32::System::Ole::IDropTarget)]
struct DropTarget {
    app: AppHandle,
    label: String,
    effect: std::cell::Cell<windows::Win32::System::Ole::DROPEFFECT>,
}

impl DropTarget {
    fn emit(&self, kind: &'static str, paths: Vec<String>) {
        let _ = self.app.emit_to(self.label.as_str(), "file-drop", FileDrop { kind, paths });
    }
}

/// Căile fișierelor din obiectul tras (CF_HDROP); gol dacă nu sunt fișiere.
fn dropped_paths(data: Option<&windows::Win32::System::Com::IDataObject>) -> Vec<String> {
    use windows::Win32::System::Com::{DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL};
    use windows::Win32::System::Ole::{ReleaseStgMedium, CF_HDROP};
    use windows::Win32::UI::Shell::{DragQueryFileW, HDROP};
    let Some(data) = data else { return Vec::new() };
    let fmt = FORMATETC { cfFormat: CF_HDROP.0, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: -1, tymed: TYMED_HGLOBAL.0 as u32 };
    let Ok(mut medium) = (unsafe { data.GetData(&fmt) }) else { return Vec::new() };
    let hdrop = HDROP(unsafe { medium.u.hGlobal.0 });
    let n = unsafe { DragQueryFileW(hdrop, u32::MAX, None) };
    let paths = (0..n)
        .map(|i| {
            let len = unsafe { DragQueryFileW(hdrop, i, None) } as usize;
            let mut buf = vec![0u16; len + 1];
            unsafe { DragQueryFileW(hdrop, i, Some(&mut buf)) };
            String::from_utf16_lossy(&buf[..len])
        })
        .collect();
    unsafe { ReleaseStgMedium(&mut medium) };
    paths
}

#[allow(non_snake_case)]
impl windows::Win32::System::Ole::IDropTarget_Impl for DropTarget_Impl {
    fn DragEnter(
        &self,
        data: Option<&windows::Win32::System::Com::IDataObject>,
        _keys: windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS,
        _pt: &windows::Win32::Foundation::POINTL,
        effect: *mut windows::Win32::System::Ole::DROPEFFECT,
    ) -> windows::core::Result<()> {
        use windows::Win32::System::Ole::{DROPEFFECT_COPY, DROPEFFECT_NONE};
        let paths = dropped_paths(data);
        let on = !paths.is_empty() && self.app.state::<crate::settings::SettingsState>().get().modules.shelf;
        self.effect.set(if on { DROPEFFECT_COPY } else { DROPEFFECT_NONE });
        unsafe { *effect = self.effect.get() };
        if on {
            self.emit("enter", paths);
        }
        Ok(())
    }

    fn DragOver(
        &self,
        _keys: windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS,
        _pt: &windows::Win32::Foundation::POINTL,
        effect: *mut windows::Win32::System::Ole::DROPEFFECT,
    ) -> windows::core::Result<()> {
        unsafe { *effect = self.effect.get() };
        Ok(())
    }

    fn DragLeave(&self) -> windows::core::Result<()> {
        if self.effect.get() != windows::Win32::System::Ole::DROPEFFECT_NONE {
            self.emit("leave", Vec::new());
        }
        Ok(())
    }

    fn Drop(
        &self,
        data: Option<&windows::Win32::System::Com::IDataObject>,
        _keys: windows::Win32::System::SystemServices::MODIFIERKEYS_FLAGS,
        _pt: &windows::Win32::Foundation::POINTL,
        effect: *mut windows::Win32::System::Ole::DROPEFFECT,
    ) -> windows::core::Result<()> {
        unsafe { *effect = self.effect.get() };
        if self.effect.get() != windows::Win32::System::Ole::DROPEFFECT_NONE {
            self.emit("drop", dropped_paths(data));
        }
        Ok(())
    }
}

/// Pune ținta de drop pe ferestrele insulei care n-au una (apelat când cursorul intră pe insulă,
/// deci și după ce WebView2 și-a refăcut ferestrele). Pe thread-ul principal, unde OLE e inițializat.
pub fn ensure_drop_target(win: &WebviewWindow) {
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{EnumChildWindows, GetPropW};
    let Some(root) = crate::platform::hwnd_of(win) else { return };
    let (root, app, label) = (root.0 as isize, win.app_handle().clone(), win.label().to_string());
    let _ = win.run_on_main_thread(move || {
        let mut children: Vec<HWND> = Vec::new();
        unsafe {
            let _ = EnumChildWindows(HWND(root as _), Some(crate::platform::collect_hwnd), LPARAM(&mut children as *mut Vec<HWND> as isize));
        }
        for hwnd in children {
            if !unsafe { GetPropW(hwnd, windows::core::w!("OleDropTargetInterface")) }.0.is_null() {
                continue;
            }
            let target: windows::Win32::System::Ole::IDropTarget =
                DropTarget { app: app.clone(), label: label.clone(), effect: Default::default() }.into();
            if let Err(e) = unsafe { windows::Win32::System::Ole::RegisterDragDrop(hwnd, &target) } {
                log::debug!("raft: ținta de drop n-a putut fi pusă pe o fereastră: {e}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn selectie_in_explorer_cu_virgula_in_cale() {
        assert_eq!(select_arg(r"C:\Poze\Vacanță, 2026\a.jpg"), r#"/select,"C:\Poze\Vacanță, 2026\a.jpg""#);
        // deschiderea: și fără spații, virgula ar despărți calea
        assert_eq!(open_arg(r"C:\x\a,b.pdf"), r#""C:\x\a,b.pdf""#);
    }

    #[test]
    fn adauga_cele_noi_primele_fara_duplicate() {
        let list = shelf_add(Vec::new(), &paths(&[r"C:\a.txt", r"C:\b.png"]), 10);
        let list = shelf_add(list, &paths(&[r"C:\a.txt", r"C:\c.pdf"]), 20);
        let names: Vec<&str> = list.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, vec!["a.txt", "c.pdf", "b.png"]); // a.txt re-adăugat urcă primul
        assert_eq!(list[0].added_at, 20);
    }

    #[test]
    fn cel_mult_30_de_elemente() {
        let many: Vec<String> = (0..40).map(|i| format!(r"C:\f{i}.txt")).collect();
        let list = shelf_add(Vec::new(), &many, 1);
        assert_eq!(list.len(), 30);
        assert_eq!(list[0].name, "f0.txt"); // ordinea în care au fost trase se păstrează
        let list = shelf_add(list, &[r"C:\nou.txt".to_string()], 2);
        assert_eq!((list.len(), list[0].name.as_str(), list[29].name.as_str()), (30, "nou.txt", "f28.txt")); // cel mai vechi iese
    }

    #[test]
    fn golire_automata_dupa_zile() {
        let day = 86_400;
        let list = shelf_add(Vec::new(), &paths(&[r"C:\vechi.txt"]), 0);
        let list = shelf_add(list, &paths(&[r"C:\nou.txt"]), 6 * day);
        assert_eq!(shelf_prune(list.clone(), 0, 100 * day).len(), 2); // 0 = niciodată
        let kept = shelf_prune(list.clone(), 7, 7 * day);
        assert_eq!(kept.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(), vec!["nou.txt"]);
        assert_eq!(shelf_prune(list, 7, 7 * day - 1).len(), 2);
    }

    #[test]
    fn scoate_de_pe_raft() {
        let list = shelf_add(Vec::new(), &paths(&[r"C:\a.txt", r"C:\b.txt"]), 1);
        let list = shelf_remove(list, r"C:\a.txt");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "b.txt");
    }

    #[test]
    fn fisier_corupt_da_raft_gol() {
        let d = std::env::temp_dir().join(format!("di-shelf-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("shelf.json");
        std::fs::write(&p, "nu e json").unwrap();
        assert!(load(&p).is_empty());
        let list = shelf_add(Vec::new(), &paths(&[r"C:\x.txt"]), 5);
        save(&p, &list).unwrap();
        assert_eq!(load(&p), list);
    }

    #[test]
    fn numele_unui_folder() {
        let list = shelf_add(Vec::new(), &paths(&[r"C:\Users\A\Proiect\"]), 1);
        assert_eq!(list[0].name, "Proiect");
    }
}
