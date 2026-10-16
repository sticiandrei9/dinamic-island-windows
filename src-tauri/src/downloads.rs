//! Descărcările din browser în insulă: urmărește folderul Descărcări; un fișier `.crdownload` (Chrome, Edge)
//! sau `.part` (Firefox) e o descărcare în curs. Emite `downloads` (cele în curs, cu mărime și viteză)
//! și `download-done` când fișierul temporar a devenit fișierul final.

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::claude::Watch;
use crate::settings::SettingsState;

/// Viteza = media pe ultimele 3 s.
const WINDOW_MS: u64 = 3000;
/// Un temporar care nu crește atâta timp e „blocat” și se ascunde.
const STALL_MS: u64 = 60_000;
/// Un fișier nou e rezultatul unui temporar dispărut dacă a fost creat la cel mult atât de dispariție.
const MATCH_MS: u64 = 2000;
/// Un fișier nou, fără temporar văzut, e o descărcare terminată dacă a fost creat de cel mult atât (descărcări foarte scurte).
const FRESH_MS: u64 = 5000;
const TEMP_EXT: [&str; 2] = [".crdownload", ".part"];

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Active {
    pub id: String,
    /// Gol când browserul încă nu a dat numele final (Chrome: „Neconfirmat 123.crdownload”).
    pub name: String,
    pub bytes: u64,
    pub speed: f64,
}

#[derive(Debug, PartialEq)]
pub struct Done {
    pub name: String,
    pub bytes: u64,
}

#[derive(Serialize, Clone)]
struct DoneEvent {
    name: String,
    path: String,
    bytes: u64,
}

// ---------- logica pură ----------

fn strip_temp(name: &str) -> Option<&str> {
    TEMP_EXT.iter().find_map(|ext| {
        let cut = name.len().checked_sub(ext.len())?;
        (name.is_char_boundary(cut) && name[cut..].eq_ignore_ascii_case(ext)).then(|| &name[..cut])
    })
}

/// Numele final dintr-un temporar. Fără extensie e numele provizoriu al browserului
/// („Neconfirmat 123456”), deci nu spune nimic despre fișierul final.
pub fn final_name(temp: &str) -> Option<String> {
    let base = strip_temp(temp)?;
    let dot = base.rfind('.')?;
    (dot > 0 && dot + 1 < base.len()).then(|| base.to_string())
}

struct Temp {
    name: Option<String>,
    created: u64,
    bytes: u64,
    grew_at: u64,
    samples: VecDeque<(u64, u64)>,
}

#[derive(Default)]
pub struct Tracker {
    temps: BTreeMap<String, Temp>,
    /// Fișierele (non-temporare) din ultimul pas: ce nu era aici e „nou”.
    known: HashSet<String>,
    /// După prima listare (pornire sau modul reactivat) ce e deja în folder nu se anunță.
    started: bool,
}

