use crate::media::await_op;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::{interface, IUnknown, IUnknown_Vtbl, GUID, HRESULT, HSTRING, PCWSTR, PWSTR};
use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED};
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};
use winreg::enums::{RegType::REG_BINARY, HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE};
use winreg::{RegKey, RegValue};

const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

/// `None` = indisponibil pe acest PC → butonul lipsește.
#[derive(Serialize, Clone, Debug)]
pub struct Controls {
    pub wifi: Option<bool>,
    pub bluetooth: Option<bool>,
    pub dark: Option<bool>,
    pub night: Option<bool>,
    pub dnd: Option<bool>,
    pub mic_muted: Option<bool>,
}

/// (`AppsUseLightTheme`, `SystemUsesLightTheme`) pentru modul cerut: 0 = întunecat, 1 = deschis.
pub fn dark_values(on: bool) -> (u32, u32) {
    if on { (0, 0) } else { (1, 1) }
}

// ---------- radiouri (Wi-Fi / Bluetooth) ----------

/// Toate radiourile, dar numai după ce accesul e `Allowed`; altfel gol (butoanele lipsesc, nu eșuează tăcut).
/// Se reține doar un răspuns `Allowed`, ca un refuz de acum să poată fi reîncercat mai târziu.
fn radios() -> Vec<Radio> {
    static ALLOWED: AtomicBool = AtomicBool::new(false);
    if !ALLOWED.load(Ordering::Relaxed) {
        if !matches!(Radio::RequestAccessAsync().and_then(await_op), Ok(RadioAccessStatus::Allowed)) {
            return Vec::new();
        }
        ALLOWED.store(true, Ordering::Relaxed);
    }
    let Ok(all) = Radio::GetRadiosAsync().and_then(await_op) else { return Vec::new() };
    all.into_iter().collect()
}

fn of_kind(all: &[Radio], kind: RadioKind) -> Vec<&Radio> {
    all.iter().filter(|r| r.Kind().ok() == Some(kind)).collect()
}

fn radio_on(all: &[Radio], kind: RadioKind) -> Option<bool> {
    let rs = of_kind(all, kind);
    if rs.is_empty() { None } else { Some(rs.iter().any(|r| r.State().ok() == Some(RadioState::On))) }
}

fn set_radio(kind: RadioKind, on: bool) -> Result<(), String> {
    let all = radios();
    let rs = of_kind(&all, kind);
    if rs.is_empty() {
        return Err("nu există radio sau accesul e refuzat".into());
    }
    let state = if on { RadioState::On } else { RadioState::Off };
    for r in rs {
        let status = r.SetStateAsync(state).and_then(await_op).map_err(|e| e.to_string())?;
        if status != RadioAccessStatus::Allowed {
            return Err(format!("acces refuzat la radio ({})", status.0));
        }
    }
    Ok(())
}

// ---------- mod întunecat ----------

fn dark_on() -> Option<bool> {
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(PERSONALIZE, KEY_READ).ok()?;
    Some(key.get_value::<u32, _>("AppsUseLightTheme").ok()? == 0)
}

fn set_dark(on: bool) -> Result<(), String> {
    let (apps, system) = dark_values(on);
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(PERSONALIZE, KEY_SET_VALUE).map_err(|e| e.to_string())?;
    key.set_value("AppsUseLightTheme", &apps).map_err(|e| e.to_string())?;
    key.set_value("SystemUsesLightTheme", &system).map_err(|e| e.to_string())?;
    // fără anunț, Explorer și aplicațiile deschise își păstrează vechea temă până la repornire
    let area: Vec<u16> = "ImmersiveColorSet\0".encode_utf16().collect();
    unsafe {
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, WPARAM(0), LPARAM(area.as_ptr() as isize), SMTO_ABORTIFHUNG, 200, None);
    }
    Ok(())
}

