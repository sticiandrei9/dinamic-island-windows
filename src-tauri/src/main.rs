#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hittest;
mod platform;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .manage(hittest::IslandRect::default())
        .invoke_handler(tauri::generate_handler![hittest::set_island_rect])
        .setup(|app| {
            let win = app.get_webview_window("island").expect("fereastra island lipsește");
            platform::place_top_center(&win)?;
            platform::make_no_activate(&win);
            platform::show_keep_focus(&win)?;
            hittest::spawn(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("eroare la pornirea aplicației");
}