impl Tracker {
    /// `listing` = (nume, mărime, creat_ms) pentru tot folderul. Întoarce descărcările în curs
    /// și pe cele care tocmai s-au terminat (temporarul a dispărut și i-a apărut fișierul final).
    pub fn step(&mut self, listing: &[(String, u64, u64)], now_ms: u64) -> (Vec<Active>, Vec<Done>) {
        let (temps, files): (Vec<_>, Vec<_>) = listing.iter().partition(|(n, _, _)| strip_temp(n).is_some());

        let mut done = Vec::new();
        let mut claimed = HashSet::new();
        // Chrome redenumește „Neconfirmat N” în „nume.ext.crdownload” când începe descărcarea: continuă ca temporarul nou
        let new_temp = temps.iter().any(|(n, _, _)| !self.temps.contains_key(n));
        for (tname, tmp) in &self.temps {
            if temps.iter().any(|(n, _, _)| n == tname) || (tmp.name.is_none() && new_temp) {
                continue;
            }
            let found = match &tmp.name {
                Some(n) => files.iter().find(|(f, _, _)| f == n),
                // redenumirea păstrează data creării temporarului, așa că se potrivește și ea
                None => files.iter().find(|(f, _, c)| {
                    !self.known.contains(f) && !claimed.contains(f) && (c.abs_diff(now_ms) <= MATCH_MS || c.abs_diff(tmp.created) <= MATCH_MS)
                }),
            };
            if let Some((f, size, _)) = found {
                claimed.insert(f.clone());
                done.push(Done { name: f.clone(), bytes: *size });
            }
        }
        if self.started {
            for (f, size, c) in &files {
                if !self.known.contains(f) && !claimed.contains(f) && now_ms.saturating_sub(*c) <= FRESH_MS {
                    done.push(Done { name: f.clone(), bytes: *size });
                }
            }
        }
        self.started = true;

        let mut next = BTreeMap::new();
        for (n, size, created) in temps {
            let mut t = self.temps.remove(n).unwrap_or_else(|| Temp {
                name: final_name(n),
                created: *created,
                bytes: *size,
                grew_at: now_ms,
                samples: VecDeque::new(),
            });
            if *size > t.bytes {
                t.grew_at = now_ms;
            }
            t.bytes = *size;
            t.samples.push_back((now_ms, *size));
            while t.samples.front().is_some_and(|s| s.0 + WINDOW_MS < now_ms) {
                t.samples.pop_front();
            }
            next.insert(n.clone(), t);
        }
        self.temps = next;
        self.known = files.iter().map(|(n, _, _)| n.clone()).collect();

        let active = self
            .temps
            .iter()
            .filter(|(_, t)| now_ms.saturating_sub(t.grew_at) <= STALL_MS)
            .map(|(id, t)| {
                let (t0, b0) = t.samples.front().copied().unwrap_or((now_ms, t.bytes));
                let dt = now_ms.saturating_sub(t0);
                let speed = if dt == 0 { 0.0 } else { t.bytes.saturating_sub(b0) as f64 * 1000.0 / dt as f64 };
                Active { id: id.clone(), name: t.name.clone().unwrap_or_default(), bytes: t.bytes, speed }
            })
            .collect();
        (active, done)
    }
}

// ---------- Win32 ----------

fn downloads_dir() -> Option<PathBuf> {
    use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
    unsafe {
        let p = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None).ok()?;
        let s = p.to_string().ok();
        windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from)
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn list_dir(dir: &std::path::Path) -> Vec<(String, u64, u64)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            // intrarea din director e actualizată leneș pentru un fișier în scriere: la temporare deschidem fișierul (mărimea reală)
            let m = if strip_temp(&name).is_some() { std::fs::metadata(e.path()) } else { e.metadata() }.ok().filter(|m| m.is_file())?;
            let created = m.created().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
            Some((name, m.len(), created))
        })
        .collect()
}

