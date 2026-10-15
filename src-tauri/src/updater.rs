use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::i18n::{t, tv};
use crate::settings::SettingsState;

/// Timeout pentru fiecare cerere a updater-ului (verificare și descărcare).
pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);
/// La pornire nu așteptăm mai mult de atât după server (insula trebuie să apară).
const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);

/// Verificarea e datorată: prima la 30 s după pornire, apoi la 6 h după ultima, după ceasul real
/// (un `sleep` de 6 h nu numără timpul în care laptopul doarme). Timpi în secunde unix.
pub fn check_due(started: u64, last: Option<u64>, now: u64) -> bool {
    match last {
        None => now >= started + 30,
        Some(l) => now < l || now >= l + 6 * 3600,
    }
}

/// Descărcarea a eșuat: dacă un update e deja descărcat, rămâne gata de aplicat.
pub fn after_failure(kept: Option<String>, msg: String, at: u64) -> UpdEvent {
    match kept {
        Some(v) => UpdEvent::Downloaded(v),
        None => UpdEvent::Failed(msg, at),
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UpdateState {
    Idle,
    Checking,
    UpToDate { checked_at: u64 },
    Downloading { version: String },
    Ready { version: String },
    Error { msg: String, checked_at: u64 },
}

pub enum UpdEvent {
    CheckStarted,
    NoUpdate(u64),
    Found(String),
    Downloaded(String),
    Failed(String, u64),
}

/// Un update descărcat (`Ready`) rămâne gata până e aplicat, orice ar spune verificările
/// ulterioare; doar o versiune și mai nouă îl înlocuiește.
pub fn next_state(s: &UpdateState, ev: UpdEvent) -> UpdateState {
    if let UpdateState::Ready { version } = s {
        return match ev {
            UpdEvent::Found(v) if v != *version => UpdateState::Downloading { version: v },
            UpdEvent::Downloaded(v) => UpdateState::Ready { version: v },
            _ => s.clone(),
        };
    }
    match ev {
        UpdEvent::CheckStarted => UpdateState::Checking,
        UpdEvent::NoUpdate(t) => UpdateState::UpToDate { checked_at: t },
        UpdEvent::Found(v) => UpdateState::Downloading { version: v },
        UpdEvent::Downloaded(v) => UpdateState::Ready { version: v },
        UpdEvent::Failed(msg, t) => UpdateState::Error { msg, checked_at: t },
    }
}

fn parse(v: &str) -> Option<(u64, u64, u64)> {
    let mut it = v.split('.').map(|p| p.parse::<u64>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

pub fn newer(a: &str, b: &str) -> bool {
    matches!((parse(a), parse(b)), (Some(x), Some(y)) if x > y)
}

#[derive(Debug, PartialEq)]
pub enum PendingAction {
    Nothing,
    Install,
    Discard,
    Keep,
}

/// Ce facem la pornire cu un update descărcat dar neaplicat.
pub fn pending_action(pending: Option<&str>, current: &str, remote: Option<&str>) -> PendingAction {
    let Some(p) = pending else { return PendingAction::Nothing };
    if !newer(p, current) {
        return PendingAction::Discard;
    }
    match remote {
        None => PendingAction::Keep,
        Some(r) if r == p => PendingAction::Install,
        Some(_) => PendingAction::Discard,
    }
}

#[derive(Serialize, Deserialize)]
struct PendingFile {
    version: String,
}

/// Starea updater-ului + update-ul descărcat (în memorie, cât rulează aplicația).
#[derive(Default)]
pub struct Updater {
    state: Mutex<Option<UpdateState>>,
    ready: Mutex<Option<(Update, Vec<u8>)>>,
    busy: Mutex<()>,
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn updates_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_cache_dir().ok().map(|d| d.join("updates"))
}

fn read_pending(app: &AppHandle) -> Option<(String, Vec<u8>)> {
    let dir = updates_dir(app)?;
    let p: PendingFile = serde_json::from_str(&std::fs::read_to_string(dir.join("pending.json")).ok()?).ok()?;
    let bytes = std::fs::read(dir.join(format!("DynamicIsland_{}.exe", p.version))).ok()?;
    Some((p.version, bytes))
}

fn clear_pending(app: &AppHandle) {
    if let Some(dir) = updates_dir(app) {
        let _ = std::fs::remove_dir_all(dir);
    }
}

fn save_pending(app: &AppHandle, version: &str, bytes: &[u8]) -> std::io::Result<()> {
    clear_pending(app);
    let dir = updates_dir(app).ok_or_else(|| std::io::Error::other("fără app_cache_dir"))?;
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(format!("DynamicIsland_{version}.exe")), bytes)?;
    std::fs::write(dir.join("pending.json"), serde_json::to_string(&PendingFile { version: version.into() })?)
}

pub fn current_state(app: &AppHandle) -> UpdateState {
    app.state::<Updater>().state.lock().unwrap().clone().unwrap_or(UpdateState::Idle)
}

fn transition(app: &AppHandle, ev: UpdEvent) {
    let up = app.state::<Updater>();
    let new = {
        let mut st = up.state.lock().unwrap();
        let n = next_state(st.as_ref().unwrap_or(&UpdateState::Idle), ev);
        *st = Some(n.clone());
        n
    };
    let _ = app.emit("update-state", &new);
    crate::tray::refresh_update_item(app, &new);
}

/// O verificare completă: caută, descarcă și verifică semnătura, salvează. Nu instalează.
pub fn run_check(app: &AppHandle) {
    let up = app.state::<Updater>();
    let Ok(_busy) = up.busy.try_lock() else { return };
    transition(app, UpdEvent::CheckStarted);
    let was_ready = app.state::<Updater>().ready.lock().unwrap().is_some();
    let result = tauri::async_runtime::block_on(async {
        let Some(update) = app.updater_builder().timeout(NETWORK_TIMEOUT).build()?.check().await? else {
            return Ok::<_, tauri_plugin_updater::Error>(None);
        };
        let version = update.version.clone();
        if was_ready && matches!(current_state(app), UpdateState::Ready { version: ref v } if *v == version) {
            return Ok(Some(version));
        }
        transition(app, UpdEvent::Found(version.clone()));
        let bytes = update.download(|_, _| {}, || {}).await?;
        if let Err(e) = save_pending(app, &version, &bytes) {
            log::warn!("nu pot salva update-ul descărcat: {e}");
        }
        *app.state::<Updater>().ready.lock().unwrap() = Some((update, bytes));
        transition(app, UpdEvent::Downloaded(version.clone()));
        log::info!("update v{version} descărcat și verificat; aștept aplicarea");
        let _ = app
            .notification()
            .builder()
            .title(tv("update.notifTitle", &version))
            .body(t("update.notifBody"))
            .show();
        Ok(Some(version))
    });
    match result {
        Ok(None) => transition(app, UpdEvent::NoUpdate(now_secs())),
        Ok(Some(_)) => {}
        Err(e) => {
            log::warn!("verificarea update-urilor a eșuat: {e}");
            let kept = app.state::<Updater>().ready.lock().unwrap().as_ref().map(|(u, _)| u.version.clone());
            transition(app, after_failure(kept, e.to_string(), now_secs()));
        }
    }
}

/// Instalează update-ul descărcat. Pe Windows, installerul închide aplicația și o repornește.
pub fn apply(app: &AppHandle) -> Result<(), String> {
    let taken = app.state::<Updater>().ready.lock().unwrap().take();
    let (update, bytes) = match taken {
        Some(x) => x,
        None => return Err("nu există niciun update descărcat".into()),
    };
    log::info!("aplic update-ul v{}", update.version);
    let _ = app.emit("update-installing", update.version.clone());
    crate::volume::restore_osd(); // installerul închide aplicația fără RunEvent::Exit
    crate::tray::hide_icon(app);
    match update.install(&bytes) {
        Ok(()) => Ok(()),
        Err(e) => {
            log::warn!("instalarea a eșuat: {e}");
            crate::tray::show_icon(app);
            let _ = app.emit("update-failed", ());
            // update-ul rămâne gata, ca „Aplică” să poată fi încercat din nou
            *app.state::<Updater>().ready.lock().unwrap() = Some((update, bytes));
            Err(e.to_string())
        }
    }
}

/// La pornire, înainte de crearea insulelor: un update descărcat dar neaplicat se instalează acum.
pub fn install_pending_at_startup(app: &AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    let current = app.package_info().version.to_string();
    let pending = read_pending(app);
    let remote = pending.as_ref().and_then(|_| {
        tauri::async_runtime::block_on(async {
            // timeout-ul cererii limitează așteptarea; fără rețea pornirea continuă
            let u = app.updater_builder().timeout(STARTUP_TIMEOUT).build().ok()?;
            u.check().await.ok()?
        })
    });
    match pending_action(pending.as_ref().map(|(v, _)| v.as_str()), &current, remote.as_ref().map(|u| u.version.as_str())) {
        PendingAction::Install => {
            let (v, bytes) = pending.unwrap();
            log::info!("instalez la pornire update-ul descărcat v{v}");
            clear_pending(app);
            crate::volume::restore_osd();
            if let Err(e) = remote.unwrap().install(bytes) {
                log::warn!("instalarea la pornire a eșuat: {e}");
            }
        }
        PendingAction::Discard => clear_pending(app),
        PendingAction::Keep => log::info!("update în așteptare, dar serverul nu răspunde; reîncerc mai târziu"),
        PendingAction::Nothing => {}
    }
}

/// Verificarea periodică (dacă e pornită din setări).
pub fn spawn(app: AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    std::thread::spawn(move || {
        let started = now_secs();
        let mut last: Option<u64> = None;
        loop {
            std::thread::sleep(Duration::from_secs(if last.is_none() { 5 } else { 60 }));
            if !check_due(started, last, now_secs()) {
                continue;
            }
            last = Some(now_secs());
            if app.state::<SettingsState>().get().auto_update {
                run_check(&app);
            }
        }
    });
}

#[tauri::command]
pub fn get_update_state(app: AppHandle) -> UpdateState {
    current_state(&app)
}

#[tauri::command]
pub fn check_updates_now(app: AppHandle) {
    std::thread::spawn(move || run_check(&app));
}

#[tauri::command]
pub fn apply_update(app: AppHandle) -> Result<(), String> {
    apply(&app)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descarcarea_are_timeout() {
        // o conexiune blocată nu poate ține insula la nesfârșit în „Actualizare la vX”
        assert!(NETWORK_TIMEOUT > Duration::ZERO && NETWORK_TIMEOUT <= Duration::from_secs(300));
    }

    fn ready(v: &str) -> UpdateState {
        UpdateState::Ready { version: v.into() }
    }

    #[test]
    fn flux_normal_verificare_descarcare_gata() {
        let s = next_state(&UpdateState::Idle, UpdEvent::CheckStarted);
        assert_eq!(s, UpdateState::Checking);
        let s = next_state(&s, UpdEvent::Found("0.3.0".into()));
        assert_eq!(s, UpdateState::Downloading { version: "0.3.0".into() });
        assert_eq!(next_state(&s, UpdEvent::Downloaded("0.3.0".into())), ready("0.3.0"));
        assert_eq!(next_state(&UpdateState::Checking, UpdEvent::NoUpdate(5)), UpdateState::UpToDate { checked_at: 5 });
        assert_eq!(
            next_state(&UpdateState::Checking, UpdEvent::Failed("rețea".into(), 7)),
            UpdateState::Error { msg: "rețea".into(), checked_at: 7 }
        );
    }

    #[test]
    fn update_gata_nu_se_pierde_la_verificari_ulterioare() {
        let r = ready("0.3.0");
        assert_eq!(next_state(&r, UpdEvent::CheckStarted), r);
        assert_eq!(next_state(&r, UpdEvent::NoUpdate(9)), r);
        assert_eq!(next_state(&r, UpdEvent::Failed("x".into(), 9)), r);
        assert_eq!(next_state(&r, UpdEvent::Found("0.3.0".into())), r);
        // o versiune și mai nouă se descarcă
        assert_eq!(next_state(&r, UpdEvent::Found("0.4.0".into())), UpdateState::Downloading { version: "0.4.0".into() });
    }

    #[test]
    fn decizia_de_la_pornire() {
        use PendingAction::*;
        assert_eq!(pending_action(None, "0.2.0", Some("0.3.0")), Nothing);
        assert_eq!(pending_action(Some("0.3.0"), "0.2.0", Some("0.3.0")), Install);
        assert_eq!(pending_action(Some("0.3.0"), "0.3.0", Some("0.3.0")), Discard); // deja instalată
        assert_eq!(pending_action(Some("0.2.5"), "0.3.0", None), Discard); // mai veche decât cea curentă
        assert_eq!(pending_action(Some("0.3.0"), "0.2.0", None), Keep); // fără rețea
        assert_eq!(pending_action(Some("0.3.0"), "0.2.0", Some("0.4.0")), Discard); // serverul are alta
    }

    #[test]
    fn comparare_versiuni() {
        assert!(newer("0.10.0", "0.9.9"));
        assert!(newer("1.0.0", "0.99.99"));
        assert!(!newer("0.2.0", "0.2.0"));
        assert!(!newer("ceva", "0.1.0"));
    }

    #[test]
    fn prima_verificare_la_30s_apoi_la_6h_dupa_ceas() {
        let start = 1_000_000;
        assert!(!check_due(start, None, start + 29));
        assert!(check_due(start, None, start + 30));
        let last = start + 30;
        assert!(!check_due(start, Some(last), last + 6 * 3600 - 1));
        assert!(check_due(start, Some(last), last + 6 * 3600));
        // laptopul a dormit 8 ore: ora reală a trecut, verificarea e datorată imediat
        assert!(check_due(start, Some(last), last + 8 * 3600));
        // ceasul dat înapoi: verificăm, nu așteptăm zile
        assert!(check_due(start, Some(last), last - 100));
    }

    #[test]
    fn descarcare_esuata_pastreaza_updateul_gata() {
        let s = next_state(&ready("0.3.0"), UpdEvent::Found("0.4.0".into()));
        assert_eq!(next_state(&s, after_failure(Some("0.3.0".into()), "rețea".into(), 5)), ready("0.3.0"));
        assert_eq!(
            next_state(&UpdateState::Checking, after_failure(None, "rețea".into(), 5)),
            UpdateState::Error { msg: "rețea".into(), checked_at: 5 }
        );
    }
}
