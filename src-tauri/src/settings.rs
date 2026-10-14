use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Deserializer, Serialize};
use tauri::{AppHandle, Emitter};

/// Un câmp cu o valoare necunoscută (de ex. dintr-o versiune viitoare) primește valoarea implicită,
/// în loc să invalideze tot fișierul.
fn lenient<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    let v = serde_json::Value::deserialize(d)?;
    Ok(T::deserialize(v).unwrap_or_default())
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Modules {
    pub media: bool,
    pub lyrics: bool,
    pub apps: bool,
    pub timer: bool,
    pub privacy: bool,
    pub alerts: bool,
    pub volume: bool,
    pub notifications: bool,
    pub shelf: bool,
    pub mirror: bool,
}

impl Default for Modules {
    fn default() -> Self {
        Self {
            media: true, lyrics: true, apps: true, timer: true, privacy: true,
            alerts: true, volume: true, notifications: true, shelf: true, mirror: true,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExpandOn {
    #[default]
    Hover,
    Click,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Size {
    Small,
    #[default]
    Normal,
    Large,
}

impl Size {
    pub fn scale(self) -> f64 {
        match self {
            Size::Small => 0.85,
            Size::Normal => 1.0,
            Size::Large => 1.15,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Settings {
    #[serde(deserialize_with = "lenient")]
    pub modules: Modules,
    #[serde(deserialize_with = "lenient")]
    pub expand_on: ExpandOn,
    /// "primary", "all" sau numele dispozitivului (`\\.\DISPLAY2`)
    #[serde(deserialize_with = "lenient")]
    pub monitor: String,
    #[serde(deserialize_with = "lenient")]
    pub size: Size,
    #[serde(deserialize_with = "lenient")]
    pub autostart: bool,
    #[serde(deserialize_with = "lenient")]
    pub auto_update: bool,
    #[serde(deserialize_with = "lenient")]
    pub mirror_camera: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            modules: Modules::default(),
            expand_on: ExpandOn::Hover,
            monitor: "primary".into(),
            size: Size::Normal,
            autostart: true,
            auto_update: true,
            mirror_camera: None,
        }
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum LoadNote {
    Loaded,
    Missing,
    Corrupt,
}

impl Settings {
    pub fn load(path: &Path) -> (Settings, LoadNote) {
        let Ok(text) = std::fs::read_to_string(path) else {
            return (Settings::default(), LoadNote::Missing);
        };
        match serde_json::from_str(&text) {
            Ok(s) => (s, LoadNote::Loaded),
            Err(_) => {
                let _ = std::fs::rename(path, path.with_file_name("settings.corrupt.json"));
                (Settings::default(), LoadNote::Corrupt)
            }
        }
    }

    /// Scriere atomică: fișier temporar, apoi redenumire peste cel vechi.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self).map_err(std::io::Error::other)?)?;
        std::fs::rename(&tmp, path)
    }
}

pub struct SettingsState {
    pub current: Mutex<Settings>,
    pub path: PathBuf,
}

impl SettingsState {
    pub fn get(&self) -> Settings {
        self.current.lock().unwrap().clone()
    }
}

#[tauri::command]
pub fn get_settings(state: tauri::State<SettingsState>) -> Settings {
    state.get()
}

#[tauri::command]
pub fn set_settings(app: AppHandle, state: tauri::State<SettingsState>, settings: Settings) -> Result<(), String> {
    settings.save(&state.path).map_err(|e| e.to_string())?;
    let previous = std::mem::replace(&mut *state.current.lock().unwrap(), settings.clone());
    if previous.autostart != settings.autostart {
        crate::tray::apply_autostart(&app, settings.autostart);
        crate::tray::refresh_autostart_item(&app, settings.autostart);
    }
    let _ = app.emit("settings-changed", &settings);
    // ferestrele se creează în afara thread-ului comenzii (crearea dintr-o comandă sincronă blochează)
    std::thread::spawn(move || crate::displays::sync_windows(&app));
    Ok(())
}

#[tauri::command]
pub fn open_log_dir(app: AppHandle) -> Result<(), String> {
    use tauri::Manager;
    let dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    let _ = std::fs::create_dir_all(&dir);
    std::process::Command::new("explorer").arg(dir).spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("di-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn implicit_toate_modulele_pornite() {
        let s = Settings::default();
        let m = &s.modules;
        assert!(m.media && m.lyrics && m.apps && m.timer && m.privacy && m.alerts && m.volume && m.notifications && m.shelf && m.mirror);
        assert_eq!(s.expand_on, ExpandOn::Hover);
        assert_eq!(s.monitor, "primary");
        assert_eq!(s.size, Size::Normal);
        assert!(s.autostart && s.auto_update);
    }

    #[test]
    fn json_partial_completat_cu_implicite() {
        let s: Settings = serde_json::from_str(r#"{"size":"large","modules":{"lyrics":false}}"#).unwrap();
        assert_eq!(s.size, Size::Large);
        assert!(!s.modules.lyrics);
        assert!(s.modules.media);
        assert_eq!(s.expand_on, ExpandOn::Hover);
    }

    #[test]
    fn campuri_si_valori_necunoscute_ignorate() {
        let s: Settings = serde_json::from_str(r#"{"viitor":1,"size":"urias","expand_on":"click"}"#).unwrap();
        assert_eq!(s.size, Size::Normal);
        assert_eq!(s.expand_on, ExpandOn::Click);
    }

    #[test]
    fn json_corupt_recuperat_si_redenumit() {
        let d = temp_dir("corupt");
        let p = d.join("settings.json");
        std::fs::write(&p, "{ nu e json").unwrap();
        let (s, note) = Settings::load(&p);
        assert_eq!(s, Settings::default());
        assert_eq!(note, LoadNote::Corrupt);
        assert!(d.join("settings.corrupt.json").exists());
        assert!(!p.exists());
    }

    #[test]
    fn lipsa_fisierului_da_implicite() {
        let d = temp_dir("lipsa");
        let (s, note) = Settings::load(&d.join("settings.json"));
        assert_eq!(s, Settings::default());
        assert_eq!(note, LoadNote::Missing);
    }

    #[test]
    fn salvare_atomica_si_recitire() {
        let d = temp_dir("salvare");
        let p = d.join("settings.json");
        let mut s = Settings::default();
        s.size = Size::Small;
        s.modules.shelf = false;
        s.save(&p).unwrap();
        s.monitor = "\\\\.\\DISPLAY2".into();
        s.save(&p).unwrap();
        let (back, note) = Settings::load(&p);
        assert_eq!(note, LoadNote::Loaded);
        assert_eq!(back, s);
        assert!(!d.join("settings.json.tmp").exists());
    }

    #[test]
    fn scara_marimi() {
        assert_eq!(Size::Small.scale(), 0.85);
        assert_eq!(Size::Normal.scale(), 1.0);
        assert_eq!(Size::Large.scale(), 1.15);
    }
}