/// Cât `modules.downloads` e pornit: cât e o descărcare în curs, relistează la 1 s (viteza);
/// altfel doarme până la o schimbare în folder (CPU zero când nu se descarcă nimic).
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut tracker = Tracker::default();
        let mut watch: Option<Watch> = None;
        let mut was_active = false;
        let mut relist = true;
        loop {
            if !app.state::<SettingsState>().get().modules.downloads {
                watch = None;
                tracker = Tracker::default();
                if std::mem::take(&mut was_active) {
                    let _ = app.emit("downloads", Vec::<Active>::new());
                }
                relist = true;
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
            let Some(dir) = downloads_dir().filter(|d| d.is_dir()) else {
                std::thread::sleep(Duration::from_secs(5));
                continue;
            };
            let mut busy = false;
            if relist {
                let (active, done) = tracker.step(&list_dir(&dir), now_ms());
                for d in done {
                    let path = dir.join(&d.name).to_string_lossy().into_owned();
                    let _ = app.emit("download-done", DoneEvent { name: d.name, path, bytes: d.bytes });
                }
                busy = !active.is_empty();
                if busy || was_active {
                    let _ = app.emit("downloads", &active);
                }
                was_active = busy;
            }
            if busy {
                std::thread::sleep(Duration::from_secs(1));
                relist = true;
                continue;
            }
            if watch.is_none() {
                watch = Watch::new(&dir);
            }
            // mâner stricat sau lipsă: îl recreăm la următoarea tură (cel vechi se închide prin Drop)
            match watch.as_ref().and_then(|w| w.poll(2000)) {
                Some(changed) => relist = changed,
                None => {
                    watch = None;
                    relist = true;
                    std::thread::sleep(Duration::from_secs(2));
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nume_final() {
        assert_eq!(final_name("film.mp4.crdownload").as_deref(), Some("film.mp4"));
        assert_eq!(final_name("arhiva.zip.part").as_deref(), Some("arhiva.zip"));
        assert_eq!(final_name("Неподтвержденный 123456.crdownload"), None);
        assert_eq!(final_name("Neconfirmat 123456.crdownload"), None);
        assert_eq!(final_name("poza.png"), None);
    }

    #[test]
    fn descarcare_terminata_si_viteza() {
        let mut t = Tracker::default();
        let (a, _) = t.step(&[("x.zip.crdownload".into(), 1_000_000, 0)], 0);
        assert_eq!(a[0].name, "x.zip");
        let (a, _) = t.step(&[("x.zip.crdownload".into(), 4_000_000, 0)], 1000);
        assert!((a[0].speed - 3_000_000.0).abs() < 1.0);
        let (a, d) = t.step(&[("x.zip".into(), 5_000_000, 1500)], 2000);
        assert!(a.is_empty());
        assert_eq!(d[0].name, "x.zip");
    }

    #[test]
    fn nume_temporar_chrome_ia_fisierul_nou() {
        let mut t = Tracker::default();
        t.step(&[("Neconfirmat 1.crdownload".into(), 10, 0), ("vechi.pdf".into(), 5, 0)], 0);
        let (_, d) = t.step(&[("vechi.pdf".into(), 5, 0), ("raport.pdf".into(), 900, 900)], 1000);
        assert_eq!(d[0].name, "raport.pdf");
    }

    #[test]
    fn anulata_nu_e_terminata() {
        let mut t = Tracker::default();
        t.step(&[("x.zip.crdownload".into(), 10, 0)], 0);
        let (a, d) = t.step(&[], 1000);
        assert!(a.is_empty() && d.is_empty());
    }

    #[test]
    fn descarcare_rapida_fara_temporar() {
        let mut t = Tracker::default();
        t.step(&[("vechi.pdf".into(), 5, 0)], 100_000);
        let (a, d) = t.step(&[("vechi.pdf".into(), 5, 0), ("mic.zip".into(), 700, 100_500)], 101_000);
        assert!(a.is_empty());
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].name, "mic.zip");
        // fișierul vechi nu devine „terminat”, iar cel anunțat o dată nu se mai anunță
        let (_, d) = t.step(&[("vechi.pdf".into(), 5, 0), ("mic.zip".into(), 700, 100_500)], 102_000);
        assert!(d.is_empty());
    }

    #[test]
    fn prima_listare_nu_anunta() {
        let mut t = Tracker::default();
        let (_, d) = t.step(&[("proaspat.zip".into(), 700, 99_900)], 100_000);
        assert!(d.is_empty());
    }

    #[test]
    fn redenumire_temporar_nu_e_terminata() {
        let mut t = Tracker::default();
        t.step(&[("Neconfirmat 1.crdownload".into(), 10, 0)], 0);
        // Chrome redenumește temporarul când începe descărcarea; „vechi.pdf” (creat odată cu el) nu e rezultatul lui
        let (a, d) = t.step(&[("film.mp4.crdownload".into(), 20, 9000), ("vechi.pdf".into(), 5, 0)], 10_000);
        assert!(d.is_empty());
        assert_eq!(a[0].name, "film.mp4");
    }

    #[test]
    fn blocata_60s_ascunsa() {
        let mut t = Tracker::default();
        t.step(&[("x.zip.crdownload".into(), 10, 0)], 0);
        let (a, _) = t.step(&[("x.zip.crdownload".into(), 10, 0)], 61_000);
        assert!(a.is_empty());
    }
}
