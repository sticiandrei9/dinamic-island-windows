use std::f32::consts::PI;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX,
    WAVEFORMATEXTENSIBLE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::Foundation::E_NOTIMPL;

use crate::settings::SettingsState;

const N: usize = 1024;
/// Marginile celor 5 benzi (Hz), pe scară logaritmică.
const EDGES: [f32; 6] = [60.0, 250.0, 600.0, 1500.0, 4000.0, 8000.0];

/// FFT radix-2 în loc (Cooley–Tukey iterativ); lungimea e o putere a lui 2.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

/// Nivelul celor 5 benzi (0..1) din ultimele 1024 de eșantioane mono: fereastră Hann, FFT,
/// puterea însumată a binurilor din bandă în dB, −80..0 dB → 0..1 (o sinusoidă la scară plină ≈ 0 dB).
/// Suma (nu media pe bin): altfel benzile late de sus, cu energie mică pe bin, ar sta mereu jos.
pub fn bands(samples: &[f32], rate: u32) -> [f32; 5] {
    let tail = &samples[samples.len().saturating_sub(N)..];
    let mut re = [0f32; N];
    let mut im = [0f32; N];
    for (i, s) in tail.iter().enumerate() {
        re[i] = s * (0.5 - 0.5 * (2.0 * PI * i as f32 / (N - 1) as f32).cos());
    }
    fft(&mut re, &mut im);
    let hz = rate as f32 / N as f32;
    let norm = N as f32 / 4.0; // amplitudinea A dă vârful A·N/4 cu fereastra Hann
    let mut out = [0.0; 5];
    for (b, level) in out.iter_mut().enumerate() {
        let mut power = 0.0f32;
        for k in 1..N / 2 {
            let f = k as f32 * hz;
            if f >= EDGES[b] && f < EDGES[b + 1] {
                power += (re[k] * re[k] + im[k] * im[k]) / (norm * norm);
            }
        }
        let db = 10.0 * power.max(1e-12).log10();
        *level = ((db + 80.0) / 80.0).clamp(0.0, 1.0);
    }
    out
}

/// „Gain” automat pe fiecare bandă: vârful ei urcă imediat și coboară lent, ca barele să arate bine
/// și la volum mic; amplificarea e plafonată (10×), altfel zgomotul de fond ar umple barele.
pub struct AutoGain {
    peak: [f32; 5],
}

impl Default for AutoGain {
    fn default() -> Self {
        AutoGain { peak: [1.0; 5] }
    }
}

impl AutoGain {
    pub fn apply(&mut self, b: [f32; 5]) -> [f32; 5] {
        let mut out = [0.0; 5];
        for i in 0..5 {
            self.peak[i] = (self.peak[i] * 0.98).max(b[i]).max(0.1);
            out[i] = (b[i] / self.peak[i]).min(1.0);
        }
        out
    }
}

pub fn should_restart(prev_device: &str, cur_device: &str, failed: bool) -> bool {
    failed || prev_device != cur_device
}

/// Captura loopback pe difuzorul implicit (ce iese în boxe, în formatul partajat).
struct Capture {
    device: String,
    client: IAudioClient,
    capture: IAudioCaptureClient,
    rate: u32,
    channels: usize,
    float: bool,
}

