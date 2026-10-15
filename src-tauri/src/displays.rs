use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Manager, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::platform;
use crate::settings::SettingsState;

/// Dimensiunea logică (CSS px, la mărimea „normală”) a ferestrei unei insule.
pub const WIN_W: f64 = 520.0;
pub const WIN_H: f64 = 340.0;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MonitorInfo {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub scale: f64,
    pub primary: bool,
}

/// Monitoarele pe care apare insula, după setarea `monitor`.
pub fn target_monitors(choice: &str, monitors: &[MonitorInfo]) -> Vec<MonitorInfo> {
    let primary = || monitors.iter().find(|m| m.primary).or(monitors.first()).cloned();
    match choice {
        "all" => monitors.to_vec(),
        "primary" => primary().into_iter().collect(),
        name => monitors.iter().find(|m| m.name == name).cloned().or_else(primary).into_iter().collect(),
    }
}

/// (x, y, lățime, înălțime) în px fizici: lipită sus, centrată pe monitor.
pub fn island_geometry(m: &MonitorInfo, size_scale: f64) -> (i32, i32, u32, u32) {
    let w = (WIN_W * size_scale * m.scale).round() as u32;
    let h = (WIN_H * size_scale * m.scale).round() as u32;
    (m.x + (m.width as i32 - w as i32) / 2, m.y, w, h)
}

pub fn current_monitors(app: &AppHandle) -> Vec<MonitorInfo> {
    let primary = app.primary_monitor().ok().flatten().and_then(|m| m.name().cloned());
    app.available_monitors()
        .unwrap_or_default()
        .into_iter()
        .map(|m| {
            let name = m.name().cloned().unwrap_or_default();
            MonitorInfo {
                primary: Some(&name) == primary.as_ref(),
                x: m.position().x,
                y: m.position().y,
                width: m.size().width,
                height: m.size().height,
                scale: m.scale_factor(),
                name,
            }
        })
        .collect()
}

#[tauri::command]
pub fn list_monitors(app: AppHandle) -> Vec<MonitorInfo> {
    current_monitors(&app)
}

fn create_island(app: &AppHandle, label: &str) -> tauri::Result<WebviewWindow> {
    WebviewWindowBuilder::new(app, label, WebviewUrl::App("index.html".into()))
        .title("Dynamic Island")
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .shadow(false)
        .focused(false)
        .visible(false)
        .inner_size(WIN_W, WIN_H)
        .build()
}

fn placed() -> &'static Mutex<HashMap<String, (i32, i32, u32, u32)>> {
    static P: OnceLock<Mutex<HashMap<String, (i32, i32, u32, u32)>>> = OnceLock::new();
    P.get_or_init(Default::default)
}

/// Aduce ferestrele-insulă în acord cu setările și cu monitoarele existente:
/// creează ce lipsește, închide ce e în plus, repoziționează doar ce s-a schimbat.
pub fn sync_windows(app: &AppHandle) {
    static SYNC: Mutex<()> = Mutex::new(());
    let _guard = SYNC.lock().unwrap();
    let settings = app.state::<SettingsState>().get();
    let targets = target_monitors(&settings.monitor, &current_monitors(app));
    let scale = settings.size.scale();
    let mut placed = placed().lock().unwrap();

    for (i, m) in targets.iter().enumerate() {
        let label = format!("island-{i}");
        let geom = island_geometry(m, scale);
        let (win, fresh) = match app.get_webview_window(&label) {
            Some(w) => (w, false),
            None => match create_island(app, &label) {
                Ok(w) => (w, true),
                Err(e) => {
                    log::warn!("nu pot crea {label}: {e}");
                    continue;
                }
            },
        };
        if placed.get(&label) != Some(&geom) {
            let _ = win.set_position(PhysicalPosition::new(geom.0, geom.1));
            let _ = win.set_size(PhysicalSize::new(geom.2, geom.3));
            placed.insert(label.clone(), geom);
        }
        if fresh {
            // până raportează interfața dreptunghiul insulei, fereastra nu blochează click-urile
            let _ = win.set_ignore_cursor_events(true);
            platform::make_no_activate(&win);
            let _ = platform::show_keep_focus(&win);
        }
    }

    for (label, win) in app.webview_windows() {
        let extra = label
            .strip_prefix("island-")
            .and_then(|i| i.parse::<usize>().ok())
            .is_some_and(|i| i >= targets.len());
        if extra {
            placed.remove(&label);
            app.state::<crate::hittest::IslandRects>().0.lock().unwrap().remove(&label);
            let _ = win.destroy();
        }
    }
}

/// Verifică monitoarele la fiecare 5 s (conectare/deconectare, rezoluție, scalare).
pub fn spawn_watcher(app: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(5));
        sync_windows(&app);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mon(name: &str, x: i32, primary: bool) -> MonitorInfo {
        MonitorInfo { name: name.into(), x, y: 0, width: 1920, height: 1080, scale: 1.0, primary }
    }

    fn names(v: &[MonitorInfo]) -> Vec<&str> {
        v.iter().map(|m| m.name.as_str()).collect()
    }

    #[test]
    fn principal_implicit() {
        let ms = [mon("A", -1920, false), mon("B", 0, true)];
        assert_eq!(names(&target_monitors("primary", &ms)), vec!["B"]);
    }

    #[test]
    fn toate_monitoarele() {
        let ms = [mon("A", -1920, false), mon("B", 0, true)];
        assert_eq!(names(&target_monitors("all", &ms)), vec!["A", "B"]);
    }

    #[test]
    fn monitor_anume() {
        let ms = [mon("A", -1920, false), mon("B", 0, true)];
        assert_eq!(names(&target_monitors("A", &ms)), vec!["A"]);
    }

    #[test]
    fn monitor_disparut_cade_pe_principal() {
        let ms = [mon("B", 0, true)];
        assert_eq!(names(&target_monitors("A", &ms)), vec!["B"]);
    }

    #[test]
    fn fara_principal_ia_primul() {
        let ms = [mon("A", 0, false), mon("B", 1920, false)];
        assert_eq!(names(&target_monitors("primary", &ms)), vec!["A"]);
        assert!(target_monitors("primary", &[]).is_empty());
    }

    #[test]
    fn pozitia_ferestrei_centrata_sus() {
        let m = MonitorInfo { name: "A".into(), x: 1920, y: -200, width: 2560, height: 1440, scale: 1.5, primary: false };
        // 520 logic × scara mărimii 1.15 × DPI 1.5 = 897 px fizici
        assert_eq!(island_geometry(&m, 1.15), (1920 + (2560 - 897) / 2, -200, 897, 586)); // 340 × 1,15 × 1,5 = 586,4999… în virgulă mobilă
    }
}
