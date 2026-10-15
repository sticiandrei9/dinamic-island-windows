use std::io::Read;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

use crate::settings::SettingsState;

/// Linkul iCal din setări → adresa descărcată. Doar https (webcal:// e același lucru);
/// orice altceva e refuzat, ca setarea să nu poată citi fișiere locale sau rețeaua internă prin http.
pub fn ics_url(setting: &str) -> Option<String> {
    let s = setting.trim();
    let url = match s.strip_prefix("webcal://") {
        Some(rest) => format!("https://{rest}"),
        None => s.to_string(),
    };
    url.starts_with("https://").then_some(url)
}

const MAX_BYTES: u64 = 5 * 1024 * 1024;
const FRESH: Duration = Duration::from_secs(10 * 60);

struct Cached {
    url: String,
    at: Instant,
    text: String,
}

static CACHE: Mutex<Option<Cached>> = Mutex::new(None);

/// Calendarul iCal (text), descărcat cel mult o dată la 10 minute pentru toate insulele.
/// None = nu e setat niciun link.
#[tauri::command]
pub async fn calendar_fetch(app: AppHandle) -> Result<Option<String>, String> {
    // pentru teste: un fișier .ics local în locul linkului (doar în build-ul de depanare)
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("DI_ICS_FILE") {
        return std::fs::read_to_string(path).map(Some).map_err(|e| e.to_string());
    }
    let Some(url) = ics_url(&app.state::<SettingsState>().get().calendar_url) else { return Ok(None) };
    if let Some(c) = CACHE.lock().unwrap().as_ref().filter(|c| c.url == url && c.at.elapsed() < FRESH) {
        return Ok(Some(c.text.clone()));
    }
    let text = tauri::async_runtime::spawn_blocking({
        let url = url.clone();
        move || -> Result<String, String> {
            let resp = ureq::get(&url).timeout(Duration::from_secs(20)).call().map_err(|e| {
                // linkul e secret: în log ajunge doar codul de eroare, nu adresa
                let msg = match e {
                    ureq::Error::Status(code, _) => format!("HTTP {code}"),
                    ureq::Error::Transport(t) => t.kind().to_string(),
                };
                log::warn!("calendar: descărcarea a eșuat: {msg}");
                msg
            })?;
            let mut text = String::new();
            resp.into_reader().take(MAX_BYTES).read_to_string(&mut text).map_err(|e| e.to_string())?;
            Ok(text)
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    *CACHE.lock().unwrap() = Some(Cached { url, at: Instant::now(), text: text.clone() });
    Ok(Some(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doar_linkuri_https() {
        assert_eq!(ics_url(" https://calendar.google.com/x/basic.ics "), Some("https://calendar.google.com/x/basic.ics".into()));
        assert_eq!(ics_url("webcal://outlook.live.com/a.ics"), Some("https://outlook.live.com/a.ics".into()));
        assert_eq!(ics_url(""), None);
        assert_eq!(ics_url("http://192.168.0.1/a.ics"), None);
        assert_eq!(ics_url("file:///C:/a.ics"), None);
    }
}
