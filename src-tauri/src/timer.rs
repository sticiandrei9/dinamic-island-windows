use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use windows::core::PCWSTR;
use windows::Win32::Media::Audio::{PlaySoundW, SND_FILENAME, SND_NODEFAULT, SND_SYNC};

/// Timpii sunt în milisecunde unix (JS îi compară cu `Date.now()`).
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Countdown {
    pub total_ms: u64,
    /// când rulează: momentul la care se termină; în pauză: None
    pub ends_at: Option<u64>,
    /// în pauză: cât a rămas
    pub remaining_ms: u64,
}

#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct Stopwatch {
    pub base_ms: u64,
    pub started_at: Option<u64>,
    pub laps: Vec<u64>,
    /// câte tururi au fost în total (se păstrează doar ultimele 5), pentru numerotare
    pub lap_total: u32,
}

/// Pomodoro: 25 min lucru, 5 min pauză; după a 4-a rundă, pauză lungă de 15 min.
pub const WORK_MS: u64 = 25 * 60_000;
pub const BREAK_MS: u64 = 5 * 60_000;
pub const LONG_BREAK_MS: u64 = 15 * 60_000;
pub const ROUNDS: u32 = 4;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Pomodoro {
    /// "work", "break" sau "long"
    pub phase: &'static str,
    /// runda de lucru curentă, 1..=4
    pub round: u32,
}

/// Timerul de somn: la `ends_at` muzica se oprește; în ultimele 10 s volumul scade până la 0.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Sleep {
    pub ends_at: u64,
    /// durata aleasă (pentru inelul care se golește, în toate ferestrele)
    pub total_ms: u64,
    /// volumul master de dinainte de fade (reținut la intrarea în ultimele 10 s)
    pub start_volume: Option<f32>,
}

/// Cât durează scăderea volumului, înainte de final.
pub const FADE_MS: u64 = 10_000;
/// Diferența de la care o schimbare a volumului e a utilizatorului, nu a fade-ului.
const USER_CHANGE: f32 = 0.02;

/// Volumul din fade: None înainte de ultimele 10 s, apoi liniar `start → 0`.
pub fn fade_level(now: u64, ends_at: u64, start: f32) -> Option<f32> {
    let left = ends_at.saturating_sub(now);
    (left <= FADE_MS).then(|| start * (left as f32 / FADE_MS as f32))
}

/// Utilizatorul a mișcat volumul între două scrieri ale fade-ului.
pub fn user_changed(expected: f32, actual: f32) -> bool {
    (expected - actual).abs() > USER_CHANGE
}

/// După pauză volumul revine și când nu e nicio sesiune media (nimic de oprit); la altă eroare rămâne 0.
pub fn restore_after_pause(r: &Result<(), String>) -> bool {
    match r {
        Ok(()) => true,
        Err(e) => e == crate::media::NO_SESSION,
    }
}

/// Un singur ceas pentru toată aplicația (toate insulele îl afișează la fel).
#[derive(Serialize, Clone, Debug, PartialEq, Default)]
pub struct Clock {
    pub timer: Option<Countdown>,
    pub stopwatch: Option<Stopwatch>,
    /// timerul s-a terminat și insula pulsează până la click
    pub finished: bool,
    /// timerul face parte dintr-un ciclu Pomodoro
    pub pomodoro: Option<Pomodoro>,
    pub sleep: Option<Sleep>,
}

impl Clock {
    pub fn sleep_start(&mut self, total_ms: u64, now: u64) {
        self.sleep = Some(Sleep { ends_at: now + total_ms, total_ms, start_volume: None });
    }

    pub fn sleep_cancel(&mut self) {
        self.sleep = None;
    }

    pub fn timer_start(&mut self, total_ms: u64, now: u64) {
        self.finished = false;
        self.pomodoro = None;
        self.timer = Some(Countdown { total_ms, ends_at: Some(now + total_ms), remaining_ms: total_ms });
    }

