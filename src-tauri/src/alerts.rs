use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use windows::core::{IInspectable, Interface, HSTRING};
use windows::Devices::Bluetooth::{BluetoothConnectionStatus, BluetoothDevice, BluetoothLEDevice};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Foundation::Collections::IIterable;
use windows::Foundation::IReference;
use windows::Networking::Connectivity::{NetworkConnectivityLevel, NetworkInformation};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

use crate::i18n::t;
use crate::media::await_op_for;
use crate::settings::SettingsState;

/// O alertă afișată ca activitate tranzitorie în insulă.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Alert {
    pub icon: String,
    pub title: String,
    pub detail: String,
    pub color: String,
    pub level: Option<f64>,
}

fn alert(icon: &str, title: &str, detail: String, color: &str, level: Option<f64>) -> Alert {
    Alert { icon: icon.into(), title: title.into(), detail, color: color.into(), level }
}

// ---------------- baterie și încărcător ----------------

#[derive(Clone, Debug, PartialEq)]
pub struct Power {
    pub has_battery: bool,
    pub ac: bool,
    pub percent: u8,
}

pub fn power_transition(prev: Option<&Power>, cur: &Power) -> Option<Alert> {
    let prev = prev?;
    if !cur.has_battery || cur.percent > 100 {
        return None;
    }
    let pct = format!("{}%", cur.percent);
    let level = Some(cur.percent as f64 / 100.0);
    if cur.ac && !prev.ac {
        return Some(alert("bolt", &t("alert.charging"), pct, "var(--green)", level));
    }
    if !cur.ac && prev.ac {
        return Some(alert("battery", &t("alert.onBattery"), pct, "var(--fg)", level));
    }
    let crossed = |t: u8| prev.percent > t && cur.percent <= t;
    if !cur.ac && (crossed(20) || crossed(10)) {
        return Some(alert("battery", &t("alert.lowBattery"), pct, "var(--red)", level));
    }
    None
}

fn read_power() -> Option<Power> {
    let mut s = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut s).ok()? };
    Some(Power { has_battery: s.BatteryFlag & 128 == 0 && s.BatteryFlag != 255, ac: s.ACLineStatus == 1, percent: s.BatteryLifePercent })
}

// ---------------- Bluetooth ----------------

#[derive(Clone, Debug, PartialEq)]
pub struct BtDevice {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub battery: Option<u8>,
}

/// Tipul dispozitivului: clasa Bluetooth (audio → căști, periferic → controller) plus numele ca indiciu,
/// pentru că WinRT nu spune direct dacă un periferic e mouse sau tastatură.
pub fn kind_for(major: i32, minor: i32, name: &str) -> &'static str {
    let n = name.to_lowercase();
    match major {
        4 => "headphones",
        5 if n.contains("keyboard") || n.contains("keys") || n.contains("tastatur") => "keyboard",
        5 if n.contains("mouse") || n.contains("mx master") || n.contains("mice") => "mouse",
        5 if minor == 1 || minor == 2 => "gamepad",
        _ => "bluetooth",
    }
}

pub fn bt_transition(prev: Option<&[BtDevice]>, cur: &[BtDevice]) -> Vec<Alert> {
    let Some(prev) = prev else { return Vec::new() };
    let mut out = Vec::new();
    for d in cur.iter().filter(|d| !prev.iter().any(|p| p.id == d.id)) {
        let detail = match d.battery {
            Some(b) => format!("{} · {b}%", t("alert.connected")),
            None => t("alert.connected"),
        };
        out.push(alert(&d.kind, &d.name, detail, "var(--fg)", d.battery.map(|b| b as f64 / 100.0)));
    }
    for d in prev.iter().filter(|p| !cur.iter().any(|d| d.id == p.id)) {
        out.push(alert(&d.kind, &d.name, t("alert.disconnected"), "var(--fg2)", None));
    }
    out
}

// ---------------- rețea ----------------

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Net {
    pub online: bool,
    /// numele rețelei (Wi-Fi: SSID)
    pub name: String,
    /// numele conexiunii VPN active
    pub vpn: Option<String>,
}

