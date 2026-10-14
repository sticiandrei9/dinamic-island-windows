#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hittest;
mod media;
mod platform;
mod tray;
mod windows_list;

use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .manage(hittest::IslandRect::default())
        .manage(media::LastMedia::default())
        .invoke_handler(tauri::generate_handler![
            hittest::set_island_rect,
            media::media_control,
            media::media_current,
            windows_list::list_windows,
            windows_list::focus_window,
        ])
        .setup(|app| {
            let win = app.get_webview_window("island").expect("fereastra island lipsește");
            platform::place_top_center(&win)?;
            platform::make_no_activate(&win);
            platform::show_keep_focus(&win)?;
            hittest::spawn(app.handle().clone());
            media::spawn(app.handle().clone());
            tray::enable_autostart_on_first_run(app);
            tray::setup(app)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("eroare la pornirea aplicației");
}
