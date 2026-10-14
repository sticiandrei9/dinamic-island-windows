use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::platform;

/// Dreptunghiul insulei, în CSS px relativ la fereastră (raportat de JS la fiecare cadru de animație).
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Default)]
pub struct IslandRect(pub Arc<Mutex<Rect>>);

pub fn point_in_rect(px: f64, py: f64, r: &Rect, margin: f64) -> bool {
    px >= r.x - margin && px <= r.x + r.w + margin && py >= r.y - margin && py <= r.y + r.h + margin
}

#[tauri::command]
pub fn set_island_rect(state: tauri::State<IslandRect>, rect: Rect) {
    *state.0.lock().unwrap() = rect;
}

/// Thread care: (1) face fereastra transparentă la click în afara insulei,
/// (2) trimite `island-hover` către JS, (3) ascunde insula (`island-visible`) când o aplicație e pe tot ecranul.
/// Fereastra nu e ascunsă cu adevărat: re-afișarea ar fura focusul aplicației care iese din fullscreen.
pub fn spawn(app: AppHandle) {
    let shared = app.state::<IslandRect>().0.clone();
    std::thread::spawn(move || {
        let Some(win) = app.get_webview_window("island") else { return };
        let own = platform::hwnd_of(&win);
        let mut inside_last: Option<bool> = None;
        let mut hidden = false;
        let mut geo = (0i32, 0i32, 1.0f64);
        let mut geo_at: Option<Instant> = None;
        let mut fs_at = Instant::now();

        loop {
            std::thread::sleep(Duration::from_millis(30));

            if geo_at.map_or(true, |t| t.elapsed() > Duration::from_secs(1)) {
                if let (Ok(p), Ok(s)) = (win.outer_position(), win.scale_factor()) {
                    geo = (p.x, p.y, s);
                }
                geo_at = Some(Instant::now());
            }

            if fs_at.elapsed() > Duration::from_millis(500) {
                fs_at = Instant::now();
                let fs = platform::foreground_is_fullscreen(own);
                if fs != hidden {
                    hidden = fs;
                    let _ = app.emit("island-visible", !fs);
                    if fs {
                        let _ = win.set_ignore_cursor_events(true);
                        inside_last = Some(false);
                    }
                }
            }
            if hidden {
                continue;
            }

            let Some((cx, cy)) = platform::cursor_pos() else { continue };
            let rect = *shared.lock().unwrap();
            let px = (cx - geo.0) as f64 / geo.2;
            let py = (cy - geo.1) as f64 / geo.2;
            let inside = point_in_rect(px, py, &rect, 1.0);
            if inside_last != Some(inside) {
                inside_last = Some(inside);
                let _ = win.set_ignore_cursor_events(!inside);
                let _ = app.emit("island-hover", inside);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punct_in_dreptunghi() {
        let r = Rect { x: 10.0, y: 0.0, w: 100.0, h: 32.0 };
        assert!(point_in_rect(10.0, 0.0, &r, 0.0));
        assert!(point_in_rect(110.0, 32.0, &r, 0.0));
        assert!(!point_in_rect(111.0, 10.0, &r, 0.0));
        assert!(point_in_rect(111.0, 10.0, &r, 1.0));
        assert!(!point_in_rect(50.0, 40.0, &r, 1.0));
    }
}
