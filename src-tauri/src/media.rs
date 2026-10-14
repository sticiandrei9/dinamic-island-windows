use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
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
use windows::core::RuntimeType;
use windows::Foundation::{AsyncStatus, IAsyncOperation};
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
    /// identificatorul sesiunii Windows; nu ajunge în interfață
    #[serde(skip)]
    pub aumid: String,
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

/// Alege sesiunea de afișat, „lipicios”: o sursă care tocmai a pornit are prioritate,
/// apoi rămânem pe sursa deja afișată (și când e pe pauză, ca ⏯ s-o poată relua),
/// apoi orice sursă care rulează, apoi sesiunea curentă a Windows, apoi prima.
pub fn choose_session_index(
    ids: &[String],
    playing: &[bool],
    prev_playing: &HashSet<String>,
    displayed: Option<&str>,
    current: Option<usize>,
) -> Option<usize> {
    let started = (0..ids.len()).find(|&i| playing[i] && !prev_playing.contains(&ids[i]) && Some(ids[i].as_str()) != displayed);
    let shown = displayed.and_then(|d| ids.iter().position(|id| id == d));
    started
        .or(shown)
        .or_else(|| playing.iter().position(|p| *p))
        .or(current)
        .or(if ids.is_empty() { None } else { Some(0) })
}

/// `Try*Async` întoarce `false` când aplicația refuză comanda; tratăm asta ca eroare.
pub fn control_outcome(r: windows::core::Result<bool>) -> Result<(), String> {
    match r {
        Ok(true) => Ok(()),
        Ok(false) => Err("aplicația a refuzat comanda".into()),
        Err(e) => Err(e.to_string()),
    }
}

const WINRT_TIMEOUT: Duration = Duration::from_secs(2);