pub fn net_transition(prev: Option<&Net>, cur: &Net) -> Vec<Alert> {
    let Some(prev) = prev else { return Vec::new() };
    let mut out = Vec::new();
    if prev.online && !cur.online {
        out.push(alert("wifiOff", &t("net.offline"), String::new(), "var(--red)", None));
    } else if cur.online && (!prev.online || prev.name != cur.name) {
        out.push(alert("wifi", &t("net.online"), cur.name.clone(), "var(--green)", None));
    }
    match (&prev.vpn, &cur.vpn) {
        (None, Some(v)) => out.push(alert("vpn", &t("net.vpnOn"), v.clone(), "var(--green)", None)),
        (Some(v), None) => out.push(alert("vpn", &t("net.vpnOff"), v.clone(), "var(--fg2)", None)),
        _ => {}
    }
    out
}

/// Conexiunea la internet și VPN-ul activ. VPN = o conexiune pe un adaptor virtual/tunel
/// (PPP 23, virtual 53, tunel 131): WireGuard, Tailscale, OpenVPN, VPN-ul Windows.
fn read_net() -> windows::core::Result<Net> {
    let mut net = Net::default();
    if let Ok(p) = NetworkInformation::GetInternetConnectionProfile() {
        net.online = p.GetNetworkConnectivityLevel()? == NetworkConnectivityLevel::InternetAccess;
        net.name = p.ProfileName().map(|n| n.to_string()).unwrap_or_default();
    }
    for p in NetworkInformation::GetConnectionProfiles()? {
        let kind = p.NetworkAdapter().and_then(|a| a.IanaInterfaceType()).unwrap_or(0);
        let up = p.GetNetworkConnectivityLevel().is_ok_and(|l| l != NetworkConnectivityLevel::None);
        if up && matches!(kind, 23 | 53 | 131) {
            net.vpn = Some(p.ProfileName().map(|n| n.to_string()).unwrap_or_default());
            break;
        }
    }
    Ok(net)
}

const P_BATTERY: &str = "{104EA319-6EE2-4701-BD47-8DDBF425BBE5} 2";

fn prop<T: windows::core::RuntimeType + 'static>(d: &DeviceInformation, key: &str) -> Option<T> {
    let v: IInspectable = d.Properties().ok()?.Lookup(&HSTRING::from(key)).ok()?;
    v.cast::<IReference<T>>().ok()?.Value().ok()
}

/// Dispozitivele Bluetooth împerecheate și conectate acum (clasic + Low Energy).
/// Lista împerecheate vine din registrul de dispozitive (rapid); o căutare de tip
/// AssociationEndpoint ar scana după dispozitive noi și ar dura zeci de secunde.
fn read_bluetooth() -> windows::core::Result<Vec<BtDevice>> {
    let props: IIterable<HSTRING> = vec![HSTRING::from(P_BATTERY)].try_into()?;
    let mut out = Vec::new();

    let sel = BluetoothDevice::GetDeviceSelectorFromPairingState(true)?;
    let list = await_op_for(DeviceInformation::FindAllAsyncAqsFilterAndAdditionalProperties(&sel, &props)?, Duration::from_secs(10))?;
    for d in list {
        // un dispozitiv care dispare între timp e sărit, nu oprește toată lista
        let Ok(id) = d.Id() else { continue };
        let Ok(op) = BluetoothDevice::FromIdAsync(&id) else { continue };
        let Ok(b) = await_op_for(op, Duration::from_secs(5)) else { continue };
        if b.ConnectionStatus().ok() != Some(BluetoothConnectionStatus::Connected) {
            continue;
        }
        let name = d.Name().map(|n| n.to_string()).unwrap_or_default();
        let (major, minor) = b.ClassOfDevice().map(|c| (c.MajorClass().map_or(0, |m| m.0), c.MinorClass().map_or(0, |m| m.0))).unwrap_or((0, 0));
        let kind = kind_for(major, minor, &name);
        out.push(BtDevice { id: id.to_string(), kind: kind.into(), battery: prop::<u8>(&d, P_BATTERY), name });
    }

    let sel = BluetoothLEDevice::GetDeviceSelectorFromPairingState(true)?;
    let list = await_op_for(DeviceInformation::FindAllAsyncAqsFilterAndAdditionalProperties(&sel, &props)?, Duration::from_secs(10))?;
    for d in list {
        // un dispozitiv care dispare între timp e sărit, nu oprește toată lista
        let Ok(id) = d.Id() else { continue };
        let Ok(op) = BluetoothLEDevice::FromIdAsync(&id) else { continue };
        let Ok(b) = await_op_for(op, Duration::from_secs(5)) else { continue };
        if b.ConnectionStatus().ok() != Some(BluetoothConnectionStatus::Connected) {
            continue;
        }
        let name = d.Name().map(|n| n.to_string()).unwrap_or_default();
        let kind = kind_for(0, 0, &name);
        out.push(BtDevice { id: id.to_string(), kind: kind.into(), battery: prop::<u8>(&d, P_BATTERY), name });
    }
    Ok(out)
}

