use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD, Engine};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager as _};
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session,
    GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionMediaProperties as Props,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;
use windows::Win32::Foundation::E_FAIL;

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct MediaInfo {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub thumbnail: Option<String>,
    pub playing: bool,
    pub position: f64,
    pub duration: f64,
    pub source: String,
}

const TICKS_PER_SEC: f64 = 10_000_000.0;
const UNIX_TO_FILETIME_SECS: i64 = 11_644_473_600;

fn now_universal_ticks() -> i64 {
    let d = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    (d.as_secs() as i64 + UNIX_TO_FILETIME_SECS) * 10_000_000 + (d.subsec_nanos() / 100) as i64
}

/// Poziția în secunde acum, pornind de la poziția raportată la `last_updated`.
pub fn live_position(position: i64, last_updated: i64, now: i64, playing: bool, end: i64) -> f64 {
    let mut pos = position as f64 / TICKS_PER_SEC;
    if playing && last_updated > 0 && now > last_updated {
        pos += (now - last_updated) as f64 / TICKS_PER_SEC;
    }
    if end > 0 {
        pos = pos.min(end as f64 / TICKS_PER_SEC);
    }
    pos.max(0.0)
}

pub fn friendly_source(aumid: &str) -> String {
    let l = aumid.to_lowercase();
    let known = [
        ("spotify", "Spotify"), ("chrome", "Chrome"), ("msedge", "Edge"), ("firefox", "Firefox"),
        ("308046b0af4a39cb", "Firefox"), ("opera", "Opera"), ("brave", "Brave"), ("vlc", "VLC"),
        ("zunemusic", "Media Player"), ("applemusic", "Apple Music"),
    ];
    for (k, name) in known {
        if l.contains(k) {
            return name.to_string();
        }
    }
    let base = aumid.rsplit(['\\', '!']).next().unwrap_or(aumid);
    base.strip_suffix(".exe").or_else(|| base.strip_suffix(".EXE")).unwrap_or(base).to_string()
}

pub fn should_emit(prev: &Option<MediaInfo>, cur: &Option<MediaInfo>, secs_since_emit: f64) -> bool {
    match (prev, cur) {
        (None, None) => false,
        (Some(_), None) | (None, Some(_)) => true,
        (Some(p), Some(c)) => {
            if p.title != c.title || p.artist != c.artist || p.album != c.album || p.playing != c.playing
                || p.source != c.source || p.thumbnail != c.thumbnail || (p.duration - c.duration).abs() > 0.5
            {
                return true;
            }
            let expected = p.position + if p.playing { secs_since_emit } else { 0.0 };
            (c.position - expected).abs() > 1.5
        }
    }
}

/// Alege sesiunea care rulează; altfel sesiunea curentă a Windows; altfel prima.
pub fn choose_session_index(playing: &[bool], current: Option<usize>) -> Option<usize> {
    playing.iter().position(|p| *p).or(current).or(if playing.is_empty() { None } else { Some(0) })
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum MediaAction {
    PlayPause,
    Next,
    Prev,
}

impl MediaAction {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "play_pause" => Some(Self::PlayPause),
            "next" => Some(Self::Next),
            "prev" => Some(Self::Prev),
            _ => None,
        }
    }
}

/// Coperta e citită o dată pe piesă; dacă lipsește (Spotify o trimite cu întârziere), se reîncearcă.
pub struct ThumbCache {
    key: String,
    value: Option<String>,
    tries: u32,
    last_try: Option<Instant>,
    retry_every: Duration,
}

impl Default for ThumbCache {
    fn default() -> Self {
        Self::with_retry(Duration::from_millis(1500))
    }
}

impl ThumbCache {
    pub fn with_retry(retry_every: Duration) -> Self {
        Self { key: String::new(), value: None, tries: 0, last_try: None, retry_every }
    }

    pub fn get(&mut self, key: &str, load: impl FnOnce() -> Option<String>) -> Option<String> {
        if self.key != key {
            self.key = key.to_string();
            self.value = None;
            self.tries = 0;
            self.last_try = None;
        }
        let due = self.last_try.map_or(true, |t| t.elapsed() >= self.retry_every);
        if self.value.is_none() && self.tries < 6 && due {
            self.tries += 1;
            self.last_try = Some(Instant::now());
            self.value = load();
        }
        self.value.clone()
    }
}

