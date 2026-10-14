use std::time::Duration;

use tauri::{AppHandle, Emitter};
use tauri_plugin_updater::UpdaterExt;

pub fn next_delay(first: bool) -> Duration {
    if first { Duration::from_secs(30) } else { Duration::from_secs(6 * 3600) }
}

/// Verifică periodic GitHub Releases; dacă există o versiune nouă, anunță insula,
/// apoi o descarcă, îi verifică semnătura și o instalează. Installerul repornește aplicația.
pub fn spawn(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    std::thread::spawn(move || {
        let mut first = true;
        loop {
            std::thread::sleep(next_delay(first));
            first = false;
            if let Err(e) = tauri::async_runtime::block_on(check_and_install(&app)) {
                eprintln!("actualizare eșuată: {e}");
            }
        }
    });
}

async fn check_and_install(app: &AppHandle) -> tauri_plugin_updater::Result<()> {
    if let Some(update) = app.updater()?.check().await? {
        let _ = app.emit("update-installing", update.version.clone());
        std::thread::sleep(Duration::from_secs(2));
        update.download_and_install(|_, _| {}, || {}).await?;
        app.restart();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prima_verificare_la_30s_apoi_la_6h() {
        assert_eq!(next_delay(true), Duration::from_secs(30));
        assert_eq!(next_delay(false), Duration::from_secs(6 * 3600));
    }
}
