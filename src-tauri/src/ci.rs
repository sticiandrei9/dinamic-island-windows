//! CI în insulă: rulările GitHub Actions din repo-urile proprii, prin `gh` (procesul existent, autentificat;
//! aplicația nu citește tokenul). Cât `modules.ci` e pornit: la 30 s cât rulează ceva, altfel la 60 s.
//! Emite `ci` (rulările în curs, doar la schimbare) și `ci-done` (o rulare care s-a terminat).
//! Sursele sunt o listă (`Result<Vec<Run>, String>` fiecare), ca Cloudflare să se adauge fără a atinge restul.

use std::collections::VecDeque;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::settings::SettingsState;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Run {
    pub id: String,
    /// "github" (mai târziu și "cloudflare")
    pub source: String,
    pub project: String,
    pub name: String,
    /// "running" | "ok" | "failed"
    pub state: String,
    pub url: String,
    /// secunde Unix; doar la cele terminate
    pub finished_at: Option<u64>,
}

#[derive(Serialize)]
pub struct CiStatus {
    /// "ok" | "missing" | "unauthenticated" | "error"
    pub gh: &'static str,
    pub user: Option<String>,
    pub budget_paused: bool,
}

const HOUR: u64 = 3600;
const CAP: usize = 1000;
const REPOS_EVERY: u64 = 120;
const ACTIVE_DAYS: u64 = 7;
const GH_TIMEOUT: Duration = Duration::from_secs(15);

// ---------- logica pură ----------

/// Apelurile către `gh` din ultima oră: mai puțin de 1000 (plafonul din specificație).
#[derive(Default)]
pub struct Budget {
    calls: VecDeque<u64>,
}

impl Budget {
    pub const fn new() -> Self {
        Self { calls: VecDeque::new() }
    }
    pub fn record(&mut self, now: u64) {
        self.calls.push_back(now);
        while self.calls.front().is_some_and(|&t| now.saturating_sub(t) >= HOUR) {
            self.calls.pop_front();
        }
    }
    pub fn ok(&self, now: u64) -> bool {
        self.calls.iter().filter(|&&t| now.saturating_sub(t) < HOUR).count() < CAP
    }
}

/// „2026-10-17T11:36:08Z” → secunde Unix.
fn parse_iso(s: &str) -> Option<u64> {
    let n = |a: usize, b: usize| s.get(a..b)?.parse::<i64>().ok();
    if s.len() < 19 || s.as_bytes()[4] != b'-' || s.as_bytes()[10] != b'T' {
        return None;
    }
    let (mut y, m, d) = (n(0, 4)?, n(5, 7)?, n(8, 10)?);
    let (hh, mm, ss) = (n(11, 13)?, n(14, 16)?, n(17, 19)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // zile de la 1970-01-01 (algoritmul calendarului civil)
    if m <= 2 {
        y -= 1;
    }
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    u64::try_from(days * 86_400 + hh * 3600 + mm * 60 + ss).ok()
}

/// Răspunsul `/repos/{o}/{r}/actions/runs`: rulările, în forma noastră. Orice altceva (JSON stricat,
/// `{"message": "Bad credentials"}`) e eroare; textul serverului nu ajunge în mesaj.
pub fn parse_runs(json: &str, project: &str) -> Result<Vec<Run>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|_| "răspuns care nu e JSON".to_string())?;
    let list = v.get("workflow_runs").and_then(|l| l.as_array()).ok_or("răspuns fără workflow_runs")?;
    Ok(list
        .iter()
        .filter_map(|r| {
            let state = match (r.get("status")?.as_str()?, r.get("conclusion").and_then(|c| c.as_str())) {
                ("completed", Some("success")) => "ok",
                ("completed", _) => "failed",
                _ => "running", // queued, in_progress, waiting, requested, pending
            };
            Some(Run {
                id: r.get("id")?.as_u64()?.to_string(),
                source: "github".into(),
                project: project.into(),
                name: r.get("name")?.as_str()?.into(),
                state: state.into(),
                url: r.get("html_url")?.as_str()?.into(),
                finished_at: if state == "running" { None } else { r.get("updated_at").and_then(|u| u.as_str()).and_then(parse_iso) },
            })
        })
        .collect())
}