impl Capture {
    fn open(e: &IMMDeviceEnumerator) -> windows::core::Result<Capture> {
        unsafe {
            let dev = e.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let id = dev.GetId()?;
            let device = id.to_string().unwrap_or_default();
            CoTaskMemFree(Some(id.0 as *const _));
            let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
            let fmt = client.GetMixFormat()?;
            let f: WAVEFORMATEX = *fmt;
            // 3 = WAVE_FORMAT_IEEE_FLOAT; 0xFFFE = WAVE_FORMAT_EXTENSIBLE, cu tipul real în SubFormat
            let tag = if f.wFormatTag == 0xFFFE {
                let ext = std::ptr::read_unaligned(fmt as *const WAVEFORMATEXTENSIBLE);
                let sub = { ext.SubFormat }; // copie: câmpul e într-un struct `packed`
                sub.data1 as u16
            } else {
                f.wFormatTag
            };
            let (float, bits) = (tag == 3, f.wBitsPerSample);
            if !(float && bits == 32) && !(tag == 1 && bits == 16) {
                CoTaskMemFree(Some(fmt as *const _));
                return Err(windows::core::Error::new(E_NOTIMPL, format!("format nesuportat: tag {tag}, {bits} biți")));
            }
            let r = client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, 2_000_000, 0, fmt, None);
            CoTaskMemFree(Some(fmt as *const _));
            r?;
            let capture: IAudioCaptureClient = client.GetService()?;
            client.Start()?;
            Ok(Capture {
                device,
                client,
                capture,
                rate: f.nSamplesPerSec,
                channels: f.nChannels.max(1) as usize,
                float,
            })
        }
    }

    /// Adaugă în `ring` pachetele sosite (mono). Întoarce dacă a fost sunet audibil.
    fn read(&self, ring: &mut Vec<f32>) -> windows::core::Result<bool> {
        let mut sound = false;
        unsafe {
            while self.capture.GetNextPacketSize()? > 0 {
                let (mut data, mut frames, mut flags) = (std::ptr::null_mut(), 0u32, 0u32);
                self.capture.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                let silent = flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 || data.is_null();
                for fr in 0..frames as usize {
                    let mut s = 0.0f32;
                    if !silent {
                        for c in 0..self.channels {
                            let i = fr * self.channels + c;
                            s += if self.float {
                                (data as *const f32).add(i).read_unaligned()
                            } else {
                                (data as *const i16).add(i).read_unaligned() as f32 / 32768.0
                            };
                        }
                    }
                    let s = s / self.channels as f32;
                    sound |= s.abs() > 1e-4;
                    ring.push(s);
                }
                self.capture.ReleaseBuffer(frames)?;
            }
        }
        if ring.len() > N {
            ring.drain(..ring.len() - N);
        }
        Ok(sound)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
    }
}

