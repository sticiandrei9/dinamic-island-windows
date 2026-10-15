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
// toate insulele și Setările împart o descărcare; scurt, ca un eveniment adăugat cu puțin înainte să apară la timp
const FRESH: Duration = Duration::from_secs(150);

struct Cached {
    url: String,
    at: Instant,
    text: String,
}

static CACHE: Mutex<Option<Cached>> = Mutex::new(None);

/// Calendarul iCal (text), descărcat cel mult o dată la 2,5 minute pentru toate insulele.
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
            let mut bytes = Vec::new();
            resp.into_reader().take(MAX_BYTES + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            if bytes.len() as u64 > MAX_BYTES {
                return Err("calendar > 5 MB".into());
            }
            Ok(String::from_utf8_lossy(&bytes).into_owned())
        }
    })
    .await
    .map_err(|e| e.to_string())??;
    *CACHE.lock().unwrap() = Some(Cached { url, at: Instant::now(), text: text.clone() });
    Ok(Some(text))
}

/// Linkurile pe care insula le poate deschide din calendar: doar https, fără ghilimele
/// (vin din fișierul iCal, adică din textul evenimentelor, și ajung în linia de comandă a lui explorer).
pub fn safe_link(url: &str) -> bool {
    url.starts_with("https://") && !url.chars().any(|c| c == '"' || c.is_whitespace() || c.is_control())
}

/// Deschide evenimentul în Google Calendar sau linkul întâlnirii, în browserul implicit.
#[tauri::command]
pub fn calendar_open(url: String) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    if !safe_link(&url) {
        return Err("link refuzat".into());
    }
    std::process::Command::new("explorer").raw_arg(format!("\"{url}\"")).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn linkuri_deschise_doar_https() {
        assert!(super::safe_link("https://meet.google.com/abc-defg-hij"));
        assert!(super::safe_link("https://www.google.com/calendar/event?eid=abc"));
        assert!(!super::safe_link("file:///C:/Windows/System32/calc.exe"));
        assert!(!super::safe_link("https://x.com/\" & calc"));
        assert!(!super::safe_link("https://x.com/a b"));
    }

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

#[cfg(test)]
mod live {
    /// Manual: descarcă linkul din DI_TEST_ICS_URL ca aplicația (fără să-l afișeze).
    #[test]
    #[ignore]
    fn descarca_linkul_real() {
        let url = super::ics_url(&std::env::var("DI_TEST_ICS_URL").unwrap()).expect("link invalid");
        match ureq::get(&url).call() {
            Ok(r) => println!("status {} , {} bytes", r.status(), r.into_string().unwrap().len()),
            Err(e) => println!("eroare: {}", match e { ureq::Error::Status(c, _) => format!("HTTP {c}"), ureq::Error::Transport(t) => t.kind().to_string() }),
        }
    }
}
