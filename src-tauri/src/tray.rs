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

/// La prima pornire a build-ului release, activează pornirea cu Windows.
pub fn enable_autostart_on_first_run(app: &App) {
    if cfg!(debug_assertions) {
        return;
    }
    let Ok(dir) = app.path().app_config_dir() else { return };
    let marker = dir.join("first-run-done");
    if marker.exists() {
        return;
    }
    let _ = app.autolaunch().enable();
    let _ = std::fs::create_dir_all(&dir);
    let _ = std::fs::write(marker, b"1");
}