    pub fn timer_remaining(&self, now: u64) -> u64 {
        self.timer.as_ref().map_or(0, |c| c.ends_at.map_or(c.remaining_ms, |e| e.saturating_sub(now)))
    }

    pub fn timer_pause(&mut self, now: u64) {
        let rem = self.timer_remaining(now);
        if let Some(c) = self.timer.as_mut() {
            c.remaining_ms = rem;
            c.ends_at = None;
        }
    }

    pub fn timer_resume(&mut self, now: u64) {
        if let Some(c) = self.timer.as_mut() {
            if c.ends_at.is_none() {
                c.ends_at = Some(now + c.remaining_ms);
            }
        }
    }

    pub fn timer_cancel(&mut self) {
        self.timer = None;
        self.pomodoro = None;
    }

    pub fn pomodoro_start(&mut self, now: u64) {
        self.timer_start(WORK_MS, now);
        self.pomodoro = Some(Pomodoro { phase: "work", round: 1 });
    }

    /// Etapa următoare a ciclului și durata ei.
    fn pomodoro_next(p: &Pomodoro) -> (Pomodoro, u64) {
        match p.phase {
            "work" if p.round >= ROUNDS => (Pomodoro { phase: "long", round: p.round }, LONG_BREAK_MS),
            "work" => (Pomodoro { phase: "break", round: p.round }, BREAK_MS),
            "long" => (Pomodoro { phase: "work", round: 1 }, WORK_MS),
            _ => (Pomodoro { phase: "work", round: p.round + 1 }, WORK_MS),
        }
    }

    pub fn dismiss(&mut self) {
        self.finished = false;
    }

    /// Întoarce true o singură dată, când timerul ajunge la zero. În Pomodoro, etapa următoare
    /// pornește singură (fără pulsare până la click).
    pub fn tick(&mut self, now: u64) -> bool {
        let done = self.timer.as_ref().is_some_and(|c| c.ends_at.is_some()) && self.timer_remaining(now) == 0;
        if done && self.pomodoro.is_some() {
            let (next, ms) = Self::pomodoro_next(self.pomodoro.as_ref().unwrap());
            self.timer_start(ms, now);
            self.pomodoro = Some(next);
        } else if done {
            self.timer = None;
            self.finished = true;
        }
        done
    }

    pub fn stopwatch_start(&mut self, now: u64) {
        let sw = self.stopwatch.get_or_insert_with(Stopwatch::default);
        if sw.started_at.is_none() {
            sw.started_at = Some(now);
        }
    }

    pub fn stopwatch_elapsed(&self, now: u64) -> u64 {
        self.stopwatch.as_ref().map_or(0, |s| s.base_ms + s.started_at.map_or(0, |t| now.saturating_sub(t)))
    }

    pub fn stopwatch_pause(&mut self, now: u64) {
        let e = self.stopwatch_elapsed(now);
        if let Some(s) = self.stopwatch.as_mut() {
            s.base_ms = e;
            s.started_at = None;
        }
    }

    pub fn stopwatch_lap(&mut self, now: u64) {
        let e = self.stopwatch_elapsed(now);
        if let Some(s) = self.stopwatch.as_mut() {
            s.laps.push(e);
            s.lap_total += 1;
            if s.laps.len() > 5 {
                s.laps.remove(0);
            }
        }
    }

    pub fn stopwatch_reset(&mut self) {
        self.stopwatch = None;
    }
}

#[derive(Default)]
pub struct ClockState(pub Mutex<Clock>);

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn publish(app: &AppHandle) {
    let snapshot = app.state::<ClockState>().0.lock().unwrap().clone();
    let _ = app.emit("timer-state", snapshot);
}

fn with_clock(app: &AppHandle, f: impl FnOnce(&mut Clock, u64)) {
    {
        let state = app.state::<ClockState>();
        let mut c = state.0.lock().unwrap();
        f(&mut c, now_ms());
    }
    publish(app);
}

/// Sunetul de alarmă din folderul Windows (care poate fi pe altă partiție).
pub fn alarm_path(system_root: Option<&str>) -> String {
    format!("{}\\Media\\Alarm01.wav", system_root.unwrap_or("C:\\Windows"))
}

