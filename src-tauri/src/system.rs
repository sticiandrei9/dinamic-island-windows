//! Fila Sistem: procesor, RAM, video, temperaturi, viteza rețelei și ping. Senzorii se citesc doar cât
//! cel puțin o fereastră-insulă are fila deschisă (`system_watch`); altfel thread-ul doarme fără să atingă PDH/NVML/ICMP.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use windows::core::{s, w, BSTR, PCWSTR, VARIANT};
use windows::Win32::Foundation::{FreeLibrary, FILETIME, HMODULE};
use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};
use windows::Win32::NetworkManagement::IpHelper::{
    FreeMibTable, GetIfTable2, IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho, ICMP_ECHO_REPLY, IF_TYPE_ETHERNET_CSMACD, IF_TYPE_IEEE80211, MIB_IF_TABLE2,
};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CoSetProxyBlanket, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32};
use windows::Win32::System::Rpc::{RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE};
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_MORE_DATA,
};
use windows::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows::Win32::System::Threading::GetSystemTimes;
use windows::Win32::System::Wmi::{IWbemLocator, WbemLocator, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_INFINITE};

use crate::settings::SettingsState;

// ---------- logica pură ----------

/// Procentul de procesor între două citiri `GetSystemTimes` (idle, kernel, user); kernel include idle.
pub fn cpu_percent(prev: (u64, u64, u64), now: (u64, u64, u64)) -> f64 {
    let idle = now.0.saturating_sub(prev.0);
    let total = now.1.saturating_sub(prev.1) + now.2.saturating_sub(prev.2);
    if total == 0 {
        return 0.0;
    }
    (100.0 * (1.0 - idle as f64 / total as f64)).clamp(0.0, 100.0)
}

/// Viteza totală (descărcare, încărcare) în octeți/s, din (index interfață, octeți in, octeți out).
/// Interfață nouă sau contor scăzut → contribuie 0.
pub fn rates(prev: &[(u64, u64, u64)], now: &[(u64, u64, u64)], dt_s: f64) -> (f64, f64) {
    if dt_s <= 0.0 {
        return (0.0, 0.0);
    }
    let (mut down, mut up) = (0u64, 0u64);
    for &(index, rx, tx) in now {
        if let Some(&(_, prev_rx, prev_tx)) = prev.iter().find(|p| p.0 == index) {
            down += rx.saturating_sub(prev_rx);
            up += tx.saturating_sub(prev_tx);
        }
    }
    (down as f64 / dt_s, up as f64 / dt_s)
}

/// LUID-ul din numele unei instanțe PDH (`pid_X_luid_0x..._0x..._phys_N_eng_M_engtype_3D`).
fn luid_of(name: &str) -> Option<&str> {
    name.split_once("luid_")?.1.split_once("_phys")?.0.into()
}

/// Utilizarea 3D a celui mai încărcat adaptor: suma pe LUID, maximul, plafonat la 100.
pub fn gpu_max(samples: &[(String, f64)]) -> Option<f64> {
    let mut per_adapter: HashMap<&str, f64> = HashMap::new();
    for (name, value) in samples {
        if let Some(luid) = luid_of(name) {
            *per_adapter.entry(luid).or_default() += value;
        }
    }
    per_adapter.into_values().reduce(f64::max).map(|v| v.min(100.0))
}

/// Placa cu acest LUID lucrează deja (vreo instanță cu utilizare > 0)?
pub fn dgpu_busy(samples: &[(String, f64)], luid: &str) -> bool {
    samples.iter().any(|(name, value)| *value > 0.0 && luid_of(name).is_some_and(|l| l.eq_ignore_ascii_case(luid)))
}

// ---------- citiri Windows ----------

fn system_times() -> Option<(u64, u64, u64)> {
    let (mut idle, mut kernel, mut user) = (FILETIME::default(), FILETIME::default(), FILETIME::default());
    unsafe { GetSystemTimes(Some(&mut idle), Some(&mut kernel), Some(&mut user)).ok()? };
    let ticks = |f: FILETIME| (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime);
    Some((ticks(idle), ticks(kernel), ticks(user)))
}

/// (RAM folosită, RAM totală) în octeți.
fn memory() -> (u64, u64) {
    let mut m = MEMORYSTATUSEX { dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32, ..Default::default() };
    if unsafe { GlobalMemoryStatusEx(&mut m) }.is_err() {
        return (0, 0);
    }
    (m.ullTotalPhys.saturating_sub(m.ullAvailPhys), m.ullTotalPhys)
}