// ---------- lumină de noapte (blob CloudStore nedocumentat) ----------
//
// `Data` e un struct Bond CompactBinary v1 (formatul descifrat de kvnxiao/win-nightlight-cli):
// antet `43 42 01 00`, `0a 02 01 00` (metadate), `2a 06 <varint secunde Unix>`, `2a 2b 0e <varint n>`
// + n octeți (structura interioară), `00 00 00`. Structura interioară: `43 42 01 00`, câmpuri, `00`.
// Câmpuri: 0 int32 (prezent = pornită, valoarea 0), 10 int32 cauza (1 = manual, zigzag `02`),
// 20 uint64 FILETIME-ul ultimei schimbări, 30 bool „folosibilă” (lipsă = da).
// Orice altceva (alt antet, alt tip, alt câmp) → `None`: butonul lipsește, nu scriem ce nu înțelegem.

const NIGHT: &str = r"Software\Microsoft\Windows\CurrentVersion\CloudStore\Store\DefaultAccount\Current\default$windows.data.bluelightreduction.bluelightreductionstate\windows.data.bluelightreduction.bluelightreductionstate";
const CB: [u8; 4] = [0x43, 0x42, 0x01, 0x00];
const OUTER_HEAD: [u8; 10] = [0x43, 0x42, 0x01, 0x00, 0x0a, 0x02, 0x01, 0x00, 0x2a, 0x06];
const LIST_HEAD: [u8; 3] = [0x2a, 0x2b, 0x0e];
const BT_BOOL: u8 = 2;
const BT_UINT64: u8 = 6;
const BT_INT32: u8 = 16;

fn varint(b: &[u8], i: &mut usize) -> Option<u64> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let x = *b.get(*i)?;
        *i += 1;
        v |= u64::from(x & 0x7f) << shift;
        if x & 0x80 == 0 {
            return Some(v);
        }
    }
    None
}

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

/// (secunde Unix, structura interioară) dacă învelișul are exact forma cunoscută.
fn unwrap_cloudstore(b: &[u8]) -> Option<(u64, &[u8])> {
    if b.get(..10)? != OUTER_HEAD {
        return None;
    }
    let mut i = 10;
    let ts = varint(b, &mut i)?;
    if b.get(i..i + 3)? != LIST_HEAD {
        return None;
    }
    i += 3;
    let n = usize::try_from(varint(b, &mut i)?).ok()?;
    let inner = b.get(i..i.checked_add(n)?)?;
    (b.get(i + n..)? == [0, 0, 0]).then_some((ts, inner))
}

fn wrap_cloudstore(ts: u64, inner: &[u8]) -> Vec<u8> {
    let mut out = OUTER_HEAD.to_vec();
    put_varint(&mut out, ts);
    out.extend(LIST_HEAD);
    put_varint(&mut out, inner.len() as u64);
    out.extend(inner);
    out.extend([0, 0, 0]);
    out
}

/// Câmpurile structurii interioare `(id, tip, valoare)`, validate față de forma cunoscută.
fn night_fields(inner: &[u8]) -> Option<Vec<(u8, u8, Vec<u8>)>> {
    if inner.get(..4)? != CB {
        return None;
    }
    let (mut i, mut out) = (4, Vec::new());
    loop {
        let h = *inner.get(i)?;
        i += 1;
        if h == 0 {
            return (i == inner.len()).then_some(out); // stop: trebuie să fie ultimul octet
        }
        let (ty, id) = (h & 0x1f, match h >> 5 {
            6 => {
                i += 1;
                *inner.get(i - 1)?
            }
            7 => return None,
            id => id,
        });
        let start = i;
        match ty {
            BT_BOOL => i += 1,
            BT_UINT64 | BT_INT32 => _ = varint(inner, &mut i)?,
            _ => return None,
        }
        let v = inner.get(start..i)?.to_vec();
        let known = match (id, ty) {
            (0, BT_INT32) => v == [0],
            (10, BT_INT32) | (20, BT_UINT64) => true,
            (30, BT_BOOL) => v == [1],
            _ => false,
        };
        if !known || out.last().is_some_and(|(last, _, _)| *last >= id) {
            return None;
        }
        out.push((id, ty, v));
    }
}