/// Ce s-a schimbat între două verificări: „started” (rulare nouă în curs), „ok” / „failed” (a trecut din
/// „running” în terminat). O rulare văzută prima dată deja terminată se anunță doar dacă s-a terminat
/// după `since` (pornirea modulului): așa un workflow scurt tot dă bula, dar la pornire nu vine istoricul.
pub fn transitions(prev: &[Run], cur: &[Run], since: u64) -> Vec<(Run, &'static str)> {
    cur.iter()
        .filter_map(|r| {
            let was = prev.iter().find(|p| p.source == r.source && p.id == r.id).map(|p| p.state == "running");
            let done = if r.state == "ok" { "ok" } else { "failed" };
            let kind = match (r.state == "running", was) {
                (true, None) => "started",
                (true, Some(_)) | (false, Some(false)) => return None,
                (false, Some(true)) => done,
                (false, None) => r.finished_at.filter(|&f| f > since).map(|_| done)?,
            };
            Some((r.clone(), kind))
        })
        .collect()
}

/// Numele complete (`owner/repo`) ale repo-urilor cu push în ultimele 7 zile, din `/user/repos`.
pub fn active_repos(json: &str, now: u64) -> Vec<String> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| {
            let pushed = parse_iso(r.get("pushed_at")?.as_str()?)?;
            (now.saturating_sub(pushed) <= ACTIVE_DAYS * 86_400).then(|| r.get("full_name")?.as_str().map(String::from))?
        })
        .collect()
}

/// Rulările ciclului: ale surselor reușite; o sursă care a eșuat își păstrează rulările de data trecută
/// (altfel o rulare ar dispărea și ar reveni „nouă”, cu un anunț în plus).
fn merge(prev: &[Run], fetched: Vec<(&str, Result<Vec<Run>, String>)>) -> Vec<Run> {
    let mut cur = Vec::new();
    for (source, res) in fetched {
        match res {
            Ok(runs) => cur.extend(runs),
            Err(e) => {
                log::warn!("ci: sursa {source} a eșuat: {e}");
                cur.extend(prev.iter().filter(|r| r.source == source).cloned());
            }
        }
    }
    cur
}

/// Din stderr-ul lui `gh`: neautentificat sau altă eroare. Textul nu se loghează.
fn classify_failure(stderr: &str) -> &'static str {
    let s = stderr.to_lowercase();
    if ["gh auth login", "http 401", "bad credentials", "authentication"].iter().any(|k| s.contains(k)) {
        "unauthenticated"
    } else {
        "error"
    }
}

// ---------- procesul `gh` ----------

fn now_s() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// `gh.exe` din PATH, altfel din instalarea implicită.
fn gh_path() -> Option<PathBuf> {
    let in_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|d| d.join("gh.exe"))
        .find(|p| p.is_file());
    in_path.or_else(|| {
        std::env::var_os("ProgramFiles").map(|d| PathBuf::from(d).join("GitHub CLI").join("gh.exe")).filter(|p| p.is_file())
    })
}

/// Pornește comanda fără fereastră, citește stdout/stderr (în fire, ca țeava să nu se umple) și o oprește
/// dacă depășește `timeout`. Ok((a ieșit cu succes, stdout, stderr)).
fn run_capture(mut cmd: Command, timeout: Duration) -> Result<(bool, String, String), &'static str> {
    use std::os::windows::process::CommandExt;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { "missing" } else { "error" })?;
    fn drain(mut r: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
        std::thread::spawn(move || {
            let mut b = Vec::new();
            let _ = r.read_to_end(&mut b);
            String::from_utf8_lossy(&b).into_owned()
        })
    }
    let out = drain(child.stdout.take().ok_or("error")?);
    let err = drain(child.stderr.take().ok_or("error")?);
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(30)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                log::warn!("ci: gh a depășit {} s și a fost oprit", timeout.as_secs());
                return Err("error");
            }
        }
    };
    Ok((status.success(), out.join().unwrap_or_default(), err.join().unwrap_or_default()))
}