/// (index, octeți primiți, octeți trimiși) pentru interfețele Ethernet/Wi-Fi active, fără cele de filtrare (ar număra de două ori).
fn interfaces() -> Vec<(u64, u64, u64)> {
    let mut out = Vec::new();
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if GetIfTable2(&mut table).0 != 0 || table.is_null() {
            return out;
        }
        let rows = std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize);
        for r in rows {
            let physical = r.Type == IF_TYPE_ETHERNET_CSMACD || r.Type == IF_TYPE_IEEE80211;
            let filter = r.InterfaceAndOperStatusFlags._bitfield & 0b10 != 0; // FilterInterface
            if physical && r.OperStatus == IfOperStatusUp && !filter {
                out.push((u64::from(r.InterfaceIndex), r.InOctets, r.OutOctets));
            }
        }
        FreeMibTable(table as *const c_void);
    }
    out
}

/// Utilizarea motoarelor 3D din contoarele de performanță. Calea engleză: UI-ul Windows poate fi în rusă.
struct Gpu {
    query: isize,
    counter: isize,
}

impl Gpu {
    fn open() -> Option<Self> {
        unsafe {
            let (mut query, mut counter) = (0isize, 0isize);
            if PdhOpenQueryW(PCWSTR::null(), 0, &mut query) != 0 {
                return None;
            }
            if PdhAddEnglishCounterW(query, w!(r"\GPU Engine(*engtype_3D)\Utilization Percentage"), 0, &mut counter) != 0 {
                PdhCloseQuery(query);
                return None;
            }
            PdhCollectQueryData(query); // prima citire e doar baza; valorile apar la a doua
            Some(Self { query, counter })
        }
    }

    /// (nume instanță, utilizare %) pentru toate instanțele.
    fn read(&self) -> Vec<(String, f64)> {
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return Vec::new();
            }
            let (mut size, mut count) = (0u32, 0u32);
            if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, None) != PDH_MORE_DATA {
                return Vec::new();
            }
            let mut buf = vec![0u64; size as usize / 8 + 1]; // u64: aliniere pentru structurile PDH
            let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
            if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(items)) != 0 {
                return Vec::new();
            }
            std::slice::from_raw_parts(items, count as usize)
                .iter()
                .filter(|i| i.FmtValue.CStatus <= 1) // valid / date noi
                .map(|i| (i.szName.to_string().unwrap_or_default(), i.FmtValue.Anonymous.doubleValue))
                .collect()
        }
    }
}

impl Drop for Gpu {
    fn drop(&mut self) {
        unsafe { PdhCloseQuery(self.query) };
    }
}

/// LUID-ul adaptorului NVIDIA, în formatul din numele instanțelor PDH (`0x{High:08X}_0x{Low:08X}`).
fn nvidia_luid() -> Option<String> {
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        for i in 0.. {
            let Ok(adapter) = factory.EnumAdapters1(i) else { break };
            let Ok(desc) = adapter.GetDesc1() else { continue };
            if desc.VendorId == 0x10DE {
                return Some(format!("0x{:08X}_0x{:08X}", desc.AdapterLuid.HighPart as u32, desc.AdapterLuid.LowPart));
            }
        }
    }
    None
}

/// Temperatura plăcii NVIDIA prin NVML, încărcat dinamic (nu e linkat; lipsa driverului nu strică pornirea).
struct Nvml {
    lib: HMODULE,
    device: *mut c_void,
    temperature: unsafe extern "C" fn(*mut c_void, u32, *mut u32) -> i32,
    shutdown: unsafe extern "C" fn() -> i32,
}

