use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eCapture, eCommunications, eConsole, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL, COINIT_MULTITHREADED};

/// Enumeratorul Core Audio (thread-ul trebuie să aibă COM inițializat).
pub fn enumerator() -> Option<IMMDeviceEnumerator> {
    unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }.ok()
}

fn endpoint(e: &IMMDeviceEnumerator, role: windows::Win32::Media::Audio::ERole) -> Option<(String, IAudioEndpointVolume)> {
    unsafe {
        let dev = e.GetDefaultAudioEndpoint(eCapture, role).ok()?;
        let id = dev.GetId().ok()?;
        let s = id.to_string().unwrap_or_default();
        windows::Win32::System::Com::CoTaskMemFree(Some(id.0 as *const _));
        Some((s, dev.Activate(CLSCTX_ALL, None).ok()?))
    }
}

/// Microfoanele implicite (pentru aplicații și pentru comunicații — Teams/Zoom), fără dubluri.
fn default_endpoints(e: &IMMDeviceEnumerator) -> Vec<IAudioEndpointVolume> {
    let mut ids: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for role in [eConsole, eCommunications] {
        if let Some((id, v)) = endpoint(e, role) {
            if !ids.contains(&id) {
                ids.push(id);
                out.push(v);
            }
        }
    }
    out
}

/// „Pe mute” doar dacă niciun microfon implicit nu mai transmite. None = nu există microfon.
pub fn all_muted(states: &[bool]) -> Option<bool> {
    if states.is_empty() { None } else { Some(states.iter().all(|m| *m)) }
}

pub fn capture_mute(e: &IMMDeviceEnumerator) -> Option<bool> {
    let states: Vec<bool> = default_endpoints(e).iter().filter_map(|v| unsafe { v.GetMute() }.ok().map(|b| b.as_bool())).collect();
    all_muted(&states)
}

/// Mute pe toate microfoanele implicite; eroare doar dacă n-a reușit pe niciunul.
fn set_capture_mute(e: &IMMDeviceEnumerator, muted: bool) -> Result<(), String> {
    let mut ok = 0;
    let mut last_err = "nu există microfon".to_string();
    for v in default_endpoints(e) {
        match unsafe { v.SetMute(muted, std::ptr::null()) } {
            Ok(()) => ok += 1,
            Err(err) => last_err = err.to_string(),
        }
    }
    if ok > 0 { Ok(()) } else { Err(last_err) }
}

/// Butonul de mute din pastila apelului. Întoarce starea nouă.
#[tauri::command]
pub async fn mic_set_mute(muted: bool) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _com = crate::platform::ComGuard::new(COINIT_MULTITHREADED);
        let e = enumerator().ok_or("Core Audio indisponibil")?;
        set_capture_mute(&e, muted)?;
        Ok(capture_mute(&e).unwrap_or(muted))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mute_doar_daca_toate_microfoanele_implicite_sunt_pe_mute() {
        assert_eq!(all_muted(&[]), None);
        assert_eq!(all_muted(&[true]), Some(true));
        assert_eq!(all_muted(&[true, false]), Some(false)); // unul încă transmite
    }
}
