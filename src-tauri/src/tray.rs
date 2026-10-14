use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt;

use crate::settings::SettingsState;
use crate::updater::{self, UpdateState};

/// Elementele de meniu care se schimbă în timpul rulării.
pub struct TrayItems {
    update: MenuItem<tauri::Wry>,
    autostart: CheckMenuItem<tauri::Wry>,
}

/// Textul și starea rândului de update din tray.
pub fn update_item_label(state: &UpdateState) -> (String, bool) {
    match state {
        UpdateState::Ready { version } => (format!("Aplică update v{version}"), true),
        UpdateState::Checking => ("Se verifică actualizările…".into(), false),
        UpdateState::Downloading { version } => (format!("Se descarcă v{version}…"), false),
        _ => ("Verifică actualizări".into(), true),
    }
}

pub fn setup(app: &App) -> tauri::Result<()> {
    let version = MenuItem::with_id(app, "version", format!("Dynamic Island {}", app.package_info().version), false, None::<&str>)?;
    let (label, enabled) = update_item_label(&UpdateState::Idle);
    let update = MenuItem::with_id(app, "update", label, enabled, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Setări…", true, None::<&str>)?;
    let wanted = app.state::<SettingsState>().get().autostart;
    let autostart = CheckMenuItem::with_id(app, "autostart", "Pornește cu Windows", true, wanted, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Ieșire", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&version, &update, &sep1, &settings, &autostart, &sep2, &quit])?;
    app.manage(TrayItems { update, autostart });

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("iconița aplicației lipsește"))
        .tooltip("Dynamic Island")
        .menu(&menu)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "quit" => app.exit(0),
            "settings" => open_settings(app),
            "update" => {
                let app = app.clone();
                std::thread::spawn(move || {
                    if matches!(updater::current_state(&app), UpdateState::Ready { .. }) {
                        let _ = updater::apply(&app);
                    } else {
                        updater::run_check(&app);
                    }
                });
            }
            "autostart" => {
                let state = app.state::<SettingsState>();
                let mut s = state.get();
                s.autostart = !s.autostart;
                let _ = s.save(&state.path);
                *state.current.lock().unwrap() = s.clone();
                apply_autostart(app, s.autostart);
                if let Some(items) = app.try_state::<TrayItems>() {
                    let _ = items.autostart.set_checked(s.autostart);
                }
                let _ = app.emit("settings-changed", &s);
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

pub fn refresh_update_item(app: &AppHandle, state: &UpdateState) {
    if let Some(items) = app.try_state::<TrayItems>() {
        let (label, enabled) = update_item_label(state);
        let _ = items.update.set_text(label);
        let _ = items.update.set_enabled(enabled);
    }
}

pub fn refresh_autostart_item(app: &AppHandle, on: bool) {
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.autostart.set_checked(on);
    }
}

/// Deschide fereastra de setări sau o aduce în față dacă e deja deschisă.
pub fn open_settings(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        return;
    }
    // crearea ferestrei se face în afara handler-ului de meniu (pe Windows, altfel se poate bloca)
    let app = app.clone();
    std::thread::spawn(move || {
        let built = WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("settings.html".into()))
            .title("Setări — Dynamic Island")
            .inner_size(520.0, 640.0)
            .min_inner_size(440.0, 480.0)
            .center()
            .build();
        if let Err(e) = built {
            log::warn!("nu pot deschide setările: {e}");
        }
    });
}

/// La pornire (release): registrul urmează setarea `autostart` (implicit pornită).
pub fn sync_autostart(app: &AppHandle, wanted: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    if app.autolaunch().is_enabled().unwrap_or(!wanted) != wanted {
        apply_autostart(app, wanted);
    }
}

/// Aplică setarea „Pornește cu Windows” în registru (doar în build-ul release).
pub fn apply_autostart(app: &AppHandle, on: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    let al = app.autolaunch();
    let _ = if on { al.enable() } else { al.disable() };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textul_butonului_de_update() {
        assert_eq!(update_item_label(&UpdateState::Ready { version: "0.3.0".into() }), ("Aplică update v0.3.0".into(), true));
        assert_eq!(update_item_label(&UpdateState::Idle), ("Verifică actualizări".into(), true));
        assert_eq!(update_item_label(&UpdateState::UpToDate { checked_at: 1 }).1, true);
        assert_eq!(update_item_label(&UpdateState::Checking).1, false);
        assert_eq!(update_item_label(&UpdateState::Downloading { version: "0.3.0".into() }), ("Se descarcă v0.3.0…".into(), false));
    }
}
