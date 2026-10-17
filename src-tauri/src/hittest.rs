use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

use crate::platform;
use crate::settings::SettingsState;

/// Dreptunghiul insulei, în CSS px logici (fără scara mărimii), relativ la fereastră.
/// JS îl raportează la fiecare cadru de animație.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Dreptunghiul fiecărei insule, după label-ul ferestrei.
#[derive(Default)]
pub struct IslandRects(pub Mutex<HashMap<String, Rect>>);

pub fn point_in_rect(px: f64, py: f64, r: &Rect, margin: f64) -> bool {
    px >= r.x - margin && px <= r.x + r.w + margin && py >= r.y - margin && py <= r.y + r.h + margin
}

/// Colțul ferestrei e pe monitorul (left, top, right, bottom)?
pub fn on_monitor(pos: (i32, i32), mon: (i32, i32, i32, i32)) -> bool {
    pos.0 >= mon.0 && pos.0 < mon.2 && pos.1 >= mon.1 && pos.1 < mon.3
}

/// Câte insule sunt vizibile acum (nu ascunse de fullscreen); citit de vizualizator.
static VISIBLE: AtomicUsize = AtomicUsize::new(0);

pub fn any_island_visible() -> bool {
    VISIBLE.load(Ordering::Relaxed) > 0
}

#[tauri::command]
pub fn set_island_rect(window: WebviewWindow, state: tauri::State<IslandRects>, rect: Rect) {
    state.0.lock().unwrap().insert(window.label().to_string(), rect);
}

#[derive(Default)]
struct WinState {
    inside: Option<bool>,
    hidden: bool,
    geo: (i32, i32, f64),
    geo_at: Option<Instant>,
    visible_sent: Option<Instant>,
}

/// Un singur thread pentru toate insulele: (1) fereastra e transparentă la click în afara insulei,
/// (2) `island-hover` către fereastra respectivă, (3) `island-visible` = false cât timp o aplicație
/// rulează pe tot ecranul pe monitorul acelei insule. Fereastra nu e ascunsă cu adevărat:
/// re-afișarea ar fura focusul aplicației care iese din fullscreen.
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut states: HashMap<String, WinState> = HashMap::new();
        let mut fs_at = Instant::now() - Duration::from_secs(1);
        let mut fs_mons: Vec<(i32, i32, i32, i32)> = Vec::new();

        loop {
            std::thread::sleep(Duration::from_millis(30));
            let size_scale = app.state::<SettingsState>().get().size.scale();
            if fs_at.elapsed() > Duration::from_millis(500) {
                fs_at = Instant::now();
                fs_mons = platform::fullscreen_monitors();
            }
            let cursor = platform::cursor_pos();
            let rects = app.state::<IslandRects>().0.lock().unwrap().clone();
            let windows: Vec<(String, WebviewWindow)> =
                app.webview_windows().into_iter().filter(|(l, _)| l.starts_with("island-")).collect();
            states.retain(|l, _| windows.iter().any(|(w, _)| w == l));
            let mut visible = 0;

            for (label, win) in windows {
                let st = states.entry(label.clone()).or_default();
                if st.geo_at.map_or(true, |t| t.elapsed() > Duration::from_secs(1)) {
                    if let (Ok(p), Ok(s)) = (win.outer_position(), win.scale_factor()) {
                        st.geo = (p.x, p.y, s);
                    }
                    st.geo_at = Some(Instant::now());
                }

                let hidden = fs_mons.iter().any(|m| on_monitor((st.geo.0, st.geo.1), *m));
                // retrimis la 2 s: o fereastră abia creată poate să nu fi ascultat încă
                let resend = st.visible_sent.map_or(true, |t| t.elapsed() > Duration::from_secs(2));
                if hidden != st.hidden || resend {
                    st.visible_sent = Some(Instant::now());
                    let _ = app.emit_to(label.as_str(), "island-visible", !hidden);
                }
                if hidden != st.hidden {
                    st.hidden = hidden;
                    if hidden {
                        let _ = win.set_ignore_cursor_events(true);
                        // cursorul putea fi pe insulă: interfața nu trebuie să creadă că e încă acolo
                        if st.inside == Some(true) {
                            let _ = app.emit_to(label.as_str(), "island-hover", false);
                        }
                        st.inside = Some(false);
                    }
                }
                if hidden {
                    continue;
                }
                visible += 1;

                let (Some((cx, cy)), Some(r)) = (cursor, rects.get(&label)) else { continue };
                let k = size_scale;
                let scaled = Rect { x: r.x * k, y: r.y * k, w: r.w * k, h: r.h * k };
                let px = (cx - st.geo.0) as f64 / st.geo.2;
                let py = (cy - st.geo.1) as f64 / st.geo.2;
                let inside = point_in_rect(px, py, &scaled, 1.0);
                if st.inside != Some(inside) {
                    st.inside = Some(inside);
                    let _ = win.set_ignore_cursor_events(!inside);
                    if inside {
                        crate::shelf::ensure_drop_target(&win);
                    }
                    let _ = app.emit_to(label.as_str(), "island-hover", inside);
                }
            }
            VISIBLE.store(visible, Ordering::Relaxed);
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

    #[test]
    fn fereastra_pe_monitor() {
        let mon = (1920, 0, 3840, 1080);
        assert!(on_monitor((2600, 0), mon));
        assert!(!on_monitor((700, 0), mon));
        assert!(!on_monitor((3840, 0), mon));
    }
}