/// `Some(true)` = pornită; `None` = format necunoscut sau nefolosibilă pe acest dispozitiv.
pub fn night_light_state(blob: &[u8]) -> Option<bool> {
    let (_, inner) = unwrap_cloudstore(blob)?;
    Some(night_fields(inner)?.iter().any(|(id, _, _)| *id == 0))
}

/// Blob-ul nou cu starea cerută, marcat ca schimbare manuală acum; `None` dacă formatul nu e recunoscut.
pub fn night_light_set(blob: &[u8], on: bool) -> Option<Vec<u8>> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?;
    // FILETIME: intervale de 100 ns de la 1601-01-01
    let filetime = (now.as_secs() + 11_644_473_600) * 10_000_000 + u64::from(now.subsec_nanos() / 100);
    night_light_set_at(blob, on, now.as_secs(), filetime)
}

fn night_light_set_at(blob: &[u8], on: bool, unix: u64, filetime: u64) -> Option<Vec<u8>> {
    let (_, inner) = unwrap_cloudstore(blob)?;
    let mut fields = night_fields(inner)?;
    fields.retain(|(id, _, _)| ![0, 10, 20].contains(id));
    if on {
        fields.push((0, BT_INT32, vec![0]));
    }
    fields.push((10, BT_INT32, vec![2])); // cauza 1 = manual (zigzag)
    let mut ft = Vec::new();
    put_varint(&mut ft, filetime);
    fields.push((20, BT_UINT64, ft));
    fields.sort_by_key(|(id, _, _)| *id);
    let mut out = CB.to_vec();
    for (id, ty, v) in fields {
        if id < 6 {
            out.push(id << 5 | ty);
        } else {
            out.extend([0xc0 | ty, id]);
        }
        out.extend(v);
    }
    out.push(0);
    Some(wrap_cloudstore(unix, &out))
}

fn night_blob() -> Option<Vec<u8>> {
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(NIGHT, KEY_READ).ok()?;
    let v = key.get_raw_value("Data").ok()?;
    (v.vtype == REG_BINARY).then_some(v.bytes)
}

fn night_on() -> Option<bool> {
    night_light_state(&night_blob()?)
}

/// Doar blob-ul: Windows urmărește cheia și aplică singur schimbarea.
fn set_night(on: bool) -> Result<(), String> {
    let blob = night_blob().ok_or("lumina de noapte indisponibilă")?;
    if night_light_state(&blob) == Some(on) {
        return Ok(());
    }
    let bytes = night_light_set(&blob, on).ok_or("format necunoscut al luminii de noapte")?;
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(NIGHT, KEY_SET_VALUE).map_err(|e| e.to_string())?;
    key.set_raw_value("Data", &RegValue { bytes, vtype: REG_BINARY }).map_err(|e| e.to_string())
}

// ---------- Nu deranja (QuietHoursSettings, COM nedocumentat) ----------
//
// Pe Windows 11, „Nu deranja” = profilul ales în serviciul shell-ului de „quiet hours”
// (interfața din IDL-ul lui Rafael Rivera, folosită de tdesktop, yasb ș.a.). Serviciul salvează
// singur profilul în CloudStore (`…donotdisturb.quiethourssettings`) și anunță sistemul.
// `NOC_GLOBAL_SETTING_TOASTS_ENABLED` e comutatorul general al notificărilor, nu „Nu deranja”.

const QH_OFF: &str = "Microsoft.QuietHoursProfile.Unrestricted";
const QH_ON: &str = "Microsoft.QuietHoursProfile.PriorityOnly";
const CLSID_QUIET_HOURS: GUID = GUID::from_u128(0xf53321fa_34f8_4b7f_b9a3_361877cb94cf);