/// Ultima stare emisă; interfața o cere la pornire, pentru că evenimentele trimise
/// înainte ca pagina să asculte se pierd.
#[derive(Default, Clone)]
pub struct LastMedia(Arc<Mutex<Option<MediaInfo>>>);

impl LastMedia {
    pub fn get(&self) -> Option<MediaInfo> {
        self.0.lock().unwrap().clone()
    }

    pub fn set(&self, info: Option<MediaInfo>) {
        *self.0.lock().unwrap() = info;
    }
}

#[tauri::command]
pub fn media_current(state: tauri::State<LastMedia>) -> Option<MediaInfo> {
    state.get()
}

fn pick_session(m: &Manager) -> Option<Session> {
    let list: Vec<Session> = m.GetSessions().map(|v| v.into_iter().collect()).unwrap_or_default();
    let playing: Vec<bool> = list
        .iter()
        .map(|s| s.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).map(|st| st == Status::Playing).unwrap_or(false))
        .collect();
    let current_id = m.GetCurrentSession().and_then(|s| s.SourceAppUserModelId()).ok();
    let current = current_id.and_then(|id| list.iter().position(|s| s.SourceAppUserModelId().ok().as_ref() == Some(&id)));
    choose_session_index(&playing, current).and_then(|i| list.get(i).cloned())
}

fn read_thumbnail(props: &Props) -> Option<String> {
    let read = || -> windows::core::Result<String> {
        let stream = props.Thumbnail()?.OpenReadAsync()?.get()?;
        let size = stream.Size()? as u32;
        if size == 0 || size > 8 * 1024 * 1024 {
            return Err(E_FAIL.into());
        }
        let reader = DataReader::CreateDataReader(&stream)?;
        reader.LoadAsync(size)?.get()?;
        let mut buf = vec![0u8; size as usize];
        reader.ReadBytes(&mut buf)?;
        let ct = stream.ContentType().map(|s| s.to_string()).unwrap_or_default();
        let ct = if ct.starts_with("image/") { ct } else { "image/png".to_string() };
        Ok(format!("data:{ct};base64,{}", STANDARD.encode(&buf)))
    };
    read().ok()
}

fn read_session(session: &Session, cache: &mut ThumbCache) -> windows::core::Result<MediaInfo> {
    let props = session.TryGetMediaPropertiesAsync()?.get()?;
    let title = props.Title()?.to_string();
    let artist = props.Artist().map(|s| s.to_string()).unwrap_or_default();
    let album = props.AlbumTitle().map(|s| s.to_string()).unwrap_or_default();
    let playing = session.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing;
    let (position, duration) = match session.GetTimelineProperties() {
        Ok(tl) => {
            let pos = tl.Position().map(|t| t.Duration).unwrap_or(0);
            let end = tl.EndTime().map(|t| t.Duration).unwrap_or(0);
            let upd = tl.LastUpdatedTime().map(|t| t.UniversalTime).unwrap_or(0);
            (live_position(pos, upd, now_universal_ticks(), playing, end), end as f64 / TICKS_PER_SEC)
        }
        Err(_) => (0.0, 0.0),
    };
    let source = friendly_source(&session.SourceAppUserModelId()?.to_string());
    let key = format!("{source}|{title}|{artist}");
    let thumbnail = cache.get(&key, || read_thumbnail(&props));
    Ok(MediaInfo { title, artist, album, thumbnail, playing, position, duration, source })
}