/// Sunetul de alarmă Windows, de `times` ori, pe un thread separat.
fn play_alarm(times: usize) {
    std::thread::spawn(move || {
        let root = std::env::var("SystemRoot").ok();
        let path: Vec<u16> = alarm_path(root.as_deref()).encode_utf16().chain(Some(0)).collect();
        for _ in 0..times {
            unsafe {
                let _ = PlaySoundW(PCWSTR(path.as_ptr()), None, SND_FILENAME | SND_SYNC | SND_NODEFAULT);
            }
        }
    });
}

/// Fade-ul în curs (doar în thread-ul timerului): somnul căruia îi aparține, volumul de dinainte și ultima valoare scrisă.
struct Fade {
    ends_at: u64,
    start: Option<f32>,
    last: Option<f32>,
}

/// Anulează somnul din ceas (utilizatorul a schimbat volumul sau modulul a fost oprit).
fn drop_sleep(app: &AppHandle) {
    with_clock(app, |c, _| c.sleep_cancel());
}

/// Pauza și volumul la final de somn, pe un thread separat (pauza poate dura câteva secunde).
fn finish_sleep(app: AppHandle, start: Option<f32>) {
    std::thread::spawn(move || {
        let r = crate::media::pause_displayed(&app.state::<crate::media::LastMedia>());
        if restore_after_pause(&r) {
            if let Some(v) = start {
                crate::volume::suppress_transient(Instant::now() + Duration::from_secs(1));
                let _ = crate::volume::set_master_volume(v);
            }
            let _ = app.emit("sleep-done", ());
        } else {
            log::warn!("timer de somn: pauza a eșuat: {}", r.unwrap_err());
            let _ = app.emit(
                "alert",
                serde_json::json!({ "icon": "bell", "title": crate::i18n::t("sleep.pauseFailed"), "detail": "", "color": "var(--red)", "level": null }),
            );
        }
    });
}

/// Un pas al somnului (la 250 ms): în ultimele 10 s scade volumul; la final oprește muzica.
fn sleep_tick(app: &AppHandle, fade: &mut Option<Fade>, now: u64) {
    let enabled = app.state::<crate::settings::SettingsState>().get().modules.sleep;
    let sleep = app.state::<ClockState>().0.lock().unwrap().sleep.clone();
    if sleep.is_some() && !enabled {
        drop_sleep(app); // modulul oprit din Setări: somnul dispare
    }
    let current = sleep.filter(|_| enabled);
    // somnul s-a anulat sau a fost înlocuit în timpul fade-ului: volumul lăsat jos revine
    if fade.as_ref().is_some_and(|f| current.as_ref().map(|s| s.ends_at) != Some(f.ends_at)) {
        if let Some(v) = fade.take().and_then(|f| f.start) {
            crate::volume::suppress_transient(Instant::now() + Duration::from_secs(1));
            let _ = crate::volume::set_master_volume(v);
        }
    }
    let Some(s) = current else { return };
    if fade_level(now, s.ends_at, 1.0).is_none() {
        return; // încă înainte de ultimele 10 s
    }
    if fade.is_none() {
        let start = crate::volume::master_volume();
        *fade = Some(Fade { ends_at: s.ends_at, start, last: None });
        with_clock(app, |c, _| {
            if let Some(sl) = c.sleep.as_mut() {
                sl.start_volume = start;
            }
        });
    }
    let f = fade.as_mut().unwrap();
    if let (Some(last), Some(actual)) = (f.last, crate::volume::master_volume()) {
        if user_changed(last, actual) {
            *fade = None; // a ales omul volumul: fade-ul și somnul se opresc, fără pauză
            drop_sleep(app);
            return;
        }
    }
    if let Some(level) = f.start.and_then(|st| fade_level(now, s.ends_at, st)) {
        crate::volume::suppress_transient(Instant::now() + Duration::from_secs(1));
        if crate::volume::set_master_volume(level).is_ok() {
            f.last = Some(level);
        }
    }
    if now >= s.ends_at {
        let start = f.start;
        *fade = None;
        drop_sleep(app);
        finish_sleep(app.clone(), start);
    }
}

