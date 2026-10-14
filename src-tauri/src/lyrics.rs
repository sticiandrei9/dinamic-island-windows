use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Un rând de versuri sincronizate: momentul (secunde) și textul.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Line {
    pub t: f64,
    pub text: String,
}

#[derive(Deserialize, Clone, Debug)]
pub struct Track {
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub album: String,
    pub duration: f64,
}

/// Rezultat LRCLIB (doar câmpurile folosite).
#[derive(Deserialize, Clone, Debug)]
pub struct Found {
    #[serde(default)]
    pub duration: f64,
    #[serde(rename = "syncedLyrics")]
    pub synced: Option<String>,
}

fn parse_stamp(s: &str) -> Option<f64> {
    let (m, rest) = s.split_once(':')?;
    let m: f64 = m.trim().parse().ok()?;
    let sec: f64 = rest.trim().parse().ok()?;
    Some(m * 60.0 + sec)
}

/// `[mm:ss.xx]text`, mai multe timpi pe un rând, rânduri goale = pauză (♪), etichetele `[ar:]` ignorate.
pub fn parse_lrc(text: &str) -> Vec<Line> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut stamps = Vec::new();
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            match parse_stamp(&stripped[..end]) {
                Some(t) => stamps.push(t),
                None => break, // etichetă ([ar:…]) sau text cu paranteze
            }
            rest = stripped[end + 1..].trim();
        }
        let text = if rest.is_empty() { "♪".to_string() } else { rest.to_string() };
        for t in stamps {
            out.push(Line { t: (t * 100.0).round() / 100.0, text: text.clone() });
        }
    }
    out.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
    out
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

pub fn get_url(t: &Track) -> String {
    let mut url = format!("https://lrclib.net/api/get?track_name={}&artist_name={}", encode(&t.title), encode(&t.artist));
    if !t.album.is_empty() {
        url.push_str(&format!("&album_name={}", encode(&t.album)));
    }
    url.push_str(&format!("&duration={}", t.duration.round() as i64));
    url
}

pub fn search_url(t: &Track) -> String {
    format!("https://lrclib.net/api/search?track_name={}&artist_name={}", encode(&t.title), encode(&t.artist))
}

/// Versuri folosibile: cel puțin 3 rânduri (LRCLIB are și intrări-gunoi, de ex. „[00:00.00]probe”).
pub fn usable(lines: &[Line]) -> bool {
    lines.len() >= 3
}

/// Din rezultatele căutării: primul cu versuri sincronizate folosibile și durată la ±3 s.
pub fn pick_result(results: &[Found], duration: f64) -> Option<&Found> {
    results.iter().find(|r| {
        (r.duration - duration).abs() <= 3.0 && r.synced.as_deref().is_some_and(|s| usable(&parse_lrc(s)))
    })
}

/// Ultimele 50 de piese (inclusiv „fără versuri”, ca să nu fie cerute din nou).
#[derive(Default)]
pub struct Cache {
    items: VecDeque<(String, Option<Vec<Line>>)>,
}

impl Cache {
    pub fn get(&self, key: &str) -> Option<&Option<Vec<Line>>> {
        self.items.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn put(&mut self, key: String, value: Option<Vec<Line>>) {
        self.items.retain(|(k, _)| *k != key);
        self.items.push_back((key, value));
        while self.items.len() > 50 {
            self.items.pop_front();
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }
}

#[derive(Default)]
pub struct LyricsCache(pub Mutex<Cache>);

fn fetch(t: &Track) -> Option<Vec<Line>> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(8))
        .user_agent(&format!(
            "dynamic-island-windows v{} (https://github.com/sticiandrei9/dinamic-island-windows)",
            env!("CARGO_PKG_VERSION")
        ))
        .build();
    let direct = agent.get(&get_url(t)).call().ok().and_then(|r| r.into_json::<Found>().ok());
    if let Some(lines) = direct.and_then(|f| f.synced).map(|s| parse_lrc(&s)).filter(|l| usable(l)) {
        return Some(lines);
    }
    let results: Vec<Found> = agent.get(&search_url(t)).call().ok()?.into_json().ok()?;
    Some(parse_lrc(pick_result(&results, t.duration)?.synced.as_deref()?))
}