impl Nvml {
    fn open() -> Option<Self> {
        unsafe {
            let lib = LoadLibraryExW(w!("nvml.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32).ok()?; // doar System32, nu folderul curent
            let symbols = (|| {
                let init: unsafe extern "C" fn() -> i32 = std::mem::transmute(GetProcAddress(lib, s!("nvmlInit_v2"))?);
                let handle: unsafe extern "C" fn(u32, *mut *mut c_void) -> i32 = std::mem::transmute(GetProcAddress(lib, s!("nvmlDeviceGetHandleByIndex_v2"))?);
                let temperature = std::mem::transmute(GetProcAddress(lib, s!("nvmlDeviceGetTemperature"))?);
                let shutdown = std::mem::transmute(GetProcAddress(lib, s!("nvmlShutdown"))?);
                Some((init, handle, temperature, shutdown))
            })();
            let Some((init, handle, temperature, shutdown)) = symbols else {
                let _ = FreeLibrary(lib);
                return None;
            };
            if init() != 0 {
                let _ = FreeLibrary(lib);
                return None;
            }
            let mut nvml = Self { lib, device: std::ptr::null_mut(), temperature, shutdown };
            if handle(0, &mut nvml.device) != 0 {
                return None; // Drop închide NVML
            }
            Some(nvml)
        }
    }

    fn temperature(&self) -> Option<f64> {
        let mut celsius = 0u32;
        (unsafe { (self.temperature)(self.device, 0, &mut celsius) } == 0).then_some(f64::from(celsius)) // 0 = NVML_TEMPERATURE_GPU
    }
}

impl Drop for Nvml {
    fn drop(&mut self) {
        unsafe {
            (self.shutdown)();
            let _ = FreeLibrary(self.lib);
        }
    }
}

/// Temperatura procesorului din WMI, încercată până prima dată când dă greș: de obicei „Acces refuzat” fără drepturi
/// de administrator, caz în care rămâne `None` pe toată rularea, fără log repetat.
static WMI_DEAD: AtomicBool = AtomicBool::new(false);

fn cpu_temp() -> Option<f64> {
    if WMI_DEAD.load(Ordering::Relaxed) {
        return None;
    }
    let value = unsafe {
        let com = CoInitializeEx(None, COINIT_MULTITHREADED).is_ok();
        let value = wmi_temp();
        if com {
            CoUninitialize();
        }
        value
    };
    if value.is_none() {
        WMI_DEAD.store(true, Ordering::Relaxed);
        log::info!("temperatura procesorului nu e disponibilă (WMI)");
    }
    value
}

unsafe fn wmi_temp() -> Option<f64> {
    let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER).ok()?;
    let services = locator.ConnectServer(&BSTR::from("ROOT\\WMI"), &BSTR::new(), &BSTR::new(), &BSTR::new(), 0, &BSTR::new(), None).ok()?;
    CoSetProxyBlanket(&services, RPC_C_AUTHN_WINNT, RPC_C_AUTHZ_NONE, None, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE, None, EOAC_NONE).ok()?;
    let rows = services
        .ExecQuery(&BSTR::from("WQL"), &BSTR::from("SELECT CurrentTemperature FROM MSAcpi_ThermalZoneTemperature"), WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY, None)
        .ok()?;
    let mut hottest: Option<f64> = None;
    loop {
        let mut row = [None];
        let mut returned = 0u32;
        if rows.Next(WBEM_INFINITE, &mut row, &mut returned).is_err() || returned == 0 {
            break;
        }
        let Some(row) = row[0].take() else { break };
        let mut value = VARIANT::default();
        if row.Get(w!("CurrentTemperature"), 0, &mut value, None, None).is_ok() {
            // zecimi de kelvin
            let celsius = f64::from(value.as_raw().Anonymous.Anonymous.Anonymous.lVal) / 10.0 - 273.15;
            if (0.0..150.0).contains(&celsius) {
                hottest = Some(hottest.map_or(celsius, |h| h.max(celsius)));
            }
        }
    }
    hottest
}

/// Ping ICMP spre 1.1.1.1 la 5 s, timeout 1 s, pe thread-ul lui (o cerere fără răspuns blochează până la timeout).
const NO_PING: u32 = u32::MAX;

fn ping_loop(stop: Arc<AtomicBool>, last: Arc<AtomicU32>) {
    unsafe {
        let Ok(handle) = IcmpCreateFile() else { return };
        let payload = [0x61u8; 32];
        let mut reply = vec![0u64; (std::mem::size_of::<ICMP_ECHO_REPLY>() + payload.len() + 8) / 8 + 1];
        while !stop.load(Ordering::Relaxed) {
            let n = IcmpSendEcho(handle, u32::from_ne_bytes([1, 1, 1, 1]), payload.as_ptr().cast(), payload.len() as u16, None, reply.as_mut_ptr().cast(), (reply.len() * 8) as u32, 1000);
            let r = &*(reply.as_ptr() as *const ICMP_ECHO_REPLY);
            last.store(if n > 0 && r.Status == 0 { r.RoundTripTime } else { NO_PING }, Ordering::Relaxed);
            for _ in 0..50 {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        let _ = IcmpCloseHandle(handle);
    }
}

// ---------- sesiunea de citire ----------

#[derive(Serialize, Clone, Debug)]
pub struct Sample {
    pub cpu: f64,
    pub ram_used: u64,
    pub ram_total: u64,
    pub gpu: Option<f64>,
    pub gpu_temp: Option<f64>,
    pub cpu_temp: Option<f64>,
    pub net_down: f64,
    pub net_up: f64,
    pub ping: Option<u32>,
}

/// Temperatura plăcii NVIDIA se citește doar când placa lucrează deja (altfel citirea ar trezi-o) și cel mult la 5 s;
/// se păstrează 30 s după ultima citire.
const TEMP_EVERY: Duration = Duration::from_secs(5);
const GPU_TEMP_KEEP: Duration = Duration::from_secs(30);

struct Sampler {
    gpu: Option<Gpu>,
    nvidia: Option<String>,
    nvml: Option<Option<Nvml>>, // None = neîncercat, Some(None) = indisponibil
    stop: Arc<AtomicBool>,
    ping: Arc<AtomicU32>,
    prev_cpu: Option<(u64, u64, u64)>,
    prev_net: Vec<(u64, u64, u64)>,
    prev_at: Instant,
    gpu_temp: Option<(f64, Instant)>,
    last_nvml: Option<Instant>,
    cpu_temp: Option<f64>,
    last_wmi: Option<Instant>,
}

impl Sampler {
    fn new() -> Self {
        let gpu = Gpu::open();
        if gpu.is_none() {
            log::warn!("contoarele GPU nu s-au putut deschide; rândul video lipsește");
        }
        let (stop, ping) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicU32::new(NO_PING)));
        {
            let (stop, ping) = (stop.clone(), ping.clone());
            std::thread::spawn(move || ping_loop(stop, ping));
        }
        Self {
            gpu,
            nvidia: nvidia_luid(),
            nvml: None,
            stop,
            ping,
            prev_cpu: system_times(),
            prev_net: interfaces(),
            prev_at: Instant::now(),
            gpu_temp: None,
            last_nvml: None,
            cpu_temp: None,
            last_wmi: None,
        }
    }

    fn tick(&mut self) -> Sample {
        let now = Instant::now();
        let dt = now.duration_since(self.prev_at).as_secs_f64();
        self.prev_at = now;

        let cpu_now = system_times();
        let cpu = match (self.prev_cpu, cpu_now) {
            (Some(p), Some(n)) => cpu_percent(p, n),
            _ => 0.0,
        };
        self.prev_cpu = cpu_now;

        let net_now = interfaces();
        let (net_down, net_up) = rates(&self.prev_net, &net_now, dt);
        self.prev_net = net_now;

        let (ram_used, ram_total) = memory();
        let samples = self.gpu.as_ref().map(Gpu::read).unwrap_or_default();

        if self.last_wmi.map_or(true, |t| t.elapsed() >= TEMP_EVERY) {
            self.last_wmi = Some(now);
            self.cpu_temp = cpu_temp();
        }
        let busy = self.nvidia.as_deref().is_some_and(|luid| dgpu_busy(&samples, luid));
        if busy && self.last_nvml.map_or(true, |t| t.elapsed() >= TEMP_EVERY) {
            self.last_nvml = Some(now);
            if let Some(t) = self.nvml.get_or_insert_with(Nvml::open).as_ref().and_then(Nvml::temperature) {
                self.gpu_temp = Some((t, now));
            }
        }

        let ping = self.ping.load(Ordering::Relaxed);
        Sample {
            cpu,
            ram_used,
            ram_total,
            gpu: gpu_max(&samples),
            gpu_temp: self.gpu_temp.filter(|(_, at)| at.elapsed() < GPU_TEMP_KEEP).map(|(t, _)| t),
            cpu_temp: self.cpu_temp,
            net_down,
            net_up,
            ping: (ping != NO_PING).then_some(ping),
        }
    }
}

impl Drop for Sampler {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed); // oprește pingul; Gpu și Nvml se închid prin Drop
    }
}