// doar primele două metode; ordinea lor în tabelul virtual e cea din IDL
#[interface("6bff4732-81ec-4ffb-ae67-b6c1bc29631f")]
unsafe trait IQuietHoursSettings: IUnknown {
    fn user_selected_profile(&self, profile: *mut PWSTR) -> HRESULT;
    fn set_user_selected_profile(&self, profile: PCWSTR) -> HRESULT;
}

/// Profil necunoscut sau lipsă → `None` (butonul lipsește).
pub fn dnd_from(profile: Option<&str>) -> Option<bool> {
    match profile? {
        QH_OFF => Some(false),
        QH_ON | "Microsoft.QuietHoursProfile.AlarmsOnly" => Some(true),
        _ => None,
    }
}

fn quiet_hours() -> Option<IQuietHoursSettings> {
    unsafe { CoCreateInstance(&CLSID_QUIET_HOURS, None, CLSCTX_LOCAL_SERVER) }.ok()
}

fn dnd_on() -> Option<bool> {
    let qh = quiet_hours()?;
    let mut p = PWSTR::null();
    unsafe { qh.user_selected_profile(&mut p) }.ok().ok()?;
    if p.is_null() {
        return None;
    }
    let s = unsafe { p.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(p.0 as *const _)) };
    dnd_from(s.as_deref())
}

fn set_dnd(on: bool) -> Result<(), String> {
    let qh = quiet_hours().ok_or("Nu deranja indisponibil")?;
    let p = HSTRING::from(if on { QH_ON } else { QH_OFF });
    unsafe { qh.set_user_selected_profile(PCWSTR(p.as_ptr())) }.ok().map_err(|e| e.to_string())
}

// ---------- comenzi ----------

fn read_state() -> Controls {
    let _com = crate::platform::ComGuard::new(COINIT_MULTITHREADED);
    let all = radios(); // o singură enumerare pentru Wi-Fi și Bluetooth
    Controls {
        wifi: radio_on(&all, RadioKind::WiFi),
        bluetooth: radio_on(&all, RadioKind::Bluetooth),
        dark: dark_on(),
        night: night_on(),
        dnd: dnd_on(),
        mic_muted: crate::mic::enumerator().and_then(|e| crate::mic::capture_mute(&e)),
    }
}

#[tauri::command]
pub async fn controls_state() -> Result<Controls, String> {
    // la eroare de join JS păstrează starea veche (nu un Controls gol, care ar ascunde butoanele)
    tauri::async_runtime::spawn_blocking(read_state).await.map_err(|e| e.to_string())
}