/// `gh <args>`: stdout, sau tipul eșecului ("missing" | "unauthenticated" | "error").
fn run_gh(args: &[&str]) -> Result<String, &'static str> {
    let mut cmd = Command::new(gh_path().ok_or("missing")?);
    cmd.args(args).env("GH_PROMPT_DISABLED", "1").env("GH_NO_UPDATE_NOTIFIER", "1");
    let (ok, stdout, stderr) = run_capture(cmd, GH_TIMEOUT)?;
    if ok {
        Ok(stdout)
    } else {
        let kind = classify_failure(&stderr);
        log::warn!("ci: gh a eșuat ({kind})"); // stderr nu intră în log
        Err(kind)
    }
}

/// Plafonul de apeluri e comun buclei și Setărilor.
static BUDGET: Mutex<Budget> = Mutex::new(Budget::new());

/// Un apel numărat în buget.
fn call(args: &[&str]) -> Result<String, &'static str> {
    BUDGET.lock().unwrap().record(now_s());
    run_gh(args)
}

/// Contul autentificat (`gh api user`).
fn check_user() -> Result<String, &'static str> {
    let login = call(&["api", "user", "--jq", ".login"])?;
    Some(login.trim().to_string()).filter(|l| !l.is_empty()).ok_or("error")
}

#[tauri::command]
pub async fn ci_status() -> CiStatus {
    tauri::async_runtime::spawn_blocking(|| {
        let (gh, user) = match check_user() {
            Ok(u) => ("ok", Some(u)),
            Err(kind) => (kind, None),
        };
        CiStatus { gh, user, budget_paused: !BUDGET.lock().unwrap().ok(now_s()) }
    })
    .await
    .unwrap_or(CiStatus { gh: "error", user: None, budget_paused: false })
}

// ---------- bucla ----------

#[derive(Default)]
struct Poller {
    active: bool,
    since: u64,
    gh_ok: bool,
    paused: bool,
    prev: Vec<Run>,
    shown: Vec<Run>,
    repos: Vec<String>,
    repos_at: Option<u64>,
}

impl Poller {
    /// Un apel `gh` pentru sursa GitHub; pierderea lui `gh` / a autentificării oprește ciclurile până se reverifică.
    fn gh(&mut self, args: &[&str]) -> Result<String, String> {
        if !BUDGET.lock().unwrap().ok(now_s()) {
            return Err("plafonul de apeluri".into());
        }
        call(args).map_err(|kind| {
            if kind != "error" {
                self.gh_ok = false;
            }
            kind.to_string()
        })
    }

    /// Rulările recente din repo-urile proprii cu push în ultimele 7 zile.
    fn github(&mut self, now: u64) -> Result<Vec<Run>, String> {
        if self.repos_at.is_none_or(|t| now.saturating_sub(t) >= REPOS_EVERY) {
            let json = self.gh(&["api", "/user/repos?affiliation=owner&sort=pushed&per_page=10"])?;
            self.repos = active_repos(&json, now);
            self.repos_at = Some(now);
        }
        let (mut all, mut failed) = (Vec::new(), 0);
        for full in self.repos.clone() {
            let project = full.rsplit('/').next().unwrap_or(&full).to_string();
            match self.gh(&["api", &format!("/repos/{full}/actions/runs?per_page=5")]).and_then(|j| parse_runs(&j, &project)) {
                Ok(runs) => all.extend(runs),
                Err(_) => {
                    failed += 1;
                    all.extend(self.prev.iter().filter(|r| r.source == "github" && r.project == project).cloned());
                }
            }
        }
        if failed > 0 && failed == self.repos.len() {
            return Err("toate apelurile au eșuat".into());
        }
        Ok(all)
    }