/// Așteaptă până când `poll` întoarce un rezultat, cel mult `timeout`
/// (o aplicație-sursă blocată nu mai poate îngheța thread-ul media).
pub fn wait_until<T>(mut poll: impl FnMut() -> Option<windows::core::Result<T>>, timeout: Duration) -> windows::core::Result<T> {
    let start = Instant::now();
    loop {
        if let Some(r) = poll() {
            return r;
        }
        if start.elapsed() >= timeout {
            return Err(E_FAIL.into());
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn await_op<T: RuntimeType + 'static>(op: IAsyncOperation<T>) -> windows::core::Result<T> {
    let r = wait_until(
        || match op.Status() {
            Ok(AsyncStatus::Started) => None,
            Ok(_) => Some(op.GetResults()),
            Err(e) => Some(Err(e)),
        },
        WINRT_TIMEOUT,
    );
    if r.is_err() {
        let _ = op.Cancel();
    }
    r
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
/// înainte ca pagina să asculte se pierd. După o comandă, starea se retrimite,
/// ca afișarea optimistă din JS să fie corectată dacă aplicația a ignorat comanda.
#[derive(Default, Clone)]
pub struct LastMedia {
    info: Arc<Mutex<Option<MediaInfo>>>,
    resync: Arc<AtomicBool>,
}

impl LastMedia {
    pub fn get(&self) -> Option<MediaInfo> {
        self.info.lock().unwrap().clone()
    }

    pub fn set(&self, info: Option<MediaInfo>) {
        *self.info.lock().unwrap() = info;
    }

    pub fn request_resync(&self) {
        self.resync.store(true, Ordering::SeqCst);
    }

    pub fn take_resync(&self) -> bool {
        self.resync.swap(false, Ordering::SeqCst)
    }
}

#[tauri::command]
pub fn media_current(state: tauri::State<LastMedia>) -> Option<MediaInfo> {
    state.get()
}

fn session_id(s: &Session) -> String {
    s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default()
}

/// Sesiunea de afișat + mulțimea surselor care rulează acum (pentru următoarea alegere).
fn pick_session(m: &Manager, prev_playing: &HashSet<String>, displayed: Option<&str>) -> (Option<Session>, HashSet<String>) {
    let list: Vec<Session> = m.GetSessions().map(|v| v.into_iter().collect()).unwrap_or_default();
    let ids: Vec<String> = list.iter().map(session_id).collect();
    let playing: Vec<bool> = list
        .iter()
        .map(|s| s.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).map(|st| st == Status::Playing).unwrap_or(false))
        .collect();
    let current_id = m.GetCurrentSession().map(|s| session_id(&s)).ok();
    let current = current_id.and_then(|id| ids.iter().position(|x| *x == id));
    let now_playing = ids.iter().zip(&playing).filter(|(_, p)| **p).map(|(id, _)| id.clone()).collect();
    let chosen = choose_session_index(&ids, &playing, prev_playing, displayed, current).and_then(|i| list.get(i).cloned());
    (chosen, now_playing)
}

fn read_thumbnail(props: &Props) -> Option<String> {
    let read = || -> windows::core::Result<String> {
        let stream = await_op(props.Thumbnail()?.OpenReadAsync()?)?;
        let size = stream.Size()? as u32;
        if size == 0 || size > 8 * 1024 * 1024 {
            return Err(E_FAIL.into());
        }
        let reader = DataReader::CreateDataReader(&stream)?;
        let load = reader.LoadAsync(size)?;
        let loaded = wait_until(
            || match load.Status() {
                Ok(AsyncStatus::Started) => None,
                Ok(_) => Some(load.GetResults()),
                Err(e) => Some(Err(e)),
            },
            WINRT_TIMEOUT,
        );
        if loaded.is_err() {
            let _ = load.Cancel();
        }
        loaded?;
        let mut buf = vec![0u8; size as usize];
        reader.ReadBytes(&mut buf)?;
        let ct = stream.ContentType().map(|s| s.to_string()).unwrap_or_default();
        let ct = if ct.starts_with("image/") { ct } else { "image/png".to_string() };
        Ok(format!("data:{ct};base64,{}", STANDARD.encode(&buf)))
    };
    read().ok()
}

fn read_session(session: &Session, cache: &mut ThumbCache) -> windows::core::Result<MediaInfo> {
    let props = await_op(session.TryGetMediaPropertiesAsync()?)?;
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
    let aumid = session_id(session);
    let source = friendly_source(&aumid);
    let key = format!("{source}|{title}|{artist}");
    let thumbnail = cache.get(&key, || read_thumbnail(&props));
    Ok(MediaInfo { title, artist, album, thumbnail, playing, position, duration, source, aumid })
}

fn request_manager() -> windows::core::Result<Manager> {
    await_op(Manager::RequestAsync()?)
}

/// Citește media la fiecare 500 ms și emite `media-update` doar când e nevoie.
pub fn spawn(app: AppHandle) {
    let store = app.state::<LastMedia>().inner().clone();
    std::thread::spawn(move || {
        let mut last: Option<MediaInfo> = None;
        let mut last_emit = Instant::now();
        let mut cache = ThumbCache::default();
        let mut prev_playing: HashSet<String> = HashSet::new();
        loop {
            let manager = match request_manager() {
                Ok(m) => m,
                Err(e) => {
                    eprintln!("GSMTC indisponibil: {e}");
                    std::thread::sleep(Duration::from_secs(5));
                    continue;
                }
            };
            // managerul e re-creat la ~5 minute, ca să nu rămână blocat după restartul serviciului media
            for _ in 0..600 {
                let displayed = last.as_ref().map(|m| m.aumid.clone());
                let (session, now_playing) = pick_session(&manager, &prev_playing, displayed.as_deref());
                prev_playing = now_playing;
                let cur = session.and_then(|s| read_session(&s, &mut cache).ok());
                let since = last_emit.elapsed().as_secs_f64();
                if store.take_resync() || should_emit(&last, &cur, since) {
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

/// Comanda merge la sursa afișată în insulă, nu la una re-aleasă independent.
#[tauri::command]
pub async fn media_control(action: String, state: tauri::State<'_, LastMedia>) -> Result<(), String> {
    let action = MediaAction::parse(&action).ok_or_else(|| format!("acțiune necunoscută: {action}"))?;
    let store = state.inner().clone();
    let displayed = store.get().map(|m| m.aumid);
    let result = tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let m = request_manager().map_err(|e| e.to_string())?;
        let s = pick_session(&m, &HashSet::new(), displayed.as_deref()).0.ok_or("nicio sesiune media")?;
        let op = match action {
            MediaAction::PlayPause => s.TryTogglePlayPauseAsync(),
            MediaAction::Next => s.TrySkipNextAsync(),
            MediaAction::Prev => s.TrySkipPreviousAsync(),
        };
        control_outcome(op.and_then(await_op))
    })
    .await
    .map_err(|e| e.to_string())?;
    store.request_resync();
    result
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

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn set(v: &[&str]) -> HashSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn prefera_sesiunea_care_ruleaza_cand_nu_e_nimic_afisat() {
        let s = ids(&["spotify", "chrome"]);
        assert_eq!(choose_session_index(&s, &[false, true], &set(&[]), None, Some(0)), Some(1));
        assert_eq!(choose_session_index(&s, &[false, false], &set(&[]), None, Some(1)), Some(1));
        assert_eq!(choose_session_index(&s, &[false, false], &set(&[]), None, None), Some(0));
        assert_eq!(choose_session_index(&[], &[], &set(&[]), None, None), None);
    }

    #[test]
    fn ramane_pe_sursa_afisata_dupa_pauza() {
        let s = ids(&["spotify", "chrome"]);
        // scenariul A: YouTube pus pe pauză, Windows consideră Spotify „current”
        assert_eq!(choose_session_index(&s, &[false, false], &set(&["chrome"]), Some("chrome"), Some(0)), Some(1));
        // scenariul B: ambele rulau, pui pe pauză sursa afișată
        assert_eq!(choose_session_index(&s, &[true, false], &set(&["spotify", "chrome"]), Some("chrome"), Some(0)), Some(1));
    }

    #[test]
    fn trece_pe_sursa_care_tocmai_a_pornit() {
        let s = ids(&["spotify", "chrome"]);
        assert_eq!(choose_session_index(&s, &[true, false], &set(&[]), Some("chrome"), Some(1)), Some(0));
    }

    #[test]
    fn sursa_afisata_inchisa_trece_pe_cea_care_ruleaza() {
        let s = ids(&["spotify"]);
        assert_eq!(choose_session_index(&s, &[true], &set(&["spotify"]), Some("chrome"), None), Some(0));
    }

    #[test]
    fn comanda_refuzata_de_aplicatie_e_eroare() {
        assert_eq!(control_outcome(Ok(true)), Ok(()));
        assert!(control_outcome(Ok(false)).is_err());
        assert!(control_outcome(Err(E_FAIL.into())).is_err());
    }

    #[test]
    fn dupa_o_comanda_starea_se_retrimite() {
        let last = LastMedia::default();
        assert!(!last.take_resync());
        last.request_resync();
        assert!(last.take_resync());
        assert!(!last.take_resync());
    }

    #[test]
    fn asteptarea_winrt_are_timeout() {
        let r: windows::core::Result<u32> = wait_until(|| None, Duration::from_millis(50));
        assert!(r.is_err());
        let mut n = 0;
        let r = wait_until(|| { n += 1; if n > 2 { Some(Ok(7u32)) } else { None } }, Duration::from_secs(1));
        assert_eq!(r.unwrap(), 7);
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
