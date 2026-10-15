use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

/// Același dicționar ca interfața (`src/locales.json`), inclus în exe.
const LOCALES: &str = include_str!("../../src/locales.json");

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Lang {
    Ro,
    En,
    Ru,
}

impl Lang {
    pub fn code(self) -> &'static str {
        match self {
            Lang::Ro => "ro",
            Lang::En => "en",
            Lang::Ru => "ru",
        }
    }
}

/// Setarea `language` („auto” sau un cod) + limba interfeței Windows (LANGID) → limba aplicației.
pub fn resolve(setting: &str, ui_langid: u16) -> Lang {
    match setting {
        "ro" => Lang::Ro,
        "en" => Lang::En,
        "ru" => Lang::Ru,
        _ => match ui_langid & 0x3ff {
            0x18 => Lang::Ro,
            0x19 => Lang::Ru,
            _ => Lang::En,
        },
    }
}

pub fn windows_ui_lang() -> u16 {
    unsafe { windows::Win32::Globalization::GetUserDefaultUILanguage() }
}

static CURRENT: AtomicU8 = AtomicU8::new(0);

pub fn set_current(l: Lang) {
    CURRENT.store(l as u8, Ordering::SeqCst);
}

pub fn current() -> Lang {
    match CURRENT.load(Ordering::SeqCst) {
        1 => Lang::En,
        2 => Lang::Ru,
        _ => Lang::Ro,
    }
}

type Table = HashMap<String, HashMap<String, serde_json::Value>>;

fn table() -> &'static Table {
    static T: OnceLock<Table> = OnceLock::new();
    T.get_or_init(|| serde_json::from_str(LOCALES).expect("locales.json invalid"))
}

/// Textul în limba dată; lipsă → română → cheia însăși.
pub fn tr(lang: Lang, key: &str) -> String {
    let get = |l: &str| table().get(l).and_then(|m| m.get(key)).and_then(|v| v.as_str()).map(str::to_string);
    get(lang.code()).or_else(|| get("ro")).unwrap_or_else(|| key.to_string())
}

pub fn t(key: &str) -> String {
    tr(current(), key)
}

/// Textul cu parametrul `{v}` înlocuit.
pub fn tv(key: &str, v: &str) -> String {
    t(key).replace("{v}", v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limba_aleasa_sau_dupa_windows() {
        assert_eq!(resolve("ro", 0x0409), Lang::Ro);
        assert_eq!(resolve("ru", 0x0418), Lang::Ru);
        assert_eq!(resolve("auto", 0x0418), Lang::Ro); // ro-RO
        assert_eq!(resolve("auto", 0x0818), Lang::Ro); // ro-MD
        assert_eq!(resolve("auto", 0x0419), Lang::Ru); // ru-RU
        assert_eq!(resolve("auto", 0x0407), Lang::En); // de-DE → engleză
        assert_eq!(resolve("ceva", 0x0409), Lang::En);
    }

    #[test]
    fn texte_in_cele_trei_limbi() {
        assert_eq!(tr(Lang::Ro, "tray.quit"), "Ieșire");
        assert_eq!(tr(Lang::En, "tray.quit"), "Quit");
        assert_eq!(tr(Lang::Ru, "tray.quit"), "Выход");
        assert_eq!(tr(Lang::En, "nu.exista"), "nu.exista");
    }

    #[test]
    fn cheile_folosite_in_rust_exista() {
        let used = [
            "tray.apply", "tray.checking", "tray.downloading", "tray.check", "tray.settings", "tray.autostart", "tray.quit",
            "settings.title", "update.notifTitle", "update.notifBody", "alert.charging", "alert.onBattery",
            "alert.lowBattery", "alert.connected", "alert.disconnected", "shot.file",
        ];
        for l in [Lang::Ro, Lang::En, Lang::Ru] {
            for k in used {
                assert!(table()[l.code()].get(k).and_then(|v| v.as_str()).is_some(), "{}: lipsește {k}", l.code());
            }
        }
    }
}