/// Verifică de 4 ori pe secundă dacă timerul s-a terminat; după 30 s, pulsarea se oprește singură.
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut finished_at: Option<u64> = None;
        let mut fade: Option<Fade> = None;
        loop {
            std::thread::sleep(Duration::from_millis(250));
            let now = now_ms();
            sleep_tick(&app, &mut fade, now);
            let (done, clear, pomodoro) = {
                let state = app.state::<ClockState>();
                let mut c = state.0.lock().unwrap();
                let done = c.tick(now);
                let clear = c.finished && finished_at.is_some_and(|t| now.saturating_sub(t) > 30_000);
                if clear {
                    c.dismiss();
                }
                (done, clear, c.pomodoro.is_some())
            };
            if done {
                finished_at = Some(now);
                // cu modulul Timer oprit nu se vede nimic, deci nici nu sună
                if app.state::<crate::settings::SettingsState>().get().modules.timer {
                    play_alarm(if pomodoro { 1 } else { 3 }); // Pomodoro: un singur semnal la fiecare etapă
                }
            }
            if clear {
                finished_at = None;
            }
            if done || clear {
                publish(&app);
            }
        }
    });
}

#[tauri::command]
pub fn get_timer_state(app: AppHandle) -> Clock {
    app.state::<ClockState>().0.lock().unwrap().clone()
}

