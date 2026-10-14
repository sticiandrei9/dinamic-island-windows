#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod platform;

use tauri::Manager;

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let win = app.get_webview_window("island").expect("fereastra island lipsește");
            platform::place_top_center(&win)?;
            platform::make_no_activate(&win);
            platform::show_keep_focus(&win)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("eroare la pornirea aplicației");
}