// ---------- comanda și thread-ul ----------

/// Ferestrele-insulă (câte una pe monitor) care au fila Sistem deschisă; thread-ul citește cât lista nu e goală.
static WATCHERS: Mutex<Vec<String>> = Mutex::new(Vec::new());
static WAKE: Condvar = Condvar::new();

#[tauri::command]
pub fn system_watch(window: tauri::WebviewWindow, on: bool) {
    let mut watchers = WATCHERS.lock().unwrap();
    watchers.retain(|l| l != window.label());
    if on {
        watchers.push(window.label().to_string());
    }
    WAKE.notify_all();
}

/// Modulul pornit și cel puțin o fereastră (încă existentă) cu fila deschisă.
fn active(app: &AppHandle) -> bool {
    let enabled = app.state::<SettingsState>().get().modules.system;
    let mut watchers = WATCHERS.lock().unwrap();
    watchers.retain(|l| app.get_webview_window(l).is_some()); // o fereastră închisă (monitor scos) nu ține senzorii porniți
    enabled && !watchers.is_empty()
}

/// Doarme `dur`, dar se trezește imediat când nu mai e nicio fereastră care urmărește.
fn nap(dur: Duration) {
    let end = Instant::now() + dur;
    let mut watchers = WATCHERS.lock().unwrap();
    loop {
        let left = end.saturating_duration_since(Instant::now());
        if watchers.is_empty() || left.is_zero() {
            return;
        }
        watchers = WAKE.wait_timeout(watchers, left).unwrap().0;
    }
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || loop {
        {
            let mut watchers = WATCHERS.lock().unwrap();
            while watchers.is_empty() {
                watchers = WAKE.wait(watchers).unwrap(); // nimic nu e citit cât nu urmărește nimeni
            }
        }
        if !active(&app) {
            std::thread::sleep(Duration::from_secs(1)); // modul oprit, dar fila încă marcată deschisă
            continue;
        }
        let mut sampler = Sampler::new();
        nap(Duration::from_millis(500)); // primele valori vin repede, restul la 1 s
        while active(&app) {
            let _ = app.emit("system", sampler.tick());
            nap(Duration::from_secs(1));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procesor_procent() {
        assert_eq!(cpu_percent((100, 200, 100), (175, 300, 150)), 50.0); // idle +75 din total +150
        assert_eq!(cpu_percent((1, 1, 1), (1, 1, 1)), 0.0);
    }

    #[test]
    fn viteze_retea_fara_negative() {
        let (d, u) = rates(&[(1, 1000, 500), (2, 10, 10)], &[(1, 3000, 1500), (2, 5, 20)], 2.0);
        assert_eq!((d, u), (1000.0, 505.0)); // interfața 2: contor resetat la in → 0
        assert_eq!(rates(&[], &[(1, 3000, 1500)], 1.0), (0.0, 0.0));
    }

    fn sample() -> Vec<(String, f64)> {
        vec![
            ("pid_1_luid_0x00000000_0x0000000A_phys_0_eng_0_engtype_3D".into(), 30.0),
            ("pid_2_luid_0x00000000_0x0000000A_phys_0_eng_0_engtype_3D".into(), 25.0),
            ("pid_3_luid_0x00000000_0x0000000B_phys_0_eng_0_engtype_3D".into(), 40.0),
        ]
    }

    #[test]
    fn placa_video_maxim_pe_adaptor() {
        assert_eq!(gpu_max(&sample()), Some(55.0));
        assert_eq!(gpu_max(&[]), None);
        assert_eq!(gpu_max(&[("x_luid_0x0_0x1_phys_0".into(), 80.0), ("y_luid_0x0_0x1_phys_0".into(), 80.0)]), Some(100.0)); // plafon
    }

    #[test]
    fn placa_dedicata_lucreaza() {
        let s = sample();
        assert!(dgpu_busy(&s, "0x00000000_0x0000000B"));
        assert!(!dgpu_busy(&s, "0x00000000_0x0000000C"));
        assert!(!dgpu_busy(&[("pid_1_luid_0x00000000_0x0000000B_phys_0".into(), 0.0)], "0x00000000_0x0000000B")); // utilizare 0
    }

    /// Citește senzorii reali și îi tipărește: `cargo test -- --ignored --nocapture esantion_real`
    #[test]
    #[ignore]
    fn esantion_real() {
        let mut sampler = Sampler::new();
        println!("LUID NVIDIA: {:?}", sampler.nvidia);
        let instances = sampler.gpu.as_ref().map(Gpu::read).unwrap_or_default();
        let mut luids: Vec<_> = instances.iter().filter_map(|(n, _)| luid_of(n)).collect();
        luids.sort();
        luids.dedup();
        println!("instanțe PDH: {}, LUID-uri: {:?}", instances.len(), luids);
        for _ in 0..4 {
            std::thread::sleep(Duration::from_secs(1));
            println!("{:?}", sampler.tick());
        }
    }
}
