// IPolicyConfig păstrează numele metodelor COM
#![allow(non_snake_case)]

use std::ffi::c_void;

use serde::Serialize;
use windows::core::{interface, IUnknown, IUnknown_Vtbl, GUID, HRESULT, HSTRING, PCWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{eConsole, eMultimedia, eRender, ERole, DEVICE_STATE_ACTIVE};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL, COINIT_APARTMENTTHREADED, STGM_READ};

/// O ieșire audio (difuzoare, căști, monitor HDMI…).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Output {
    pub id: String,
    pub name: String,
    pub default: bool,
}

// Windows nu are o funcție publică pentru „dispozitivul implicit”; IPolicyConfig e interfața
// (nedocumentată, stabilă din Windows 7) folosită de panoul de sunet și de aplicațiile de profil.
// Doar SetDefaultEndpoint e apelată; celelalte metode țin locul în tabelul virtual.
#[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
unsafe trait IPolicyConfig: IUnknown {
    fn GetMixFormat(&self, id: PCWSTR, format: *mut *mut c_void) -> HRESULT;
    fn GetDeviceFormat(&self, id: PCWSTR, default: i32, format: *mut *mut c_void) -> HRESULT;
    fn ResetDeviceFormat(&self, id: PCWSTR) -> HRESULT;
    fn SetDeviceFormat(&self, id: PCWSTR, endpoint: *mut c_void, mix: *mut c_void) -> HRESULT;
    fn GetProcessingPeriod(&self, id: PCWSTR, default: i32, def: *mut i64, min: *mut i64) -> HRESULT;
    fn SetProcessingPeriod(&self, id: PCWSTR, period: *mut i64) -> HRESULT;
    fn GetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn SetShareMode(&self, id: PCWSTR, mode: *mut c_void) -> HRESULT;
    fn GetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *mut c_void) -> HRESULT;
    fn SetPropertyValue(&self, id: PCWSTR, key: *const c_void, value: *const c_void) -> HRESULT;
    fn SetDefaultEndpoint(&self, id: PCWSTR, role: ERole) -> HRESULT;
    fn SetEndpointVisibility(&self, id: PCWSTR, visible: i32) -> HRESULT;
}

const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

fn take_id(id: windows::core::PWSTR) -> String {
    let s = unsafe { id.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(id.0 as *const _)) };
    s
}

fn list() -> Vec<Output> {
    let _com = crate::platform::ComGuard::new(COINIT_APARTMENTTHREADED);
    let Some(e) = crate::mic::enumerator() else { return Vec::new() };
    unsafe {
        let default = e.GetDefaultAudioEndpoint(eRender, eConsole).and_then(|d| d.GetId()).map(take_id).unwrap_or_default();
        let Ok(devices) = e.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else { return Vec::new() };
        let n = devices.GetCount().unwrap_or(0);
        (0..n)
            .filter_map(|i| {
                let d = devices.Item(i).ok()?;
                let id = take_id(d.GetId().ok()?);
                let name = d.OpenPropertyStore(STGM_READ).ok()?.GetValue(&PKEY_Device_FriendlyName).ok()?.to_string();
                Some(Output { default: id == default, id, name })
            })
            .collect()
    }
}

#[tauri::command]
pub async fn audio_outputs() -> Vec<Output> {
    tauri::async_runtime::spawn_blocking(list).await.unwrap_or_default()
}

/// Face ieșirea implicită pentru aplicații și multimedia, ca „Implicit” din panoul de sunet; dispozitivul
/// de comunicare (ales separat pentru Teams/Zoom) rămâne neatins.
#[tauri::command]
pub async fn audio_set_output(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _com = crate::platform::ComGuard::new(COINIT_APARTMENTTHREADED);
        let policy: IPolicyConfig = unsafe { CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL) }.map_err(|e| e.to_string())?;
        let id = HSTRING::from(id);
        for role in [eConsole, eMultimedia] {
            unsafe { policy.SetDefaultEndpoint(PCWSTR(id.as_ptr()), role) }.ok().map_err(|e| e.to_string())?;
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rulează doar manual (`cargo test -- --ignored`): ieșirile audio reale.
    #[test]
    #[ignore]
    fn listeaza_iesirile_reale() {
        for o in list() {
            println!("{} {} [{}]", if o.default { "*" } else { " " }, o.name, o.id);
        }
        // implicitul pus din nou pe același dispozitiv: verifică apelul IPolicyConfig fără să schimbe nimic
        let def = list().into_iter().find(|o| o.default).expect("niciun dispozitiv implicit");
        tauri::async_runtime::block_on(audio_set_output(def.id.clone())).unwrap();
        assert!(list().iter().any(|o| o.default && o.id == def.id));
    }
}