/// Citește media la fiecare 500 ms și emite `media-update` doar când e nevoie.
pub fn spawn(app: AppHandle) {
    let store = app.state::<LastMedia>().inner().clone();
    std::thread::spawn(move || {
        let mut last: Option<MediaInfo> = None;
        let mut last_emit = Instant::now();
        let mut cache = ThumbCache::default();
        loop {
            let manager = match Manager::RequestAsync().and_then(|op| op.get()) {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("GSMTC indisponibil: {e}");
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                }
            };
            // managerul e re-creat la ~5 minute, ca să nu rămână blocat după restartul serviciului media
            for _ in 0..600 {
                let cur = pick_session(&manager).and_then(|s| read_session(&s, &mut cache).ok());
                let since = last_emit.elapsed().as_secs_f64();
                if should_emit(&last, &cur, since) {
                    store.set(cur.clone());
                    let _ = app.emit("media-update", cur.clone());
                    last_emit = Instant::now();
                    last = cur;
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
    });
}

#[tauri::command]
pub async fn media_control(action: String) -> Result<(), String> {
    let action = MediaAction::parse(&action).ok_or_else(|| format!("acțiune necunoscută: {action}"))?;
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let m = Manager::RequestAsync().and_then(|op| op.get()).map_err(|e| e.to_string())?;
        let s = pick_session(&m).ok_or("nicio sesiune media")?;
        let op = match action {
            MediaAction::PlayPause => s.TryTogglePlayPauseAsync(),
            MediaAction::Next => s.TrySkipNextAsync(),
            MediaAction::Prev => s.TrySkipPreviousAsync(),
        };
        op.and_then(|o| o.get()).map(|_| ()).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(title: &str, playing: bool, position: f64) -> MediaInfo {
        MediaInfo { title: title.into(), artist: "A".into(), playing, position, duration: 200.0, source: "Spotify".into(), ..Default::default() }
    }

    #[test]
    fn pozitie_extrapolata_doar_cand_ruleaza() {
        let s = 10_000_000i64;
        assert_eq!(live_position(10 * s, 100 * s, 103 * s, true, 0), 13.0);
        assert_eq!(live_position(10 * s, 100 * s, 103 * s, false, 0), 10.0);
        assert_eq!(live_position(10 * s, 100 * s, 500 * s, true, 20 * s), 20.0);
        assert_eq!(live_position(10 * s, 0, 500 * s, true, 0), 10.0);
    }

    #[test]
    fn nume_prietenoase() {
        assert_eq!(friendly_source("Spotify.exe"), "Spotify");
        assert_eq!(friendly_source("SpotifyAB.SpotifyMusic_zpdnekdrzrea0!Spotify"), "Spotify");
        assert_eq!(friendly_source("Chrome"), "Chrome");
        assert_eq!(friendly_source("MSEdge"), "Edge");
        assert_eq!(friendly_source("308046B0AF4A39CB"), "Firefox");
        assert_eq!(friendly_source(r"C:\Apps\foobar2000.exe"), "foobar2000");
    }

    #[test]
    fn emite_doar_la_schimbari() {
        let a = Some(info("X", true, 10.0));
        assert!(!should_emit(&None, &None, 1.0));
        assert!(should_emit(&None, &a, 0.0));
        assert!(should_emit(&a, &None, 0.0));
        assert!(!should_emit(&a, &Some(info("X", true, 10.5)), 0.5)); // avans normal
        assert!(should_emit(&a, &Some(info("X", true, 60.0)), 0.5)); // seek
        assert!(should_emit(&a, &Some(info("X", false, 10.0)), 0.0)); // pauză
        assert!(should_emit(&a, &Some(info("Y", true, 0.0)), 0.5)); // piesă nouă
    }

    #[test]
    fn prefera_sesiunea_care_ruleaza() {
        assert_eq!(choose_session_index(&[false, true, false], Some(0)), Some(1));
        assert_eq!(choose_session_index(&[false, false], Some(1)), Some(1));
        assert_eq!(choose_session_index(&[false, false], None), Some(0));
        assert_eq!(choose_session_index(&[], None), None);
    }

    #[test]
    fn ultima_stare_ramane_disponibila_pentru_interfata() {
        let last = LastMedia::default();
        assert_eq!(last.get(), None);
        last.set(Some(info("X", true, 1.0)));
        assert_eq!(last.get().map(|m| m.title), Some("X".to_string()));
        last.set(None);
        assert_eq!(last.get(), None);
    }

    #[test]
    fn actiuni_media() {
        assert_eq!(MediaAction::parse("play_pause"), Some(MediaAction::PlayPause));
        assert_eq!(MediaAction::parse("next"), Some(MediaAction::Next));
        assert_eq!(MediaAction::parse("prev"), Some(MediaAction::Prev));
        assert_eq!(MediaAction::parse("volum"), None);
    }

    #[test]
    fn coperta_reincercata_pana_apare() {
        let mut c = ThumbCache::with_retry(Duration::ZERO);
        assert_eq!(c.get("k1", || None), None);
        assert_eq!(c.get("k1", || Some("img".into())), Some("img".into()));
        // cât timp piesa e aceeași, nu se mai citește
        assert_eq!(c.get("k1", || panic!("nu trebuia citită")), Some("img".into()));
        // piesă nouă → citire nouă
        assert_eq!(c.get("k2", || Some("img2".into())), Some("img2".into()));
    }
}
