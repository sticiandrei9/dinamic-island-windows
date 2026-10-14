#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod hittest;
mod media;
mod platform;
mod settings;
mod tray;
mod updater;
mod windows_list;

use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|_app, _args, _cwd| {}))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .clear_targets()
                .target(Target::new(TargetKind::LogDir { file_name: Some("dynamic-island".into()) }))
                .target(Target::new(TargetKind::Stdout))
                .max_file_size(1_000_000)
                .rotation_strategy(RotationStrategy::KeepOne)
                .level(if cfg!(debug_assertions) { log::LevelFilter::Debug } else { log::LevelFilter::Info })
                .build(),
        )
        .manage(hittest::IslandRect::default())
        .manage(media::LastMedia::default())
        .invoke_handler(tauri::generate_handler![
            hittest::set_island_rect,
            media::media_control,
            media::media_current,
            windows_list::list_windows,
            windows_list::focus_window,
            settings::get_settings,
            settings::set_settings,
            settings::open_log_dir,
        ])
        .setup(|app| {
            log::info!("Dynamic Island {} pornit", app.package_info().version);
            let path = app.path().app_config_dir()?.join("settings.json");
            let (loaded, note) = settings::Settings::load(&path);
            match note {
                settings::LoadNote::Corrupt => log::warn!("settings.json corupt; redenumit settings.corrupt.json, pornesc cu valorile implicite"),
                settings::LoadNote::Missing => log::info!("settings.json lipsește; folosesc valorile implicite"),
                settings::LoadNote::Loaded => {}
            }
            tray::sync_autostart(app.handle(), loaded.autostart);
            app.manage(settings::SettingsState { current: std::sync::Mutex::new(loaded), path });

            let win = app.get_webview_window("island").expect("fereastra island lipsește");
            platform::place_top_center(&win)?;
            platform::make_no_activate(&win);
            platform::show_keep_focus(&win)?;
            hittest::spawn(app.handle().clone());
            media::spawn(app.handle().clone());
            tray::setup(app)?;
            updater::spawn(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("eroare la pornirea aplicației");
}
