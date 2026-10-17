use crate::media::await_op;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Com::COINIT_MULTITHREADED;
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};
use winreg::enums::{HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
use winreg::RegKey;

const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

/// `None` = indisponibil pe acest PC → butonul lipsește. `night` și `dnd` vin în Task 4.
#[derive(Serialize, Clone, Debug)]
pub struct Controls {
    pub wifi: Option<bool>,
    pub bluetooth: Option<bool>,
    pub dark: Option<bool>,
    pub night: Option<bool>,
    pub dnd: Option<bool>,
    pub mic_muted: Option<bool>,
}

/// (`AppsUseLightTheme`, `SystemUsesLightTheme`) pentru modul cerut: 0 = întunecat, 1 = deschis.
pub fn dark_values(on: bool) -> (u32, u32) {
    if on { (0, 0) } else { (1, 1) }
}

// ---------- radiouri (Wi-Fi / Bluetooth) ----------

/// Toate radiourile, dar numai după ce accesul e `Allowed`; altfel gol (butoanele lipsesc, nu eșuează tăcut).
/// Se reține doar un răspuns `Allowed`, ca un refuz de acum să poată fi reîncercat mai târziu.
fn radios() -> Vec<Radio> {
    static ALLOWED: AtomicBool = AtomicBool::new(false);
    if !ALLOWED.load(Ordering::Relaxed) {
        if !matches!(Radio::RequestAccessAsync().and_then(await_op), Ok(RadioAccessStatus::Allowed)) {
            return Vec::new();
        }
        ALLOWED.store(true, Ordering::Relaxed);
    }
    let Ok(all) = Radio::GetRadiosAsync().and_then(await_op) else { return Vec::new() };
    all.into_iter().collect()
}

fn of_kind(all: &[Radio], kind: RadioKind) -> Vec<&Radio> {
    all.iter().filter(|r| r.Kind().ok() == Some(kind)).collect()
}

fn radio_on(all: &[Radio], kind: RadioKind) -> Option<bool> {
    let rs = of_kind(all, kind);
    if rs.is_empty() { None } else { Some(rs.iter().any(|r| r.State().ok() == Some(RadioState::On))) }
}

fn set_radio(kind: RadioKind, on: bool) -> Result<(), String> {
    let all = radios();
    let rs = of_kind(&all, kind);
    if rs.is_empty() {
        return Err("nu există radio sau accesul e refuzat".into());
    }
    let state = if on { RadioState::On } else { RadioState::Off };
    for r in rs {
        let status = r.SetStateAsync(state).and_then(await_op).map_err(|e| e.to_string())?;
        if status != RadioAccessStatus::Allowed {
            return Err(format!("acces refuzat la radio ({})", status.0));
        }
    }
    Ok(())
}

// ---------- mod întunecat ----------

fn dark_on() -> Option<bool> {
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(PERSONALIZE, KEY_READ).ok()?;
    Some(key.get_value::<u32, _>("AppsUseLightTheme").ok()? == 0)
}

fn set_dark(on: bool) -> Result<(), String> {
    let (apps, system) = dark_values(on);
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(PERSONALIZE, KEY_SET_VALUE).map_err(|e| e.to_string())?;
    key.set_value("AppsUseLightTheme", &apps).map_err(|e| e.to_string())?;
    key.set_value("SystemUsesLightTheme", &system).map_err(|e| e.to_string())?;
    // fără anunț, Explorer și aplicațiile deschise își păstrează vechea temă până la repornire
    let area: Vec<u16> = "ImmersiveColorSet\0".encode_utf16().collect();
    unsafe {
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(area.as_ptr() as isize), SMTO_ABORTIFHUNG, 200, None);
    }
    Ok(())
}

// ---------- comenzi ----------

fn read_state() -> Controls {
    let _com = crate::platform::ComGuard::new(COINIT_MULTITHREADED);
    let all = radios(); // o singură enumerare pentru Wi-Fi și Bluetooth
    Controls {
        wifi: radio_on(&all, RadioKind::WiFi),
        bluetooth: radio_on(&all, RadioKind::Bluetooth),
        dark: dark_on(),
        night: None,
        dnd: None,
        mic_muted: crate::mic::enumerator().and_then(|e| crate::mic::capture_mute(&e)),
    }
}

#[tauri::command]
pub async fn controls_state() -> Result<Controls, String> {
    // la eroare de join JS păstrează starea veche (nu un Controls gol, care ar ascunde butoanele)
    tauri::async_runtime::spawn_blocking(read_state).await.map_err(|e| e.to_string())
}

/// `on` = starea cerută a câmpului din `Controls` (pentru microfon: `true` = pe mute). Întoarce starea nouă.
#[tauri::command]
pub async fn controls_set(name: String, on: bool) -> Result<Controls, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _com = crate::platform::ComGuard::new(COINIT_MULTITHREADED);
        match name.as_str() {
            "wifi" => set_radio(RadioKind::WiFi, on)?,
            "bluetooth" => set_radio(RadioKind::Bluetooth, on)?,
            "dark" => set_dark(on)?,
            "mic" => crate::mic::set_capture_mute(&crate::mic::enumerator().ok_or("Core Audio indisponibil")?, on)?,
            _ => return Err(format!("control necunoscut: {name}")),
        }
        Ok(read_state())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn controls_action(name: String) -> Result<(), String> {
    match name.as_str() {
        "lock" => unsafe { LockWorkStation() }.map_err(|e| e.to_string()),
        // SetSuspendState se întoarce abia la trezire: pe firul lui, ca să nu blocăm comanda
        "sleep" => {
            std::thread::spawn(|| {
                if !unsafe { SetSuspendState(false, false, false) }.as_bool() {
                    log::warn!("SetSuspendState a eșuat (sleep neexecutat)");
                }
            });
            Ok(())
        }
        _ => Err(format!("acțiune necunoscută: {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valori_mod_intunecat() {
        assert_eq!(dark_values(true), (0, 0));
        assert_eq!(dark_values(false), (1, 1));
    }

    /// Doar citire (R2: niciodată nu comută nimic). `cargo test controls -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn citeste_starea_de_pe_pc() {
        println!("{:?}", read_state());
    }
}
