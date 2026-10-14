use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

use crate::settings::SettingsState;

const STORE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore";

/// O intrare din registru: o aplicație și ultimele momente de pornire/oprire a accesului.
pub struct Entry {
    pub key: String,
    pub start: u64,
    pub stop: u64,
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct Privacy {
    pub mic: Vec<String>,
    pub cam: Vec<String>,
}

/// `C:#Apps#Discord.exe` → `Discord`; `Microsoft.WindowsCamera_8wekyb3d8bbwe` → `WindowsCamera`.
pub fn friendly_app(key: &str) -> String {
    if key.contains('#') {
        let file = key.rsplit('#').next().unwrap_or(key);
        return file.strip_suffix(".exe").or_else(|| file.strip_suffix(".EXE")).unwrap_or(file).to_string();
    }
    let family = key.split('_').next().unwrap_or(key);
    family.rsplit('.').next().unwrap_or(family).to_string()
}

/// Aplicațiile care folosesc dispozitivul acum (pornit și neoprit), fără Dynamic Island.
pub fn in_use(entries: &[Entry], self_exe: &str) -> Vec<String> {
    let me = self_exe.to_lowercase().replace('\\', "#");
    let mut out: Vec<String> = Vec::new();
    for e in entries.iter().filter(|e| e.start > 0 && e.stop == 0) {
        if !me.is_empty() && e.key.to_lowercase() == me {
            continue;
        }
        let name = friendly_app(&e.key);
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

/// Oglinda insulei folosește camera prin procesul WebView2; cât e deschisă, nu o raportăm.
pub fn hide_own_mirror(apps: Vec<String>, mirror_active: bool) -> Vec<String> {
    if !mirror_active {
        return apps;
    }
    apps.into_iter().filter(|a| !a.eq_ignore_ascii_case("msedgewebview2")).collect()
}

/// Oglinda e deschisă acum (anunțat de interfață).
#[derive(Default)]
pub struct MirrorActive(pub std::sync::atomic::AtomicBool);

#[tauri::command]
pub fn set_mirror_active(state: tauri::State<MirrorActive>, active: bool) {
    state.0.store(active, std::sync::atomic::Ordering::SeqCst);
}

fn read_entries(capability: &str) -> Vec<Entry> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let Ok(root) = hkcu.open_subkey(format!(r"{STORE}\{capability}")) else { return Vec::new() };
    let read = |k: &RegKey, name: String| Entry {
        start: k.get_value::<u64, _>("LastUsedTimeStart").unwrap_or(0),
        stop: k.get_value::<u64, _>("LastUsedTimeStop").unwrap_or(0),
        key: name,
    };
    let mut out = Vec::new();
    for name in root.enum_keys().flatten() {
        if name == "NonPackaged" {
            if let Ok(np) = root.open_subkey(&name) {
                for sub in np.enum_keys().flatten() {
                    if let Ok(k) = np.open_subkey(&sub) {
                        out.push(read(&k, sub));
                    }
                }
            }
        } else if let Ok(k) = root.open_subkey(&name) {
            out.push(read(&k, name));
        }
    }
    out
}

/// Citește registrul o dată pe secundă și emite `privacy` doar la schimbări.
pub fn spawn(app: AppHandle) {
    let me = std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    std::thread::spawn(move || {
        let mut last = Privacy::default();
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let cur = if app.state::<SettingsState>().get().modules.privacy {
                let mirror = app.state::<MirrorActive>().0.load(std::sync::atomic::Ordering::SeqCst);
                Privacy {
                    mic: in_use(&read_entries("microphone"), &me),
                    cam: hide_own_mirror(in_use(&read_entries("webcam"), &me), mirror),
                }
            } else {
                Privacy::default()
            };
            if cur != last {
                let _ = app.emit("privacy", &cur);
                last = cur;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(key: &str, start: u64, stop: u64) -> Entry {
        Entry { key: key.into(), start, stop }
    }

    #[test]
    fn nume_prietenoase() {
        assert_eq!(friendly_app("C:#Users#A#AppData#Local#Discord#app-1.0#Discord.exe"), "Discord");
        assert_eq!(friendly_app("Microsoft.WindowsCamera_8wekyb3d8bbwe"), "WindowsCamera");
        assert_eq!(friendly_app("Zoom"), "Zoom");
    }

    #[test]
    fn doar_aplicatiile_care_folosesc_acum() {
        let list = vec![
            e("C:#Apps#Discord.exe", 100, 0),      // în uz
            e("C:#Apps#Teams.exe", 100, 200),      // oprit
            e("Microsoft.WindowsSoundRecorder_8wekyb3d8bbwe", 0, 0), // niciodată folosit
        ];
        assert_eq!(in_use(&list, "c:\\apps\\dynamic-island.exe"), vec!["Discord".to_string()]);
    }

    #[test]
    fn insula_insasi_e_ignorata() {
        let list = vec![e("C:#Program Files#Dynamic Island#dynamic-island.exe", 5, 0)];
        assert!(in_use(&list, "C:\\Program Files\\Dynamic Island\\dynamic-island.exe").is_empty());
    }

    #[test]
    fn oglinda_noastra_nu_aprinde_punctul_de_camera() {
        let cams = vec!["msedgewebview2".to_string(), "Zoom".to_string()];
        assert_eq!(hide_own_mirror(cams.clone(), true), vec!["Zoom".to_string()]);
        assert_eq!(hide_own_mirror(cams.clone(), false), cams);
    }

    #[test]
    fn fara_duplicate() {
        let list = vec![e("C:#A#chrome.exe", 1, 0), e("C:#B#chrome.exe", 1, 0)];
        assert_eq!(in_use(&list, ""), vec!["chrome".to_string()]);
    }

    /// Rulează doar manual (`cargo test -- --ignored`): citește registrul real al calculatorului.
    #[test]
    #[ignore]
    fn citeste_registrul_real() {
        let cam = read_entries("webcam");
        let mic = read_entries("microphone");
        println!("webcam: {} intrări, microphone: {} intrări", cam.len(), mic.len());
        for e in cam.iter().chain(mic.iter()).take(12) {
            println!("  {} start={} stop={}", friendly_app(&e.key), e.start, e.stop);
        }
        assert!(!cam.is_empty() || !mic.is_empty());
    }
}
