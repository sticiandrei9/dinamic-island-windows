//! Descărcările din browser în insulă: urmărește folderul Descărcări; un fișier `.crdownload` (Chrome, Edge)
//! sau `.part` (Firefox) e o descărcare în curs. Emite `downloads` (cele în curs, cu mărime și viteză)
//! și `download-done` când fișierul temporar a devenit fișierul final.

use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

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

/// O intrare din folderul Descărcări.
pub struct Entry {
    pub name: String,
    pub bytes: u64,
    pub created: u64,
    /// atribut ascuns sau de sistem (fișierele de blocare Office, de pildă)
    pub hidden: bool,
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
    /// `listing` = tot folderul. Întoarce descărcările în curs
    /// și pe cele care tocmai s-au terminat (temporarul a dispărut și i-a apărut fișierul final).
    pub fn step(&mut self, listing: &[Entry], now_ms: u64) -> (Vec<Active>, Vec<Done>) {
        let (temps, files): (Vec<_>, Vec<_>) = listing.iter().partition(|e| strip_temp(&e.name).is_some());

        // nu sunt descărcări: fișiere goale, ascunse, de blocare (Office, LibreOffice) sau finale cu temporarul încă alături
        let has_temp = |f: &Entry| temps.iter().any(|t| strip_temp(&t.name).is_some_and(|b| b.eq_ignore_ascii_case(&f.name)));
        let real = |f: &Entry| f.bytes > 0 && !f.hidden && !f.name.starts_with("~$") && !f.name.starts_with(".~lock.") && !has_temp(f);

        let mut done = Vec::new();
        let mut claimed = HashSet::new();
        // Chrome redenumește „Neconfirmat N” în „nume.ext.crdownload” când începe descărcarea: continuă ca temporarul nou
        let new_temp = temps.iter().any(|e| !self.temps.contains_key(&e.name));
        for (tname, tmp) in &self.temps {
            if temps.iter().any(|e| e.name == *tname) || (tmp.name.is_none() && new_temp) {
                continue;
            }
            let found = match &tmp.name {
                Some(n) => files.iter().find(|f| f.name == *n),
                // redenumirea păstrează data creării temporarului, așa că se potrivește și ea
                None => files.iter().find(|f| {
                    !self.known.contains(&f.name)
                        && !claimed.contains(&f.name)
                        && real(f)
                        && (f.created.abs_diff(now_ms) <= MATCH_MS || f.created.abs_diff(tmp.created) <= MATCH_MS)
                }),
            };
            if let Some(f) = found {
                claimed.insert(f.name.clone());
                done.push(Done { name: f.name.clone(), bytes: f.bytes });
            }
        }
        if self.started {
            for f in &files {
                if !self.known.contains(&f.name) && !claimed.contains(&f.name) && now_ms.saturating_sub(f.created) <= FRESH_MS && real(f) {
                    done.push(Done { name: f.name.clone(), bytes: f.bytes });
                }
            }
        }
        // temporarele găsite la prima listare (rămase de la o descărcare întreruptă) apar doar după ce cresc
        let first = !std::mem::replace(&mut self.started, true);

        let mut next = BTreeMap::new();
        for e in temps {
            let (n, size) = (&e.name, &e.bytes);
            let mut t = self.temps.remove(n).unwrap_or_else(|| Temp {
                name: final_name(n),
                created: e.created,
                bytes: *size,
                grew_at: if first { 0 } else { now_ms },
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
        self.known = files.iter().map(|f| f.name.clone()).collect();

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

fn list_dir(dir: &std::path::Path) -> Vec<Entry> {
    use std::os::windows::fs::MetadataExt;
    use windows::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_SYSTEM};
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            // intrarea din director e actualizată leneș pentru un fișier în scriere: la temporare deschidem fișierul (mărimea reală)
            let m = if strip_temp(&name).is_some() { std::fs::metadata(e.path()) } else { e.metadata() }.ok().filter(|m| m.is_file())?;
            let created = m.created().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
            let hidden = m.file_attributes() & (FILE_ATTRIBUTE_HIDDEN.0 | FILE_ATTRIBUTE_SYSTEM.0) != 0;
            Some(Entry { name, bytes: m.len(), created, hidden })
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
        let mut listed_at: Option<Instant> = None;
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
                // cel mult o listare pe secundă, și când folderul se schimbă încontinuu (torrent, launcher)
                if let Some(wait) = listed_at.and_then(|t| Duration::from_secs(1).checked_sub(t.elapsed())) {
                    std::thread::sleep(wait);
                }
                listed_at = Some(Instant::now());
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

    fn e(name: &str, bytes: u64, created: u64) -> Entry {
        Entry { name: name.into(), bytes, created, hidden: false }
    }

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
        let (a, _) = t.step(&[e("x.zip.crdownload", 1_000_000, 0)], 0);
        assert_eq!(a[0].name, "x.zip");
        let (a, _) = t.step(&[e("x.zip.crdownload", 4_000_000, 0)], 1000);
        assert!((a[0].speed - 3_000_000.0).abs() < 1.0);
        let (a, d) = t.step(&[e("x.zip", 5_000_000, 1500)], 2000);
        assert!(a.is_empty());
        assert_eq!(d[0].name, "x.zip");
    }

    #[test]
    fn nume_temporar_chrome_ia_fisierul_nou() {
        let mut t = Tracker::default();
        t.step(&[e("Neconfirmat 1.crdownload", 10, 0), e("vechi.pdf", 5, 0)], 0);
        let (_, d) = t.step(&[e("vechi.pdf", 5, 0), e("raport.pdf", 900, 900)], 1000);
        assert_eq!(d[0].name, "raport.pdf");
    }

    #[test]
    fn temporar_neconfirmat_nu_ia_fisiere_de_blocare_sau_goale() {
        // descărcarea „Neconfirmat” e anulată exact când se deschide un document Office sau apare un fișier gol
        let mut t = Tracker::default();
        t.step(&[e("Neconfirmat 1.crdownload", 10, 0)], 0);
        let (_, d) = t.step(&[e("~$raport.docx", 162, 900), e("gol.zip", 0, 900)], 1000);
        assert!(d.is_empty());
    }

    #[test]
    fn anulata_nu_e_terminata() {
        let mut t = Tracker::default();
        t.step(&[e("x.zip.crdownload", 10, 0)], 0);
        let (a, d) = t.step(&[], 1000);
        assert!(a.is_empty() && d.is_empty());
    }

    #[test]
    fn descarcare_rapida_fara_temporar() {
        let mut t = Tracker::default();
        t.step(&[e("vechi.pdf", 5, 0)], 100_000);
        let (a, d) = t.step(&[e("vechi.pdf", 5, 0), e("mic.zip", 700, 100_500)], 101_000);
        assert!(a.is_empty());
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].name, "mic.zip");
        // fișierul vechi nu devine „terminat”, iar cel anunțat o dată nu se mai anunță
        let (_, d) = t.step(&[e("vechi.pdf", 5, 0), e("mic.zip", 700, 100_500)], 102_000);
        assert!(d.is_empty());
    }

    #[test]
    fn prima_listare_nu_anunta() {
        let mut t = Tracker::default();
        let (_, d) = t.step(&[e("proaspat.zip", 700, 99_900)], 100_000);
        assert!(d.is_empty());
    }

    #[test]
    fn redenumire_temporar_nu_e_terminata() {
        let mut t = Tracker::default();
        t.step(&[e("Neconfirmat 1.crdownload", 10, 0)], 0);
        // Chrome redenumește temporarul când începe descărcarea; „vechi.pdf” (creat odată cu el) nu e rezultatul lui
        let (a, d) = t.step(&[e("film.mp4.crdownload", 20, 9000), e("vechi.pdf", 5, 0)], 10_000);
        assert!(d.is_empty());
        assert_eq!(a[0].name, "film.mp4");
    }

    /// Pasul după prima listare, cu un fișier nou apărut acum (ar fi anunțat ca descărcare rapidă).
    fn fresh(extra: Vec<Entry>) -> Vec<Done> {
        let mut t = Tracker::default();
        t.step(&[e("vechi.pdf", 5, 0)], 100_000);
        let mut listing = vec![e("vechi.pdf", 5, 0)];
        listing.extend(extra);
        t.step(&listing, 101_000).1
    }

    #[test]
    fn fisier_gol_nu_e_terminat() {
        // Firefox creează „x.zip” gol lângă „x.zip.part” la început
        assert!(fresh(vec![e("gol.zip", 0, 100_500)]).is_empty());
    }

    #[test]
    fn fisier_ascuns_nu_e_terminat() {
        let mut h = e("ascuns.dat", 700, 100_500);
        h.hidden = true;
        assert!(fresh(vec![h]).is_empty());
    }

    #[test]
    fn fisier_de_blocare_office_nu_e_terminat() {
        assert!(fresh(vec![e("~$raport.docx", 162, 100_500)]).is_empty());
        assert!(fresh(vec![e(".~lock.tabel.xlsx#", 90, 100_500)]).is_empty());
    }

    #[test]
    fn fisier_cu_temporar_alaturi_nu_e_terminat() {
        assert!(fresh(vec![e("x.zip", 700, 100_500), e("x.zip.part", 900, 100_500)]).is_empty());
        assert!(fresh(vec![e("y.iso", 700, 100_500), e("y.iso.crdownload", 900, 100_500)]).is_empty());
    }

    #[test]
    fn temporar_orfan_la_pornire_apare_doar_cand_creste() {
        let mut t = Tracker::default();
        // rămas de la o descărcare întreruptă: nu e „în curs” 60 s
        let (a, _) = t.step(&[e("vechi.zip.part", 10, 0)], 100_000);
        assert!(a.is_empty());
        let (a, _) = t.step(&[e("vechi.zip.part", 10, 0)], 101_000);
        assert!(a.is_empty());
        let (a, _) = t.step(&[e("vechi.zip.part", 20, 0)], 102_000);
        assert_eq!(a.len(), 1); // a crescut: e o descărcare reală
        // un temporar apărut după pornire se vede imediat
        let (a, _) = t.step(&[e("vechi.zip.part", 20, 0), e("nou.zip.part", 5, 102_500)], 103_000);
        assert!(a.iter().any(|x| x.name == "nou.zip"));
    }

    #[test]
    fn blocata_60s_ascunsa() {
        let mut t = Tracker::default();
        t.step(&[e("x.zip.crdownload", 10, 0)], 0);
        let (a, _) = t.step(&[e("x.zip.crdownload", 10, 0)], 61_000);
        assert!(a.is_empty());
    }
}
