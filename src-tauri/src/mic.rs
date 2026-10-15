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

/// Microfonul implicit e pe mute? None = nu există microfon.
pub fn capture_mute(e: &IMMDeviceEnumerator) -> Option<bool> {
    let (_, v) = endpoint(e, eConsole)?;
    unsafe { v.GetMute().ok().map(|b| b.as_bool()) }
}

/// Mute pe microfonul implicit, și pe cel pentru comunicații (Teams/Zoom îl folosesc pe acela) dacă e altul.
fn set_capture_mute(e: &IMMDeviceEnumerator, muted: bool) -> Result<(), String> {
    let mut done: Vec<String> = Vec::new();
    for role in [eConsole, eCommunications] {
        let Some((id, v)) = endpoint(e, role) else { continue };
        if done.contains(&id) {
            continue;
        }
        unsafe { v.SetMute(muted, std::ptr::null()) }.map_err(|e| e.to_string())?;
        done.push(id);
    }
    if done.is_empty() { Err("nu există microfon".into()) } else { Ok(()) }
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
