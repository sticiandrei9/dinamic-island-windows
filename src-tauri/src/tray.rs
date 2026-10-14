use tauri::menu::{CheckMenuItem, Menu, MenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{App, Manager};
use tauri_plugin_autostart::ManagerExt;

pub fn setup(app: &App) -> tauri::Result<()> {
    let enabled = app.autolaunch().is_enabled().unwrap_or(false);
    let autostart = CheckMenuItem::with_id(app, "autostart", "Pornește cu Windows", true, enabled, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Ieșire", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&autostart, &quit])?;
    let item = autostart.clone();

    TrayIconBuilder::with_id("main")
        .icon(app.default_window_icon().cloned().expect("iconița aplicației lipsește"))
        .tooltip("Dynamic Island")
        .menu(&menu)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "quit" => app.exit(0),
            "autostart" => {
                let al = app.autolaunch();
                let was = al.is_enabled().unwrap_or(false);
                let _ = if was { al.disable() } else { al.enable() };
                let _ = item.set_checked(al.is_enabled().unwrap_or(!was));
            }
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// La pornire (release): registrul urmează setarea `autostart` (implicit pornită).
pub fn sync_autostart(app: &tauri::AppHandle, wanted: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    if app.autolaunch().is_enabled().unwrap_or(!wanted) != wanted {
        apply_autostart(app, wanted);
    }
}

/// Aplică setarea „Pornește cu Windows” în registru (doar în build-ul release).
pub fn apply_autostart(app: &tauri::AppHandle, on: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    let al = app.autolaunch();
    let _ = if on { al.enable() } else { al.disable() };
}
