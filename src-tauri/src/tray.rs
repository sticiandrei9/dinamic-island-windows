use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::ManagerExt;

use crate::i18n::{t, tv};
use crate::settings::SettingsState;
use crate::updater::{self, UpdateState};

/// Elementele de meniu care se schimbă în timpul rulării.
pub struct TrayItems {
    update: MenuItem<tauri::Wry>,
    autostart: CheckMenuItem<tauri::Wry>,
    settings: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
}

/// Textul și starea rândului de update din tray.
pub fn update_item_label(state: &UpdateState) -> (String, bool) {
    match state {
        UpdateState::Ready { version } => (tv("tray.apply", version), true),
        UpdateState::Checking => (t("tray.checking"), false),
        UpdateState::Downloading { version } => (tv("tray.downloading", version), false),
        _ => (t("tray.check"), true),
    }
}

pub fn setup(app: &App) -> tauri::Result<()> {
    let version = MenuItem::with_id(app, "version", format!("Dynamic Island {}", app.package_info().version), false, None::<&str>)?;
    let (label, enabled) = update_item_label(&UpdateState::Idle);
    let update = MenuItem::with_id(app, "update", label, enabled, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", t("tray.settings"), true, None::<&str>)?;
    let wanted = app.state::<SettingsState>().get().autostart;
    let autostart = CheckMenuItem::with_id(app, "autostart", t("tray.autostart"), true, wanted, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", t("tray.quit"), true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&version, &update, &sep1, &settings, &autostart, &sep2, &quit])?;
    app.manage(TrayItems { update, autostart, settings, quit });

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
                // Windows a refuzat: bifa și setarea rămân cum erau
                if apply_autostart(app, !s.autostart) {
                    s.autostart = !s.autostart;
                    let _ = s.save(&state.path);
                    *state.current.lock().unwrap() = s.clone();
                }
                refresh_autostart_item(app, s.autostart);
                let _ = app.emit("settings-changed", crate::settings::payload(&s));
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

/// După schimbarea limbii: textele meniului din tray.
pub fn refresh_texts(app: &AppHandle) {
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.settings.set_text(t("tray.settings"));
        let _ = items.autostart.set_text(t("tray.autostart"));
        let _ = items.quit.set_text(t("tray.quit"));
    }
    refresh_update_item(app, &updater::current_state(app));
}

/// Înainte ca installerul să închidă aplicația: altfel iconița rămâne în tray până treci cu mouse-ul peste ea.
pub fn hide_icon(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_visible(false);
    }
}

pub fn show_icon(app: &AppHandle) {
    if let Some(tray) = app.tray_by_id("main") {
        let _ = tray.set_visible(true);
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
            .title(t("settings.title"))
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
        let _ = apply_autostart(app, wanted);
    }
}

/// Aplică setarea „Pornește cu Windows” în registru (doar în build-ul release). Întoarce false dacă Windows a refuzat.
pub fn apply_autostart(app: &AppHandle, on: bool) -> bool {
    if cfg!(debug_assertions) {
        return true;
    }
    let al = app.autolaunch();
    match if on { al.enable() } else { al.disable() } {
        Ok(()) => true,
        Err(e) => {
            log::warn!("autostart: nu pot {} pornirea cu Windows: {e}", if on { "activa" } else { "dezactiva" });
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn textul_butonului_de_update() {
        crate::i18n::set_current(crate::i18n::Lang::Ro);
        assert_eq!(update_item_label(&UpdateState::Ready { version: "0.3.0".into() }), ("Aplică update v0.3.0".into(), true));
        assert_eq!(update_item_label(&UpdateState::Idle), ("Verifică actualizări".into(), true));
        assert_eq!(update_item_label(&UpdateState::UpToDate { checked_at: 1 }).1, true);
        assert_eq!(update_item_label(&UpdateState::Checking).1, false);
        assert_eq!(update_item_label(&UpdateState::Downloading { version: "0.3.0".into() }), ("Se descarcă v0.3.0…".into(), false));
    }
}
