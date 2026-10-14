use std::sync::mpsc::{channel, Sender};
use std::time::{Duration, Instant};

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use windows::core::{implement, w, PCWSTR};
use windows::Win32::Devices::Display::DISPLAY_BRIGHTNESS;
use windows::Win32::Foundation::{CloseHandle, HANDLE, HWND};
use windows::Win32::Media::Audio::Endpoints::{
    IAudioEndpointVolume, IAudioEndpointVolumeCallback, IAudioEndpointVolumeCallback_Impl,
};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator, AUDIO_VOLUME_NOTIFICATION_DATA,
};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::Win32::System::IO::DeviceIoControl;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, FindWindowW, ShowWindow, SW_MINIMIZE, SW_RESTORE};

use crate::settings::SettingsState;

/// CTL_CODE(FILE_DEVICE_VIDEO, 0x126, METHOD_BUFFERED, FILE_ANY_ACCESS)
pub const IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS: u32 = (0x23 << 16) | (0x126 << 2);

/// Ignorăm variațiile sub 1% (unele drivere trimit callback-uri repetate cu aceeași valoare).
pub fn volume_changed(prev: Option<(f32, bool)>, cur: (f32, bool)) -> bool {
    match prev {
        None => true,
        Some((v, m)) => m != cur.1 || (v - cur.0).abs() >= 0.01,
    }
}

pub fn pick_brightness(ac: u8, dc: u8, on_ac: bool) -> f64 {
    (if on_ac { ac } else { dc }).min(100) as f64 / 100.0
}

#[implement(IAudioEndpointVolumeCallback)]
struct VolumeCallback {
    tx: Sender<(f32, bool)>,
}

impl IAudioEndpointVolumeCallback_Impl for VolumeCallback_Impl {
    fn OnNotify(&self, data: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        if let Some(d) = unsafe { data.as_ref() } {
            let _ = self.tx.send((d.fMasterVolume, d.bMuted.as_bool()));
        }
        Ok(())
    }
}

/// Difuzorul implicit + callback-ul înregistrat pe el.
struct Endpoint {
    id: String,
    volume: IAudioEndpointVolume,
    callback: IAudioEndpointVolumeCallback,
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        unsafe {
            let _ = self.volume.UnregisterControlChangeNotify(&self.callback);
        }
    }
}

fn default_endpoint_id(e: &IMMDeviceEnumerator) -> Option<String> {
    unsafe {
        let dev = e.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
        let id = dev.GetId().ok()?;
        let s = id.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(id.0 as *const _));
        s
    }
}

fn register(e: &IMMDeviceEnumerator, tx: &Sender<(f32, bool)>) -> windows::core::Result<Endpoint> {
    unsafe {
        let dev = e.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let volume: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None)?;
        let callback: IAudioEndpointVolumeCallback = VolumeCallback { tx: tx.clone() }.into();
        volume.RegisterControlChangeNotify(&callback)?;
        Ok(Endpoint { id: default_endpoint_id(e).unwrap_or_default(), volume, callback })
    }
}

/// Fereastra OSD de volum a Windows (metoda HideVolumeOSD: `NativeHWNDHost` cu un copil `DirectUIHWND`).
fn find_osd() -> Option<HWND> {
    unsafe {
        let host = FindWindowW(w!("NativeHWNDHost"), PCWSTR::null()).ok()?;
        FindWindowExW(host, HWND::default(), w!("DirectUIHWND"), PCWSTR::null()).ok()?;
        Some(host)
    }
}

/// Luminozitatea ecranului intern (laptop), prin `\\.\LCD`. None pe desktop sau dacă driverul nu răspunde.
fn read_brightness(lcd: HANDLE) -> Option<(u8, u8)> {
    let mut b = DISPLAY_BRIGHTNESS::default();
    let mut ret = 0u32;
    unsafe {
        DeviceIoControl(
            lcd,
            IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS,
            None,
            0,
            Some(&mut b as *mut _ as *mut _),
            std::mem::size_of::<DISPLAY_BRIGHTNESS>() as u32,
            Some(&mut ret),
            None,
        )
        .ok()?;
    }
    Some((b.ucACBrightness, b.ucDCBrightness))
}

fn on_ac() -> bool {
    let mut s = windows::Win32::System::Power::SYSTEM_POWER_STATUS::default();
    unsafe { windows::Win32::System::Power::GetSystemPowerStatus(&mut s).is_ok() && s.ACLineStatus != 0 }
}