/// Versurile sincronizate ale piesei, din cache sau de pe LRCLIB. None = nu există / fără rețea.
#[tauri::command]
pub async fn get_lyrics(track: Track, state: tauri::State<'_, LyricsCache>) -> Result<Option<Vec<Line>>, String> {
    if track.title.trim().is_empty() || track.artist.trim().is_empty() || !(track.duration > 0.0) {
        return Ok(None);
    }
    let key = format!("{}|{}|{}", track.artist, track.title, track.duration.round() as i64);
    if let Some(hit) = state.0.lock().unwrap().get(&key) {
        return Ok(hit.clone());
    }
    let found = tauri::async_runtime::spawn_blocking(move || fetch(&track)).await.map_err(|e| e.to_string())?;
    log::debug!("versuri: {} rânduri", found.as_ref().map_or(0, |l| l.len()));
    state.0.lock().unwrap().put(key, found.clone());
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parseaza_lrc() {
        let lrc = "[ar:Rick Astley]\n[ti:Never Gonna]\n[00:18.50]We're no strangers to love\n[00:22.10][01:30.00]You know the rules\n[00:25.00]\n";
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0], Line { t: 18.5, text: "We're no strangers to love".into() });
        assert_eq!(lines[1].t, 22.1);
        assert_eq!(lines[2], Line { t: 25.0, text: "♪".into() }); // pauză instrumentală
        assert_eq!(lines[3], Line { t: 90.0, text: "You know the rules".into() });
    }

    #[test]
    fn lrc_gol_sau_fara_timpi() {
        assert!(parse_lrc("").is_empty());
        assert!(parse_lrc("doar text fără timp").is_empty());
    }

    #[test]
    fn url_codificat_corect() {
        let t = Track { title: "Să mă ierți & să uiți".into(), artist: "Carla's Dreams".into(), album: "".into(), duration: 201.4 };
        assert_eq!(
            get_url(&t),
            "https://lrclib.net/api/get?track_name=S%C4%83%20m%C4%83%20ier%C8%9Bi%20%26%20s%C4%83%20ui%C8%9Bi&artist_name=Carla%27s%20Dreams&duration=201"
        );
        assert_eq!(search_url(&t), "https://lrclib.net/api/search?track_name=S%C4%83%20m%C4%83%20ier%C8%9Bi%20%26%20s%C4%83%20ui%C8%9Bi&artist_name=Carla%27s%20Dreams");
    }

    #[test]
    fn rezultatul_cautarii_cu_durata_apropiata() {
        let three = |x: &str| Some(format!("[00:01.00]{x}
[00:02.00]{x}
[00:03.00]{x}"));
        let results = vec![
            Found { duration: 150.0, synced: three("a") },
            Found { duration: 202.0, synced: None },
            Found { duration: 203.0, synced: three("b") },
        ];
        assert_eq!(pick_result(&results, 201.4).and_then(|r| r.synced.clone()), three("b"));
        assert!(pick_result(&results, 400.0).is_none());
    }

    #[test]
    fn versuri_degenerate_sunt_ignorate() {
        assert!(!usable(&parse_lrc("[00:00.00]probe")));
        assert!(usable(&parse_lrc("[00:01.00]a
[00:02.00]b
[00:03.00]c")));
        let results = vec![
            Found { duration: 214.0, synced: Some("[00:00.00]probe".into()) },
            Found { duration: 213.0, synced: Some("[00:01.00]a
[00:02.00]b
[00:03.00]c".into()) },
        ];
        assert_eq!(pick_result(&results, 213.0).map(|r| r.duration), Some(213.0));
    }

    #[test]
    fn cache_limitat() {
        let mut c = Cache::default();
        for i in 0..60 {
            c.put(format!("k{i}"), None);
        }
        assert_eq!(c.len(), 50);
        assert!(c.get("k0").is_none() && c.get("k59").is_some());
    }

    /// Rulează doar manual: o cerere reală către LRCLIB.
    #[test]
    #[ignore]
    fn cerere_reala_lrclib() {
        let t = Track { title: "Never Gonna Give You Up".into(), artist: "Rick Astley".into(), album: "".into(), duration: 213.0 };
        let lines = fetch(&t);
        println!("rânduri: {:?}", lines.as_ref().map(|l| l.len()));
        if let Some(l) = lines {
            for x in l.iter().take(3) {
                println!("  {:>6.2} {}", x.t, x.text);
            }
        }
    }
}