    /// O verificare; întoarce secundele până la următoarea.
    fn cycle(&mut self, app: &AppHandle) -> u64 {
        let paused = !BUDGET.lock().unwrap().ok(now_s());
        if paused != self.paused {
            self.paused = paused;
            log::warn!("ci: {}", if paused { "plafon de 1000 apeluri/oră atins, verificările se opresc" } else { "verificările reiau" });
        }
        if paused {
            return 30;
        }
        if !self.gh_ok {
            match check_user() {
                Ok(_) => self.gh_ok = true,
                Err(kind) => {
                    log::info!("ci: gh indisponibil ({kind})");
                    return 60;
                }
            }
        }
        let now = now_s();
        let fetched = vec![("github", self.github(now))]; // aici se adaugă Cloudflare
        let cur = merge(&self.prev, fetched);
        for (run, kind) in transitions(&self.prev, &cur, self.since) {
            if kind == "started" {
                log::info!("ci: o rulare a pornit");
            } else {
                log::info!("ci: o rulare s-a terminat ({kind})");
                let _ = app.emit("ci-done", &run);
            }
        }
        let running: Vec<Run> = cur.iter().filter(|r| r.state == "running").cloned().collect();
        if running != self.shown {
            let _ = app.emit("ci", &running);
            self.shown = running;
        }
        self.prev = cur;
        if self.shown.is_empty() { 60 } else { 30 }
    }
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let enabled = |app: &AppHandle| app.state::<SettingsState>().get().modules.ci;
        let mut p = Poller::default();
        loop {
            if !enabled(&app) {
                if std::mem::take(&mut p.active) && !p.shown.is_empty() {
                    let _ = app.emit("ci", Vec::<Run>::new());
                }
                p = Poller::default();
                std::thread::sleep(Duration::from_secs(1));
                continue;
            }
            if !p.active {
                p = Poller { active: true, since: now_s(), ..Default::default() };
            }
            for _ in 0..p.cycle(&app) {
                if !enabled(&app) {
                    break;
                }
                std::thread::sleep(Duration::from_secs(1));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(id: &str, state: &str, finished_at: Option<u64>) -> Run {
        Run {
            id: id.into(),
            source: "github".into(),
            project: "p".into(),
            name: "Teste".into(),
            state: state.into(),
            url: format!("https://github.com/o/p/actions/runs/{id}"),
            finished_at,
        }
    }

    #[test]
    fn fara_rafala_la_pornire() {
        let old = run("1", "ok", Some(1000));
        assert!(transitions(&[], &[old], 5000).is_empty());
    }

    #[test]
    fn rulare_scurta_intre_verificari() {
        let r = run("2", "failed", Some(6000));
        assert_eq!(transitions(&[], &[r.clone()], 5000), vec![(r, "failed")]);
    }

    #[test]
    fn pornita_apoi_terminata() {
        let a = run("3", "running", None);
        let b = run("3", "ok", Some(7000));
        assert_eq!(transitions(&[], &[a.clone()], 5000), vec![(a.clone(), "started")]);
        assert_eq!(transitions(&[a], &[b.clone()], 5000), vec![(b, "ok")]);
    }

    #[test]
    fn terminata_o_data_nu_se_mai_anunta() {
        let b = run("4", "failed", Some(7000));
        assert!(transitions(&[b.clone()], &[b.clone()], 5000).is_empty());
        let a = run("4", "running", None);
        assert!(transitions(&[a.clone()], &[a], 5000).is_empty());
    }

    #[test]
    fn bugetul_de_apeluri() {
        let mut b = Budget::default();
        for i in 0..1000 {
            b.record(i);
        }
        assert!(!b.ok(1000));
        assert!(b.ok(3601 + 999));
    }

    #[test]
    fn gh_raspunsuri_ciudate() {
        assert!(parse_runs("nu e json", "x").is_err());
        assert_eq!(parse_runs(r#"{"workflow_runs":[]}"#, "x").unwrap(), vec![]);
        assert!(parse_runs(r#"{"message":"Bad credentials"}"#, "x").is_err());
    }

    #[test]
    fn raspuns_real_github() {
        let json = r#"{"total_count":2,"workflow_runs":[
            {"id":38582468834,"name":"Teste","status":"completed","conclusion":"failure","html_url":"https://github.com/o/p/actions/runs/38582468834","updated_at":"2026-10-14T14:44:21Z"},
            {"id":5,"name":"Build","status":"in_progress","conclusion":null,"html_url":"https://github.com/o/p/actions/runs/5","updated_at":"2026-10-14T14:45:00Z"},
            {"id":6,"name":"Deploy","status":"completed","conclusion":"success","html_url":"https://github.com/o/p/actions/runs/6","updated_at":"1970-01-01T00:01:00Z"}]}"#;
        let runs = parse_runs(json, "p").unwrap();
        assert_eq!(runs.len(), 3);
        assert_eq!((runs[0].id.as_str(), runs[0].state.as_str(), runs[0].finished_at), ("38582468834", "failed", Some(1_791_989_061)));
        assert_eq!((runs[1].state.as_str(), runs[1].finished_at), ("running", None));
        assert_eq!((runs[2].state.as_str(), runs[2].finished_at), ("ok", Some(60)));
        assert_eq!(runs[0].project, "p");
        assert_eq!(runs[0].source, "github");
    }

    #[test]
    fn data_iso() {
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso("2024-02-29T23:59:59Z"), Some(1_709_251_199));
        assert_eq!(parse_iso("2026-10-17T11:36:08Z"), Some(1_792_236_968));
        assert_eq!(parse_iso("ieri"), None);
        assert_eq!(parse_iso("2026-13-01T00:00:00Z"), None);
    }

    #[test]
    fn repo_uri_active_ultimele_7_zile() {
        let now = parse_iso("2026-10-17T12:00:00Z").unwrap();
        let json = r#"[
            {"full_name":"a/nou","pushed_at":"2026-10-17T11:36:08Z"},
            {"full_name":"a/sapt","pushed_at":"2026-10-10T12:00:00Z"},
            {"full_name":"a/vechi","pushed_at":"2026-10-09T11:59:59Z"},
            {"full_name":"a/fara-push","pushed_at":null}]"#;
        assert_eq!(active_repos(json, now), vec!["a/nou", "a/sapt"]);
        assert!(active_repos("nu e json", now).is_empty());
        assert!(active_repos(r#"{"message":"Bad credentials"}"#, now).is_empty());
    }

    #[test]
    fn sursa_care_esueaza_isi_pastreaza_rularile() {
        let a = run("7", "running", None);
        let mut other = run("8", "running", None);
        other.source = "cloudflare".into();
        let cur = merge(&[a.clone(), other.clone()], vec![("github", Err("x".into())), ("cloudflare", Ok(vec![]))]);
        assert_eq!(cur, vec![a]);
    }

    #[test]
    fn esecuri_gh() {
        assert_eq!(classify_failure("To get started with GitHub CLI, please run:  gh auth login"), "unauthenticated");
        assert_eq!(classify_failure("gh: Bad credentials (HTTP 401)"), "unauthenticated");
        assert_eq!(classify_failure("gh: Not Found (HTTP 404)"), "error");
        assert_eq!(classify_failure(""), "error");
    }

    #[test]
    fn procesul_se_opreste_la_timeout() {
        let mut quick = Command::new("cmd");
        quick.args(["/C", "echo salut"]);
        let (ok, out, _) = run_capture(quick, Duration::from_secs(10)).unwrap();
        assert!(ok && out.trim() == "salut");
        let mut slow = Command::new("ping");
        slow.args(["-n", "10", "127.0.0.1"]);
        let t = Instant::now();
        assert!(run_capture(slow, Duration::from_millis(400)).is_err());
        assert!(t.elapsed() < Duration::from_secs(5));
        assert_eq!(run_capture(Command::new("nu-exista-xyz"), Duration::from_secs(1)).unwrap_err(), "missing");
    }
}

#[cfg(test)]
mod live {
    /// Manual (`cargo test ci::live -- --ignored --nocapture`): rulările curente, citite ca de aplicație. Doar citire.
    #[test]
    #[ignore]
    fn rulari_reale() {
        println!("gh: {:?}", super::check_user().map(|u| u.len()));
        let repos = super::run_gh(&["api", "/user/repos?affiliation=owner&sort=pushed&per_page=10"]).unwrap();
        let active = super::active_repos(&repos, super::now_s());
        println!("repo-uri active: {}", active.len());
        for full in active {
            let project = full.rsplit('/').next().unwrap().to_string();
            let json = super::run_gh(&["api", &format!("/repos/{full}/actions/runs?per_page=5")]).unwrap();
            let runs = super::parse_runs(&json, &project).unwrap();
            println!("{project}: {} rulări", runs.len());
            for r in runs {
                println!("  {} | {} | {:?}", r.name, r.state, r.finished_at);
            }
        }
    }
}
