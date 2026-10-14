#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod alerts;
mod dev_demo;
mod displays;
mod hittest;
mod lyrics;
mod media;
mod notifications;
mod platform;
mod privacy;
mod settings;
mod shelf;
mod timer;
mod tray;
mod updater;
mod volume;
mod windows_list;

use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

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
                .timezone_strategy(TimezoneStrategy::UseLocal)
                .level(if cfg!(debug_assertions) { log::LevelFilter::Debug } else { log::LevelFilter::Info })
                .build(),
        )
        .manage(hittest::IslandRects::default())
        .manage(media::LastMedia::default())
        .manage(updater::Updater::default())
        .manage(timer::ClockState::default())
        .manage(lyrics::LyricsCache::default())
        .invoke_handler(tauri::generate_handler![
            hittest::set_island_rect,
            media::media_control,
            media::media_current,
            media::media_seek,
            media::media_open_source,
            lyrics::get_lyrics,
            shelf::shelf_list,
            shelf::shelf_add_paths,
            shelf::shelf_remove_path,
            shelf::shelf_clear,
            shelf::shelf_open,
            shelf::shelf_reveal,
            shelf::shelf_drag_out,
            windows_list::list_windows,
            windows_list::focus_window,
            settings::get_settings,
            settings::set_settings,
            settings::open_log_dir,
            settings::log_js,
            displays::list_monitors,
            updater::get_update_state,
            updater::check_updates_now,
            updater::apply_update,
            timer::get_timer_state,
            timer::timer_command,
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
            app.manage(shelf::ShelfState::load(app.path().app_config_dir()?.join("shelf.json")));

            updater::install_pending_at_startup(app.handle());
            displays::sync_windows(app.handle());
            displays::spawn_watcher(app.handle().clone());
            hittest::spawn(app.handle().clone());
            media::spawn(app.handle().clone());
            timer::spawn(app.handle().clone());
            privacy::spawn(app.handle().clone());
            alerts::spawn(app.handle().clone());
            dev_demo::spawn(app.handle().clone());
            volume::spawn(app.handle().clone());
            notifications::spawn(app.handle().clone());
            tray::setup(app)?;
            updater::spawn(app.handle().clone());
            if std::env::var_os("DI_OPEN_SETTINGS").is_some() {
                tray::open_settings(app.handle()); // pentru teste: deschide setările la pornire
            }
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("eroare la pornirea aplicației")
        .run(|_app, event| {
            if let tauri::RunEvent::Exit = event {
                volume::restore_osd(); // pop-up-ul de volum al Windows revine
            }
        });
}