#[tauri::command]
pub fn timer_command(app: AppHandle, action: String, seconds: Option<u64>) -> Result<(), String> {
    match action.as_str() {
        "pomodoro" => with_clock(&app, |c, now| c.pomodoro_start(now)),
        "start" => with_clock(&app, |c, now| c.timer_start(seconds.unwrap_or(300).clamp(1, 24 * 3600) * 1000, now)),
        "pause" => with_clock(&app, |c, now| c.timer_pause(now)),
        "resume" => with_clock(&app, |c, now| c.timer_resume(now)),
        "cancel" => with_clock(&app, |c, _| c.timer_cancel()),
        "dismiss" => with_clock(&app, |c, _| c.dismiss()),
        "sleep_start" => with_clock(&app, |c, now| c.sleep_start(seconds.unwrap_or(1800).clamp(1, 3 * 3600) * 1000, now)),
        "sleep_cancel" => with_clock(&app, |c, _| c.sleep_cancel()),
        "sw_start" => with_clock(&app, |c, now| c.stopwatch_start(now)),
        "sw_pause" => with_clock(&app, |c, now| c.stopwatch_pause(now)),
        "sw_lap" => with_clock(&app, |c, now| c.stopwatch_lap(now)),
        "sw_reset" => with_clock(&app, |c, _| c.stopwatch_reset()),
        other => return Err(format!("acțiune necunoscută: {other}")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timer_porneste_pauza_reia() {
        let mut s = Clock::default();
        s.timer_start(60_000, 1_000);
        assert_eq!(s.timer_remaining(31_000), 30_000);
        s.timer_pause(31_000);
        assert_eq!(s.timer_remaining(99_000), 30_000); // în pauză nu scade
        s.timer_resume(100_000);
        assert_eq!(s.timer_remaining(110_000), 20_000);
        assert_eq!(s.timer_remaining(500_000), 0); // nu trece sub zero
    }

    #[test]
    fn timer_se_termina_o_singura_data() {
        let mut s = Clock::default();
        s.timer_start(10_000, 0);
        assert!(!s.tick(5_000));
        assert!(s.tick(10_000)); // a trecut la terminat
        assert!(s.finished && s.timer.is_none());
        assert!(!s.tick(11_000)); // nu se mai anunță
        s.dismiss();
        assert!(!s.finished);
    }

    #[test]
    fn pomodoro_cicluri() {
        let mut s = Clock::default();
        s.pomodoro_start(0);
        let mut t = 0;
        let mut phases = Vec::new();
        for _ in 0..9 {
            t += s.timer.as_ref().unwrap().total_ms;
            assert!(s.tick(t)); // fiecare etapă sună
            let p = s.pomodoro.as_ref().unwrap();
            phases.push((p.phase, p.round, s.timer.as_ref().unwrap().total_ms / 60_000));
            assert!(!s.finished); // fără pulsare: etapa următoare a pornit deja
        }
        assert_eq!(phases, vec![
            ("break", 1, 5), ("work", 2, 25), ("break", 2, 5), ("work", 3, 25), ("break", 3, 5),
            ("work", 4, 25), ("long", 4, 15), ("work", 1, 25), ("break", 1, 5),
        ]);
        s.timer_cancel();
        assert!(s.pomodoro.is_none() && s.timer.is_none());
        s.pomodoro_start(0);
        s.timer_start(60_000, 0); // un timer obișnuit iese din ciclu
        assert!(s.pomodoro.is_none());
    }

    #[test]
    fn timer_anulat() {
        let mut s = Clock::default();
        s.timer_start(10_000, 0);
        s.timer_cancel();
        assert!(s.timer.is_none() && !s.tick(20_000));
    }

    #[test]
    fn cronometru_cu_tururi() {
        let mut s = Clock::default();
        s.stopwatch_start(1_000);
        assert_eq!(s.stopwatch_elapsed(4_000), 3_000);
        s.stopwatch_lap(4_000);
        s.stopwatch_pause(6_000);
        assert_eq!(s.stopwatch_elapsed(60_000), 5_000);
        s.stopwatch_start(70_000);
        assert_eq!(s.stopwatch_elapsed(71_000), 6_000);
        s.stopwatch_lap(71_000);
        assert_eq!(s.stopwatch.as_ref().unwrap().laps, vec![3_000, 6_000]);
        s.stopwatch_reset();
        assert!(s.stopwatch.is_none());
    }

    #[test]
    fn doar_ultimele_5_tururi() {
        let mut s = Clock::default();
        s.stopwatch_start(0);
        for t in 1..=7 {
            s.stopwatch_lap(t * 1_000);
        }
        assert_eq!(s.stopwatch.as_ref().unwrap().laps, vec![3_000, 4_000, 5_000, 6_000, 7_000]);
        // numerotarea continuă: cele 5 afișate sunt turele 3–7
        assert_eq!(s.stopwatch.as_ref().unwrap().lap_total, 7);
    }

    #[test]
    fn fade_in_ultimele_10s() {
        assert_eq!(fade_level(0, 20_000, 0.8), None);
        assert_eq!(fade_level(10_000, 20_000, 0.8), Some(0.8));
        assert!((fade_level(15_000, 20_000, 0.8).unwrap() - 0.4).abs() < 1e-6);
        assert_eq!(fade_level(25_000, 20_000, 0.8), Some(0.0));
    }

    #[test]
    fn fade_oprit_de_utilizator() {
        assert!(!user_changed(0.40, 0.41));
        assert!(user_changed(0.40, 0.70));
    }

    #[test]
    fn somn_porneste_si_se_anuleaza() {
        let mut c = Clock::default();
        c.sleep_start(1_800_000, 0);
        assert_eq!(c.sleep.as_ref().unwrap().ends_at, 1_800_000);
        c.sleep_cancel();
        assert!(c.sleep.is_none());
    }

    #[test]
    fn fara_media_volum_restaurat() {
        assert!(restore_after_pause(&Ok(())));
        assert!(restore_after_pause(&Err("nicio sesiune media".into())));
        assert!(!restore_after_pause(&Err("timeout".into())));
    }

    #[test]
    fn sunetul_alarmei_din_folderul_windows() {
        assert_eq!(alarm_path(Some("D:\\Win")), "D:\\Win\\Media\\Alarm01.wav");
        assert_eq!(alarm_path(None), "C:\\Windows\\Media\\Alarm01.wav");
    }
}