/// `on` = starea cerută a câmpului din `Controls` (pentru microfon: `true` = pe mute). Întoarce starea nouă.
#[tauri::command]
pub async fn controls_set(name: String, on: bool) -> Result<Controls, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _com = crate::platform::ComGuard::new(COINIT_MULTITHREADED);
        match name.as_str() {
            "wifi" => set_radio(RadioKind::WiFi, on)?,
            "bluetooth" => set_radio(RadioKind::Bluetooth, on)?,
            "dark" => set_dark(on)?,
            "night" => set_night(on)?,
            "dnd" => set_dnd(on)?,
            "mic" => crate::mic::set_capture_mute(&crate::mic::enumerator().ok_or("Core Audio indisponibil")?, on)?,
            _ => return Err(format!("control necunoscut: {name}")),
        }
        Ok(read_state())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn controls_action(name: String) -> Result<(), String> {
    match name.as_str() {
        "lock" => unsafe { LockWorkStation() }.map_err(|e| e.to_string()),
        // SetSuspendState se întoarce abia la trezire: pe firul lui, ca să nu blocăm comanda
        "sleep" => {
            std::thread::spawn(|| {
                if !unsafe { SetSuspendState(false, false, false) }.as_bool() {
                    log::warn!("SetSuspendState a eșuat (sleep neexecutat)");
                }
            });
            Ok(())
        }
        _ => Err(format!("acțiune necunoscută: {name}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REAL: citit (doar citire) de pe PC-ul lui Andrei, 2026-10-17, cheia `default$…bluelightreductionstate`.
    /// Câmpul 0 (`10 00`) e prezent → lumina de noapte pornită, schimbare manuală (`d0 0a 02`).
    const REAL_ON: &[u8] = &[
        0x43, 0x42, 0x01, 0x00, 0x0a, 0x02, 0x01, 0x00, 0x2a, 0x06, 0xc4, 0x9b, 0x88, 0xd4, 0x06, 0x2a, 0x2b, 0x0e, 0x15,
        0x43, 0x42, 0x01, 0x00, 0x10, 0x00, 0xd0, 0x0a, 0x02, 0xc6, 0x14, 0xd8, 0x87, 0xe3, 0xb1, 0xc7, 0xb6, 0xcb, 0xee, 0x01, 0x00,
        0x00, 0x00, 0x00,
    ];
    /// DERIVAT din REAL_ON: fără `10 00`, lungimea interioară 0x15 → 0x13 (ca STATE_INACTIVE din win-nightlight-cli).
    const OFF_SAMPLE: &[u8] = &[
        0x43, 0x42, 0x01, 0x00, 0x0a, 0x02, 0x01, 0x00, 0x2a, 0x06, 0xc4, 0x9b, 0x88, 0xd4, 0x06, 0x2a, 0x2b, 0x0e, 0x13,
        0x43, 0x42, 0x01, 0x00, 0xd0, 0x0a, 0x02, 0xc6, 0x14, 0xd8, 0x87, 0xe3, 0xb1, 0xc7, 0xb6, 0xcb, 0xee, 0x01, 0x00,
        0x00, 0x00, 0x00,
    ];
    /// Mostră reală de pe alt PC (STATE_LIVE din kvnxiao/win-nightlight-cli, fixtures.rs): oprită.
    const COMMUNITY_OFF: &[u8] = &[
        0x43, 0x42, 0x01, 0x00, 0x0a, 0x02, 0x01, 0x00, 0x2a, 0x06, 0x9d, 0xed, 0x93, 0xce, 0x06, 0x2a, 0x2b, 0x0e, 0x13,
        0x43, 0x42, 0x01, 0x00, 0xd0, 0x0a, 0x02, 0xc6, 0x14, 0xb0, 0x9f, 0xd1, 0xe6, 0xf8, 0x9f, 0xaf, 0xee, 0x01, 0x00,
        0x00, 0x00, 0x00,
    ];
    /// STATE_ACTIVE_SCHEDULED din același proiect: pornită de program (fără câmpul 10).
    const SCHEDULED_ON: &[u8] = &[
        0x43, 0x42, 0x01, 0x00, 0x0a, 0x02, 0x01, 0x00, 0x2a, 0x06, 0x89, 0x95, 0xfc, 0xbe, 0x06, 0x2a, 0x2b, 0x0e, 0x12,
        0x43, 0x42, 0x01, 0x00, 0x10, 0x00, 0xc6, 0x14, 0xa9, 0xf6, 0xe2, 0xd3, 0xef, 0xea, 0xe6, 0xed, 0x01, 0x00,
        0x00, 0x00, 0x00,
    ];

    /// (secunde Unix din antet, FILETIME din câmpul 20) ale mostrei reale.
    fn real_times() -> (u64, u64) {
        let (mut a, mut b) = (10, 30);
        (varint(REAL_ON, &mut a).unwrap(), varint(REAL_ON, &mut b).unwrap())
    }

    #[test]
    fn lumina_de_noapte_citita_si_comutata() {
        assert_eq!(night_light_state(OFF_SAMPLE), Some(false));
        let on = night_light_set(OFF_SAMPLE, true).unwrap();
        assert_eq!(night_light_state(&on), Some(true));
        assert_eq!(night_light_state(&night_light_set(&on, false).unwrap()), Some(false));
    }

    #[test]
    fn mostra_reala_si_scriere_identica_cu_windows() {
        assert_eq!(night_light_state(REAL_ON), Some(true));
        let (ts, ft) = real_times();
        // cu aceleași momente, scrierea noastră reproduce octet cu octet ce a scris Windows
        assert_eq!(night_light_set_at(OFF_SAMPLE, true, ts, ft).unwrap(), REAL_ON);
        assert_eq!(night_light_set_at(REAL_ON, false, ts, ft).unwrap(), OFF_SAMPLE);
    }

    #[test]
    fn scrierea_actualizeaza_momentele() {
        let out = night_light_set_at(REAL_ON, false, 1_792_240_976, 134_000_000_000_000_000).unwrap();
        let mut i = 10;
        assert_eq!(varint(&out, &mut i), Some(1_792_240_976));
        assert!(out.windows(2).any(|w| w == [0xc6, 0x14]));
        let (_, inner) = unwrap_cloudstore(&out).unwrap();
        let mut j = inner.len() - 1 - 9; // FILETIME-ul are 9 octeți de varint înainte de stop
        assert_eq!(varint(inner, &mut j), Some(134_000_000_000_000_000));
    }

    #[test]
    fn mostre_din_comunitate() {
        assert_eq!(night_light_state(COMMUNITY_OFF), Some(false));
        assert_eq!(night_light_state(&night_light_set(COMMUNITY_OFF, true).unwrap()), Some(true));
        assert_eq!(night_light_state(SCHEDULED_ON), Some(true));
        // oprirea manuală adaugă cauza „manual” (câmpul 10) în ordinea câmpurilor
        let off = night_light_set_at(SCHEDULED_ON, false, 1, 2).unwrap();
        assert_eq!(night_light_state(&off), Some(false));
        let (_, inner) = unwrap_cloudstore(&off).unwrap();
        assert_eq!(inner, [0x43, 0x42, 0x01, 0x00, 0xd0, 0x0a, 0x02, 0xc6, 0x14, 0x02, 0x00]);
    }

    #[test]
    fn format_necunoscut_nu_se_scrie() {
        assert_eq!(night_light_state(&[1, 2, 3]), None);
        assert_eq!(night_light_set(&[1, 2, 3], true), None);
        let bad = |f: &dyn Fn(&mut Vec<u8>)| {
            let mut b = REAL_ON.to_vec();
            f(&mut b);
            assert_eq!(night_light_state(&b), None, "{b:02x?}");
            assert_eq!(night_light_set(&b, false), None);
        };
        bad(&|b| b.truncate(42)); // trunchiat
        bad(&|b| b.push(0)); // coadă în plus
        bad(&|b| b[18] = 0x14); // lungime interioară greșită
        bad(&|b| b[24] = 0x02); // starea are altă valoare decât 0
        bad(&|b| b[25] = 0xc9); // tip necunoscut (string) la câmpul 10
        bad(&|b| b[26] = 0x05); // câmp necunoscut (id 5)
        bad(&|b| b[4] = 0x0b); // antet exterior diferit
        // „nefolosibilă pe acest dispozitiv” (câmpul 30 = false) → butonul lipsește
        bad(&|b| {
            b.splice(39..39, [0xc2, 0x1e, 0x00]);
            b[18] = 0x18;
        });
    }

    #[test]
    fn nu_deranja_din_profil() {
        assert_eq!(dnd_from(Some("Microsoft.QuietHoursProfile.Unrestricted")), Some(false));
        assert_eq!(dnd_from(Some("Microsoft.QuietHoursProfile.PriorityOnly")), Some(true));
        assert_eq!(dnd_from(Some("Microsoft.QuietHoursProfile.AlarmsOnly")), Some(true));
        assert_eq!(dnd_from(Some("altceva")), None);
        assert_eq!(dnd_from(None), None);
    }

    #[test]
    fn valori_mod_intunecat() {
        assert_eq!(dark_values(true), (0, 0));
        assert_eq!(dark_values(false), (1, 1));
    }

    /// Doar citire (R2: niciodată nu comută nimic). `cargo test controls -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn citeste_starea_de_pe_pc() {
        println!("{:?}", read_state());
    }
}
