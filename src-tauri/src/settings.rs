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
    pub calls: bool,
    pub screenshots: bool,
    pub shelf: bool,
    pub mirror: bool,
    pub keyboard: bool,
    pub network: bool,
    pub output: bool,
    pub calendar: bool,
    pub claude: bool,
    pub downloads: bool,
    pub unlock: bool,
    pub system: bool,
    pub clipboard: bool,
    pub sleep: bool,
    pub notes: bool,
}

impl Default for Modules {
    fn default() -> Self {
        Self {
            media: true, lyrics: true, apps: true, timer: true, privacy: true,
            alerts: true, volume: true, notifications: true, calls: true, screenshots: true, shelf: true, mirror: true,
            keyboard: true, network: true, output: true, calendar: true,
            claude: false, downloads: true, unlock: true, system: true, clipboard: false, sleep: true, notes: true,
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
    /// "auto" (după limba Windows), "ro", "en" sau "ru"
    #[serde(deserialize_with = "lenient")]
    pub language: String,
    /// scurtătura care deschide insula, de ex. "Win+Shift+I"; goală = niciuna
    #[serde(deserialize_with = "lenient")]
    pub hotkey: String,
    /// fișierele de pe raft mai vechi de atâtea zile ies singure; 0 = niciodată
    #[serde(deserialize_with = "lenient")]
    pub shelf_days: u32,
    /// linkul secret iCal (Google Calendar); gol = fără calendar
    #[serde(deserialize_with = "lenient")]
    pub calendar_url: String,
    /// scurtătura care deschide notițele; goală = niciuna (sugestia din Setări: Win+Shift+N)
    #[serde(deserialize_with = "lenient")]
    pub notes_hotkey: String,
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
            language: "auto".into(),
            hotkey: String::new(),
            shelf_days: 0,
            calendar_url: String::new(),
            notes_hotkey: String::new(),
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

impl Settings {
    /// Limba efectivă: cea aleasă sau, la „auto”, limba interfeței Windows.
    pub fn lang(&self) -> crate::i18n::Lang {
        crate::i18n::resolve(&self.language, crate::i18n::windows_ui_lang())
    }
}

/// Setările trimise interfeței: plus limba efectivă (calculată, nu se salvează).
pub fn payload(s: &Settings) -> serde_json::Value {
    let mut v = serde_json::to_value(s).unwrap_or_default();
    v["resolved_language"] = s.lang().code().into();
    v
}

#[tauri::command]
pub fn get_settings(state: tauri::State<SettingsState>) -> serde_json::Value {
    payload(&state.get())
}

#[tauri::command]
pub fn set_settings(app: AppHandle, state: tauri::State<SettingsState>, mut settings: Settings) -> Result<(), String> {
    let previous = state.get();
    if previous.autostart != settings.autostart && !crate::tray::apply_autostart(&app, settings.autostart) {
        settings.autostart = previous.autostart; // Windows a refuzat: setarea rămâne cum era
    }
    settings.save(&state.path).map_err(|e| e.to_string())?;
    // comutatorul Claude instalează / scoate hook-ul, abia după ce setarea e salvată; dacă instalarea
    // eșuează, comutatorul rămâne oprit (și se salvează din nou), iar motivul ajunge în Setări
    if previous.modules.claude != settings.modules.claude {
        if settings.modules.claude {
            if crate::claude::install_hook() != crate::claude_hook::HookStatus::Installed {
                settings.modules.claude = false;
                settings.save(&state.path).map_err(|e| e.to_string())?;
            }
        } else {
            crate::claude::remove_hook();
        }
    }
    *state.current.lock().unwrap() = settings.clone();
    crate::tray::refresh_autostart_item(&app, settings.autostart);
    if previous.hotkey != settings.hotkey {
        crate::hotkey::apply_and_report(&settings.hotkey);
    }
    if previous.notes_hotkey != settings.notes_hotkey {
        crate::hotkey::apply_notes_and_report(&settings.notes_hotkey);
    }
    if previous.language != settings.language {
        crate::i18n::set_current(settings.lang());
        crate::tray::refresh_texts(&app);
        if let Some(w) = tauri::Manager::get_webview_window(&app, "settings") {
            let _ = w.set_title(&crate::i18n::t("settings.title"));
        }
    }
    let _ = app.emit("settings-changed", payload(&settings));
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

/// Erorile din interfață (JS), scrise în log-ul aplicației.
#[tauri::command]
pub fn log_js(msg: String) {
    log::warn!("JS: {msg}");
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
        assert!(m.calls && m.screenshots && m.keyboard && m.network && m.output && m.calendar);
        assert!(s.hotkey.is_empty() && s.calendar_url.is_empty());
        assert_eq!(s.shelf_days, 0);
        assert_eq!(s.language, "auto");
    }

    #[test]
    fn limba_efectiva_trimisa_interfetei() {
        let mut s = Settings::default();
        s.language = "ru".into();
        let v = payload(&s);
        assert_eq!(v["resolved_language"], "ru");
        assert_eq!(v["language"], "ru");
        // câmpul calculat nu strică citirea setărilor trimise înapoi de interfață
        let back: Settings = serde_json::from_value(v).unwrap();
        assert_eq!(back, s);
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

    #[test]
    fn module_noi_implicite() {
        let m = Settings::default().modules;
        assert!(!m.claude && !m.clipboard);
        assert!(m.downloads && m.unlock && m.system && m.sleep && m.notes);
        assert!(Settings::default().notes_hotkey.is_empty());
    }

    #[test]
    fn setari_v043_primesc_implicitele_noi() {
        // modules din v0.4.3, fara cheile noi
        let s: Settings = serde_json::from_str(r#"{"modules":{"media":true,"lyrics":false,"apps":true,"timer":true,"privacy":true,"alerts":true,"volume":true,"notifications":true,"calls":true,"screenshots":true,"shelf":true,"mirror":true,"keyboard":true,"network":true,"output":true,"calendar":true},"hotkey":"Ctrl+Shift+I"}"#).unwrap();
        assert!(!s.modules.lyrics);
        assert!(!s.modules.claude && !s.modules.clipboard);
        assert!(s.modules.downloads && s.modules.notes);
        assert_eq!(s.hotkey, "Ctrl+Shift+I");
        assert!(s.notes_hotkey.is_empty());
    }
}