struct ComGuard;
impl Drop for ComGuard {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

/// Emite `viz` (~30/s) doar cât: modulul e pornit, media e `playing` și o insulă e vizibilă.
/// Tăcere > 1 s → un singur `[0;5]`, apoi nimic până revine sunetul.
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        let _com = ComGuard;
        let e: IMMDeviceEnumerator = match unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) } {
            Ok(e) => e,
            Err(err) => return log::warn!("vizualizator: fără Core Audio ({err})"),
        };
        let mut cap: Option<Capture> = None;
        let (mut failed, mut failed_at, mut logged) = (false, None::<Instant>, false);
        let mut check_at = Instant::now() - Duration::from_secs(1);
        let mut ring: Vec<f32> = Vec::with_capacity(N * 4);
        let mut gain = AutoGain::default();
        let (mut last_sound, mut quiet_sent) = (Instant::now(), false);
        // dispozitivul cu format nesuportat: nu-l mai redeschidem până se schimbă cel implicit
        let mut unsupported: Option<String> = None;
        loop {
            std::thread::sleep(Duration::from_millis(if cap.is_some() { 33 } else { 250 }));
            if check_at.elapsed() >= Duration::from_millis(250) {
                check_at = Instant::now();
                let want = app.state::<SettingsState>().get().modules.visualizer
                    && app.state::<crate::media::LastMedia>().get().map_or(false, |m| m.playing)
                    && crate::hittest::any_island_visible();
                if let Some(c) = &cap {
                    let cur = crate::volume::default_endpoint_id(&e).unwrap_or_default();
                    if !want || should_restart(&c.device, &cur, failed) {
                        cap = None;
                    }
                }
                if failed {
                    failed = false;
                    failed_at = Some(Instant::now());
                }
                let cur = || crate::volume::default_endpoint_id(&e).unwrap_or_default();
                if want
                    && cap.is_none()
                    && failed_at.map_or(true, |t| t.elapsed() > Duration::from_secs(2))
                    && unsupported.as_ref().map_or(true, |d| *d != cur())
                {
                    match Capture::open(&e) {
                        Ok(c) => {
                            ring.clear();
                            gain = AutoGain::default();
                            (last_sound, quiet_sent, logged) = (Instant::now(), false, false);
                            cap = Some(c);
                        }
                        Err(err) => {
                            if err.code() == E_NOTIMPL {
                                unsupported = Some(cur());
                            }
                            // o singură dată până la o captură reușită (altfel log la 2 s)
                            if !logged {
                                logged = true;
                                log::warn!("vizualizator: captura nu pornește ({err})");
                            }
                            failed_at = Some(Instant::now());
                        }
                    }
                }
            }
            let Some(c) = &cap else { continue };
            match c.read(&mut ring) {
                Ok(true) => (last_sound, quiet_sent) = (Instant::now(), false),
                Ok(false) => {}
                Err(_) => {
                    failed = true;
                    continue;
                }
            }
            if last_sound.elapsed() > Duration::from_secs(1) {
                if !quiet_sent {
                    quiet_sent = true;
                    let _ = app.emit("viz", [0.0f32; 5]);
                }
                continue;
            }
            let _ = app.emit("viz", gain.apply(bands(&ring, c.rate)));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(freq: f32, rate: u32, n: usize) -> Vec<f32> {
        (0..n).map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / rate as f32).sin()).collect()
    }

    #[test]
    fn benzi_pe_frecventa() {
        let low = bands(&sine(100.0, 48_000, 1024), 48_000);
        let high = bands(&sine(5_000.0, 48_000, 1024), 48_000);
        assert!(low[0] > low[4] && low[0] > 0.5);
        assert!(high[4] > high[0] && high[4] > 0.5);
        assert_eq!(bands(&vec![0.0; 1024], 48_000), [0.0; 5]);
    }

    #[test]
    fn autogain_urca_repede_coboara_lent() {
        let mut g = AutoGain::default();
        let a = g.apply([0.1; 5]);
        for _ in 0..30 {
            g.apply([0.1; 5]);
        }
        let b = g.apply([0.1; 5]);
        assert!(b[0] > a[0] && b[0] <= 1.0);
        let c = g.apply([1.0; 5]);
        assert!(c.iter().all(|v| *v <= 1.0));
    }

    /// Zgomot alb încet (cam cât muzica la volum mic, −50 dBFS): benzile late de sus nu stau moarte.
    #[test]
    fn zgomot_larg_ridica_benzile_de_sus() {
        let mut seed = 1u32;
        let mut noise = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32 * 0.006 - 0.003
        };
        let mut g = AutoGain::default();
        let mut out = [0.0; 5];
        for _ in 0..200 {
            let frame: Vec<f32> = (0..1024).map(|_| noise()).collect();
            out = g.apply(bands(&frame, 48_000));
        }
        assert!(out[2..].iter().all(|v| *v > 0.2), "{out:?}");
    }

    #[test]
    fn refacere_la_schimbarea_dispozitivului() {
        assert!(!should_restart("A", "A", false));
        assert!(should_restart("A", "B", false));
        assert!(should_restart("A", "A", true));
    }

    /// Captură reală ~1 s de pe difuzorul implicit (doar citește; nu schimbă nimic).
    /// `cargo test -- --ignored --nocapture captura_reala`
    #[test]
    #[ignore]
    fn captura_reala() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let e: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).unwrap();
            let c = Capture::open(&e).unwrap();
            println!("dispozitiv {}, {} Hz, {} canale, float {}", c.device, c.rate, c.channels, c.float);
            let mut ring = Vec::new();
            let mut gain = AutoGain::default();
            for i in 0..30 {
                std::thread::sleep(Duration::from_millis(33));
                let sound = c.read(&mut ring).unwrap();
                let raw = bands(&ring, c.rate);
                let peak = ring.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                println!("{i:2} sunet={sound} vârf={peak:.4} benzi={raw:.2?} cu gain={:.2?}", gain.apply(raw));
            }
            drop(c);
            CoUninitialize();
        }
    }
}
