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
    let items = app.state::<ShelfState>().items.lock().unwrap().clone();
    tauri::async_runtime::spawn_blocking(move || {
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

/// Deschide fișierul cu aplicația implicită.
#[tauri::command]
pub fn shelf_open(path: String) -> Result<(), String> {
    std::process::Command::new("explorer").arg(&path).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Arată fișierul selectat în Explorer.
#[tauri::command]
pub fn shelf_reveal(path: String) -> Result<(), String> {
    std::process::Command::new("explorer").arg(format!("/select,{path}")).spawn().map(|_| ()).map_err(|e| e.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
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