/// Thread dedicat (COM MTA): volumul prin callback, luminozitatea la 500 ms, OSD-ul Windows ascuns.
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        let enumerator: IMMDeviceEnumerator = match unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) } {
            Ok(e) => e,
            Err(e) => {
                log::warn!("volum: Core Audio indisponibil, modulul e oprit: {e}");
                return;
            }
        };
        let (tx, rx) = channel::<(f32, bool)>();
        let mut endpoint: Option<Endpoint> = None;
        let mut last_volume: Option<(f32, bool)> = None;
        let mut osd: Option<HWND> = None;
        let mut osd_logged = false;
        let lcd = unsafe {
            CreateFileW(w!("\\\\.\\LCD"), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None).ok()
        };
        let mut last_brightness = lcd.and_then(read_brightness).map(|(a, d)| pick_brightness(a, d, on_ac()));
        let mut checked_device = Instant::now() - Duration::from_secs(10);

        loop {
            let enabled = app.state::<SettingsState>().get().modules.volume;
            if !enabled {
                // modul oprit: renunțăm la callback și dăm înapoi OSD-ul Windows
                endpoint = None;
                if let Some(h) = osd.take() {
                    unsafe { let _ = ShowWindow(h, SW_RESTORE); }
                }
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }

            // difuzorul implicit se poate schimba (căști conectate etc.)
            if checked_device.elapsed() > Duration::from_secs(2) {
                checked_device = Instant::now();
                let id = default_endpoint_id(&enumerator);
                if endpoint.as_ref().map(|e| Some(&e.id)) != Some(id.as_ref()) {
                    endpoint = None;
                    match register(&enumerator, &tx) {
                        Ok(e) => endpoint = Some(e),
                        Err(e) => log::warn!("volum: nu pot urmări difuzorul implicit: {e}"),
                    }
                }
            }

            // schimbările de volum (ultima din rafală)
            let mut latest = None;
            while let Ok(v) = rx.recv_timeout(if latest.is_some() { Duration::from_millis(1) } else { Duration::from_millis(500) }) {
                latest = Some(v);
            }
            if let Some(v) = latest {
                if volume_changed(last_volume, v) {
                    last_volume = Some(v);
                    let _ = app.emit("volume", json!({ "source": "volume", "level": v.0, "muted": v.1 }));
                    if osd.is_none() {
                        osd = find_osd();
                        if osd.is_none() && !osd_logged {
                            osd_logged = true;
                            log::info!("OSD-ul Windows de volum nu poate fi ascuns pe această versiune; apar ambele");
                        }
                    }
                    if let Some(h) = osd {
                        unsafe { let _ = ShowWindow(h, SW_MINIMIZE); }
                    }
                }
            }

            if let Some(h) = lcd {
                if let Some((a, d)) = read_brightness(h) {
                    let b = pick_brightness(a, d, on_ac());
                    if last_brightness.is_some_and(|prev| (prev - b).abs() >= 0.01) {
                        let _ = app.emit("volume", json!({ "source": "brightness", "level": b, "muted": false }));
                    }
                    last_brightness = Some(b);
                }
            }
        }
        #[allow(unreachable_code)]
        if let Some(h) = lcd {
            unsafe { let _ = CloseHandle(h); }
        }
    });
}

/// La ieșire, OSD-ul Windows e restaurat (dacă l-am ascuns).
pub fn restore_osd() {
    if let Some(h) = find_osd() {
        unsafe { let _ = ShowWindow(h, SW_RESTORE); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schimbari_semnificative_de_volum() {
        assert!(volume_changed(None, (0.5, false))); // prima valoare după un callback
        assert!(!volume_changed(Some((0.5, false)), (0.502, false)));
        assert!(volume_changed(Some((0.5, false)), (0.52, false)));
        assert!(volume_changed(Some((0.5, false)), (0.5, true)));
    }

    #[test]
    fn luminozitatea_dupa_sursa_de_alimentare() {
        assert_eq!(pick_brightness(70, 40, true), 0.7);
        assert_eq!(pick_brightness(70, 40, false), 0.4);
        assert_eq!(pick_brightness(250, 40, true), 1.0); // valori în afara intervalului sunt limitate
    }

    #[test]
    fn codul_ioctl_pentru_luminozitate() {
        assert_eq!(IOCTL_VIDEO_QUERY_DISPLAY_BRIGHTNESS, 0x0023_0498);
    }

    /// Rulează doar manual: luminozitatea reală a ecranului intern.
    #[test]
    #[ignore]
    fn citeste_luminozitatea_reala() {
        let lcd = unsafe {
            CreateFileW(w!(r"\\.\LCD"), 0, FILE_SHARE_READ | FILE_SHARE_WRITE, None, OPEN_EXISTING, FILE_FLAGS_AND_ATTRIBUTES(0), None)
        };
        println!("LCD: {:?}", lcd.as_ref().map(|_| "deschis"));
        println!("luminozitate: {:?}", lcd.ok().and_then(read_brightness));
    }
}