/// Baterie și rețea la 2 s, Bluetooth la 4 s; emite `alert` pentru fiecare schimbare.
/// O schimbare de rețea e anunțată doar după ce se vede de 2 ori la rând (fără clipiri).
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut power: Option<Power> = None;
        let mut bt: Option<Vec<BtDevice>> = None;
        let mut net: Option<Net> = None;
        let mut net_seen: Option<Net> = None;
        let mut bt_retry_at = std::time::Instant::now();
        let mut tick = 0u32;
        loop {
            std::thread::sleep(Duration::from_secs(2));
            tick += 1;
            let modules = app.state::<SettingsState>().get().modules;
            if !modules.network {
                (net, net_seen) = (None, None);
            } else if let Ok(cur) = read_net() {
                if net.as_ref() != Some(&cur) && net_seen.as_ref() == Some(&cur) {
                    for a in net_transition(net.as_ref(), &cur) {
                        let _ = app.emit("alert", &a);
                    }
                    net = Some(cur.clone());
                } else if net.is_none() {
                    net = Some(cur.clone()); // prima citire: fără alerte
                }
                net_seen = Some(cur);
            }
            if !modules.alerts {
                power = None;
                bt = None;
                continue;
            }
            if let Some(cur) = read_power() {
                if let Some(a) = power_transition(power.as_ref(), &cur) {
                    let _ = app.emit("alert", &a);
                }
                power = Some(cur);
            }
            if tick % 2 == 0 && std::time::Instant::now() >= bt_retry_at {
                match read_bluetooth() {
                    Ok(cur) => {
                        for a in bt_transition(bt.as_deref(), &cur) {
                            let _ = app.emit("alert", &a);
                        }
                        bt = Some(cur);
                    }
                    Err(e) => {
                        log::warn!("Bluetooth indisponibil, reîncerc peste un minut: {e}");
                        bt_retry_at = std::time::Instant::now() + Duration::from_secs(60);
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(ac: bool, pct: u8) -> Power {
        Power { has_battery: true, ac, percent: pct }
    }

    #[test]
    fn fara_alerta_la_pornire_sau_fara_baterie() {
        assert!(power_transition(None, &p(true, 50)).is_none());
        let desktop = Power { has_battery: false, ac: true, percent: 255 };
        assert!(power_transition(Some(&desktop), &Power { ac: false, ..desktop.clone() }).is_none());
    }

    #[test]
    fn incarcator_conectat_si_deconectat() {
        let a = power_transition(Some(&p(false, 64)), &p(true, 64)).unwrap();
        assert_eq!((a.icon.as_str(), a.title.as_str(), a.detail.as_str()), ("bolt", "Se încarcă", "64%"));
        let b = power_transition(Some(&p(true, 64)), &p(false, 64)).unwrap();
        assert_eq!(b.title, "Pe baterie");
    }

    #[test]
    fn baterie_scazuta_la_20_si_10_o_singura_data() {
        assert_eq!(power_transition(Some(&p(false, 21)), &p(false, 20)).unwrap().title, "Baterie scăzută");
        assert!(power_transition(Some(&p(false, 20)), &p(false, 19)).is_none());
        assert_eq!(power_transition(Some(&p(false, 11)), &p(false, 10)).unwrap().detail, "10%");
        assert!(power_transition(Some(&p(true, 21)), &p(true, 20)).is_none()); // la încărcător nu
    }

    fn dev(id: &str, name: &str, kind: &str, battery: Option<u8>) -> BtDevice {
        BtDevice { id: id.into(), name: name.into(), kind: kind.into(), battery }
    }

    #[test]
    fn bluetooth_conectat_si_deconectat() {
        let before = vec![dev("1", "Mouse", "mouse", None)];
        let after = vec![dev("1", "Mouse", "mouse", None), dev("2", "AirPods Pro", "headphones", Some(80))];
        let alerts = bt_transition(Some(&before), &after);
        assert_eq!(alerts.len(), 1);
        assert_eq!((alerts[0].icon.as_str(), alerts[0].title.as_str(), alerts[0].detail.as_str()), ("headphones", "AirPods Pro", "Conectat · 80%"));
        assert_eq!(alerts[0].level, Some(0.8));
        let gone = bt_transition(Some(&after), &before);
        assert_eq!(gone[0].detail, "Deconectat");
        assert!(bt_transition(None, &after).is_empty()); // prima citire: fără alerte
    }

    #[test]
    fn retea_pierduta_revenita_si_vpn() {
        let wifi = |name: &str| Net { online: true, name: name.into(), vpn: None };
        assert!(net_transition(None, &wifi("Acasa")).is_empty());
        let off = Net::default();
        let a = net_transition(Some(&wifi("Acasa")), &off);
        assert_eq!((a.len(), a[0].icon.as_str(), a[0].color.as_str()), (1, "wifiOff", "var(--red)"));
        let b = net_transition(Some(&off), &wifi("Acasa"));
        assert_eq!((b[0].icon.as_str(), b[0].detail.as_str()), ("wifi", "Acasa"));
        assert_eq!(net_transition(Some(&wifi("Acasa")), &wifi("Birou"))[0].detail, "Birou");
        assert!(net_transition(Some(&wifi("Acasa")), &wifi("Acasa")).is_empty());
        let vpn = Net { vpn: Some("Tailscale".into()), ..wifi("Acasa") };
        let c = net_transition(Some(&wifi("Acasa")), &vpn);
        assert_eq!((c.len(), c[0].icon.as_str(), c[0].detail.as_str()), (1, "vpn", "Tailscale"));
        assert_eq!(net_transition(Some(&vpn), &wifi("Acasa"))[0].color, "var(--fg2)");
    }

    #[test]
    fn tipul_dispozitivului_din_clasa_bluetooth() {
        assert_eq!(kind_for(4, 6, "AirPods Pro 3"), "headphones");
        assert_eq!(kind_for(5, 2, "DualSense Wireless Controller"), "gamepad");
        assert_eq!(kind_for(5, 0, "MX Keys"), "keyboard");
        assert_eq!(kind_for(5, 0, "Logitech Mouse"), "mouse");
        assert_eq!(kind_for(1, 0, "Laptop"), "bluetooth");
    }

    /// Rulează doar manual (`cargo test -- --ignored`): bateria și dispozitivele Bluetooth reale.
    #[test]
    #[ignore]
    fn citeste_dispozitivele_reale() {
        println!("baterie: {:?}", read_power());
        println!("rețea: {:?}", read_net());
        match read_bluetooth() {
            Ok(list) => {
                println!("Bluetooth conectate: {}", list.len());
                for d in list {
                    println!("  {} [{}] baterie={:?}", d.name, d.kind, d.battery);
                }
            }
            Err(e) => println!("Bluetooth: eroare {e}"),
        }
    }
}
