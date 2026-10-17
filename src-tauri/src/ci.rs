//! CI în insulă: rulările GitHub Actions din repo-urile proprii, prin `gh` (procesul existent, autentificat;
//! aplicația nu citește tokenul). Cât `modules.ci` e pornit: la 30 s cât rulează ceva, altfel la 60 s.
//! Emite `ci` (rulările în curs, doar la schimbare) și `ci-done` (o rulare care s-a terminat).
//! Sursele sunt o listă (`Result<Vec<Run>, String>` fiecare): GitHub și, cu token + ID de cont în Setări,
//! Cloudflare (deployment-urile Pages și build-urile Workers, prin API-ul REST cu `ureq`; tokenul nu intră în log).

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
    /// "github" | "cloudflare"
    pub source: String,
    pub project: String,
    pub name: String,
    /// "running" | "ok" | "failed"
    pub state: String,
    pub url: String,
    /// secunde Unix; doar la cele terminate
    pub finished_at: Option<u64>,
    /// o rulare repornită (re-run) păstrează `id`-ul și crește încercarea
    pub run_attempt: u64,
}

#[derive(Serialize)]
pub struct CiStatus {
    /// "ok" | "missing" | "unauthenticated" | "error"
    pub gh: &'static str,
    pub user: Option<String>,
    pub budget_paused: bool,
    /// Cloudflare are bugetul lui, oprit separat
    pub cf_budget_paused: bool,
}

const HOUR: u64 = 3600;
const CAP: usize = 1000;
const REPOS_EVERY: u64 = 120;
const ACTIVE_DAYS: u64 = 7;
/// O rulare `queued` mai nouă de atât e „în curs”; una mai veche probabil așteaptă un runner care nu vine.
const QUEUED_FRESH: u64 = 600;
/// Repo-urile fără rulare în curs se verifică la acest interval (cele cu rulare, la fiecare ciclu de 30 s).
/// Apeluri/oră: cel mult 10 repo-uri × 60 + 2 repo-uri cu rulare × 60 în plus + 30 de liste ≈ 750 < 1000.
const FULL_EVERY: u64 = 55;
const ANNOUNCED_KEEP: usize = 200;
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
    parse_runs_at(json, project, now_s())
}

/// Ca `parse_runs`, cu momentul curent dat (pentru teste). În listă intră doar rulările în curs (`in_progress`,
/// plus `queued` de sub 10 minute) și cele terminate; `waiting`, `requested`, `pending` și `queued` vechi se
/// anunță abia când se termină. `cancelled` și `skipped` nu dau nimic; `neutral` e ok.
pub fn parse_runs_at(json: &str, project: &str, now: u64) -> Result<Vec<Run>, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|_| "răspuns care nu e JSON".to_string())?;
    let list = v.get("workflow_runs").and_then(|l| l.as_array()).ok_or("răspuns fără workflow_runs")?;
    Ok(list
        .iter()
        .filter_map(|r| {
            let created = r.get("created_at").and_then(|c| c.as_str()).and_then(parse_iso);
            let state = match (r.get("status")?.as_str()?, r.get("conclusion").and_then(|c| c.as_str())) {
                ("completed", Some("success" | "neutral")) => "ok",
                ("completed", Some("cancelled" | "skipped")) => return None,
                ("completed", _) => "failed",
                ("in_progress", _) => "running",
                ("queued", _) if created.is_some_and(|c| now.saturating_sub(c) < QUEUED_FRESH) => "running",
                _ => return None,
            };
            Some(Run {
                id: r.get("id")?.as_u64()?.to_string(),
                source: "github".into(),
                project: project.into(),
                // `name` poate lipsi sau fi null: atunci titlul rulării
                name: r.get("name").and_then(|n| n.as_str()).or_else(|| r.get("display_title")?.as_str())?.into(),
                state: state.into(),
                url: r.get("html_url")?.as_str()?.into(),
                finished_at: if state == "running" { None } else { r.get("updated_at").and_then(|u| u.as_str()).and_then(parse_iso) },
                run_attempt: r.get("run_attempt").and_then(|a| a.as_u64()).unwrap_or(1),
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
            // o altă încercare a aceleiași rulări (re-run) e o rulare nouă
            let was = prev.iter().find(|p| p.source == r.source && p.id == r.id && p.run_attempt == r.run_attempt).map(|p| p.state == "running");
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

/// Doar ce nu s-a anunțat încă: o rulare nu se anunță de două ori (nici dacă reintră în listă, de pildă un
/// repo care redevine activ); ține minte ultimele `ANNOUNCED_KEEP`. „started” nu e anunț, trece mereu.
fn first_time(announced: &mut VecDeque<String>, found: Vec<(Run, &'static str)>) -> Vec<(Run, &'static str)> {
    let mut out = Vec::new();
    for (run, kind) in found {
        let key = format!("{}:{}:{}", run.source, run.id, run.run_attempt);
        if kind != "started" {
            if announced.contains(&key) {
                continue;
            }
            announced.push_back(key);
            if announced.len() > ANNOUNCED_KEEP {
                announced.pop_front();
            }
        }
        out.push((run, kind));
    }
    out
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

// ---------- Cloudflare: logica pură ----------

/// Paginile din panou, fără ID-ul contului (`?to=/:account/…` îl completează panoul singur).
const CF_DASH: &str = "https://dash.cloudflare.com/?to=/:account";
/// Cel mult atâtea proiecte Pages și atâtea Workers urmărite (apeluri/oră: fiecare costă ~60).
const CF_MAX: usize = 3;

/// O rulare în coadă sau inactivă de mai mult de atât probabil nu mai pornește: nu mai e „în curs”.
const CF_STUCK: u64 = 1800;
/// Un proiect fără build în ultima oră se verifică doar la atât.
const CF_IDLE_EVERY: u64 = 120;

/// `result` din răspuns; `success: false` sau alt corp e eroare `bad_response` (codurile se traduc în Setări);
/// textul serverului nu ajunge în mesaj.
fn cf_list(json: &str) -> Result<Vec<serde_json::Value>, String> {
    let bad = || "bad_response".to_string();
    let v: serde_json::Value = serde_json::from_str(json).map_err(|_| bad())?;
    if v.get("success").and_then(|s| s.as_bool()) == Some(false) {
        return Err(bad());
    }
    v.get("result").and_then(|r| r.as_array()).cloned().ok_or_else(bad)
}

fn cf_time(v: &serde_json::Value, key: &str) -> Option<u64> {
    v.get(key)?.as_str().and_then(parse_iso)
}

/// Numele intră în adrese: doar litere, cifre, `-` și `_`.
fn cf_safe(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Deployment-urile unui proiect Pages (`result[]` din `…/pages/projects/{p}/deployments`). În curs = orice
/// etapă care nu e încheiată (`deploy` + `success`); `failure` = eșuat; `canceled`, `skipped` și cele sărite nu dau nimic;
/// unul în coadă / inactiv de peste 30 de minute nu mai e „în curs”.
pub fn parse_pages(json: &str, project: &str) -> Result<Vec<Run>, String> {
    parse_pages_at(json, project, now_s())
}

/// Ca `parse_pages`, cu momentul curent dat (pentru teste).
pub fn parse_pages_at(json: &str, project: &str, now: u64) -> Result<Vec<Run>, String> {
    Ok(cf_list(json)?
        .iter()
        .filter_map(|d| {
            if d.get("is_skipped").and_then(|s| s.as_bool()) == Some(true) {
                return None;
            }
            let id = d.get("id")?.as_str()?;
            let stage = d.get("latest_stage")?;
            let (name, status) = (stage.get("name")?.as_str()?, stage.get("status")?.as_str()?);
            let state = match (name, status) {
                (_, "canceled" | "skipped") => return None,
                (_, "failure") => "failed",
                ("deploy", "success") => "ok",
                ("queued", _) | (_, "idle") if cf_time(d, "created_on").is_some_and(|c| now.saturating_sub(c) > CF_STUCK) => return None,
                _ => "running",
            };
            // reușit: site-ul; altfel jurnalul build-ului din panou
            let site = d.get("url").and_then(|u| u.as_str()).filter(|u| state == "ok" && u.starts_with("https://"));
            Some(Run {
                id: id.into(),
                source: "cloudflare".into(),
                project: project.into(),
                name: d.pointer("/deployment_trigger/metadata/branch").and_then(|b| b.as_str()).or_else(|| d.get("environment")?.as_str()).unwrap_or("Pages").into(),
                state: state.into(),
                url: site.map(String::from).unwrap_or_else(|| format!("{CF_DASH}/pages/view/{project}/{id}")),
                finished_at: if state == "running" { None } else { cf_time(d, "modified_on") },
                run_attempt: 1,
            })
        })
        .collect())
}

/// Build-urile unui Worker (`result[]` din `…/builds/workers/{tag}/builds`). În curs = `queued`, `initializing`,
/// `running`; `stopped` e reușit (`success`) sau eșuat (`fail`, `terminated`); `skipped` și `cancelled` nu dau nimic;
/// unul `queued` de peste 30 de minute nu mai e „în curs”.
pub fn parse_worker_builds(json: &str, project: &str) -> Result<Vec<Run>, String> {
    parse_worker_builds_at(json, project, now_s())
}

/// Ca `parse_worker_builds`, cu momentul curent dat (pentru teste).
pub fn parse_worker_builds_at(json: &str, project: &str, now: u64) -> Result<Vec<Run>, String> {
    Ok(cf_list(json)?
        .iter()
        .filter_map(|b| {
            let id = b.get("build_uuid")?.as_str()?;
            let state = match (b.get("status")?.as_str()?, b.get("build_outcome").and_then(|o| o.as_str())) {
                ("queued", _) if cf_time(b, "created_on").is_some_and(|c| now.saturating_sub(c) > CF_STUCK) => return None,
                ("queued" | "initializing" | "running", _) => "running",
                ("stopped", Some("success")) => "ok",
                ("stopped", Some("fail" | "terminated")) => "failed",
                _ => return None,
            };
            Some(Run {
                id: id.into(),
                source: "cloudflare".into(),
                project: project.into(),
                name: b.pointer("/build_trigger_metadata/branch").and_then(|n| n.as_str()).unwrap_or("build").into(),
                state: state.into(),
                url: format!("{CF_DASH}/workers/services/view/{project}/production/builds/{id}"),
                finished_at: if state == "running" { None } else { cf_time(b, "stopped_on").or_else(|| cf_time(b, "modified_on")) },
                run_attempt: 1,
            })
        })
        .collect())
}

/// Proiectele Pages (din `…/pages/projects`) cu ultimul deployment din ultimele 7 zile, cele mai noi întâi.
fn cf_active_pages(json: &str, now: u64) -> Vec<String> {
    let mut found: Vec<(u64, String)> = cf_list(json)
        .unwrap_or_default()
        .iter()
        .filter_map(|p| {
            let name = p.get("name")?.as_str().filter(|n| cf_safe(n))?;
            let last = p.get("latest_deployment")?;
            let at = cf_time(last, "modified_on").or_else(|| cf_time(last, "created_on"))?;
            (now.saturating_sub(at) <= ACTIVE_DAYS * 86_400).then(|| (at, name.to_string()))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().take(CF_MAX).map(|(_, n)| n).collect()
}

/// Workers (din `…/workers/scripts`): (nume, tag) ale celor mai recent modificate. Fără limită de vârstă:
/// un build care eșuează nu modifică scriptul, deci vârsta nu arată dacă are build-uri.
fn cf_recent_workers(json: &str) -> Vec<(String, String)> {
    let mut found: Vec<(u64, String, String)> = cf_list(json)
        .unwrap_or_default()
        .iter()
        .filter_map(|w| {
            let name = w.get("id")?.as_str().filter(|n| cf_safe(n))?;
            let tag = w.get("tag")?.as_str().filter(|n| cf_safe(n))?;
            Some((cf_time(w, "modified_on").unwrap_or(0), name.to_string(), tag.to_string()))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().take(CF_MAX).map(|(_, n, t)| (n, t)).collect()
}

/// Codul unei erori HTTP de la Cloudflare, tradus în Setări (`unauthorized`, `forbidden:<dreptul>`,
/// `account_not_found`, `rate_limited`, `http:<cod>`); `need` = dreptul de citire necesar apelului.
/// Fără token, fără adrese, fără textul serverului.
fn cf_error(code: u16, need: &str) -> String {
    match code {
        401 => "unauthorized".into(),
        403 => format!("forbidden:{need}"),
        404 => "account_not_found".into(),
        429 => "rate_limited".into(),
        _ => format!("http:{code}"),
    }
}

/// Când se verifică din nou un proiect: la fiecare ciclu cât are ceva în curs, la ~55 s dacă a avut un build
/// în ultima oră, altfel la 120 s.
fn cf_due(last: &[Run], polled_at: Option<u64>, now: u64) -> bool {
    let Some(at) = polled_at else { return true };
    let gap = if last.iter().any(|r| r.state == "running") {
        0
    } else if last.iter().any(|r| r.finished_at.is_some_and(|f| now.saturating_sub(f) < HOUR)) {
        FULL_EVERY
    } else {
        CF_IDLE_EVERY
    };
    now.saturating_sub(at) >= gap
}

/// (GitHub oprit de plafon, Cloudflare oprit de plafon): fiecare sursă are bugetul ei.
fn paused_flags(gh: &Budget, cf: &Budget, now: u64) -> (bool, bool) {
    (!gh.ok(now), !cf.ok(now))
}

/// Cât așteptăm până reîncercăm `gh` după o eșuare: lipsa lui nu se schimbă de la sine (10 min), restul se reîncearcă la ciclu.
fn gh_retry_after(kind: &str) -> u64 {
    if kind == "missing" { 600 } else { 0 }
}

/// Cloudflare merge doar cu ambele câmpuri; ID-ul contului intră în adresă, deci doar litere și cifre.
fn cf_ready(token: &str, account: &str) -> bool {
    !token.trim().is_empty() && !account.is_empty() && account.chars().all(|c| c.is_ascii_alphanumeric())
}

// ---------- procesul `gh` ----------

fn now_s() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// Locurile instalării implicite: pe mașină (`ProgramFiles`) și pe utilizator (MSI per-user în `LocalAppData`).
fn gh_install_candidates(program_files: Option<PathBuf>, local_app_data: Option<PathBuf>) -> Vec<PathBuf> {
    let gh = |d: PathBuf| d.join("GitHub CLI").join("gh.exe");
    program_files.map(gh).into_iter().chain(local_app_data.map(|d| gh(d.join("Programs")))).collect()
}

/// `gh.exe` din PATH, altfel din instalarea implicită.
fn gh_path() -> Option<PathBuf> {
    let in_path = std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .map(|d| d.join("gh.exe"))
        .find(|p| p.is_file());
    in_path.or_else(|| {
        let env = |k| std::env::var_os(k).map(PathBuf::from);
        gh_install_candidates(env("ProgramFiles"), env("LOCALAPPDATA")).into_iter().find(|p| p.is_file())
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

/// Plafonul de apeluri e comun buclei și Setărilor; GitHub și Cloudflare au câte unul.
static BUDGET: Mutex<Budget> = Mutex::new(Budget::new());
static CF_BUDGET: Mutex<Budget> = Mutex::new(Budget::new());

/// Un apel numărat în buget.
fn call(args: &[&str]) -> Result<String, &'static str> {
    BUDGET.lock().unwrap().record(now_s());
    run_gh(args)
}

/// Contul autentificat, ținut minte: Setările îl iau de aici, nu mai întreabă `gh` la fiecare afișare.
static LOGIN: Mutex<Option<String>> = Mutex::new(None);
/// Rulările în curs, ca `ci_current` să le dea unei insule care a pierdut evenimentul `ci`.
static CURRENT: Mutex<Vec<Run>> = Mutex::new(Vec::new());

/// Contul autentificat (`gh api user`); reușita se ține minte în `LOGIN`.
fn check_user() -> Result<String, &'static str> {
    let login = call(&["api", "user", "--jq", ".login"])?;
    let login = Some(login.trim().to_string()).filter(|l| !l.is_empty()).ok_or("error")?;
    *LOGIN.lock().unwrap() = Some(login.clone());
    Ok(login)
}

/// Starea pentru Setări. La plafon nu se cheamă `gh`; altfel `gh` se cheamă doar cât bucla n-a aflat încă contul.
pub fn status_now() -> CiStatus {
    let known = LOGIN.lock().unwrap().clone();
    let (budget_paused, cf_budget_paused) = paused_flags(&BUDGET.lock().unwrap(), &CF_BUDGET.lock().unwrap(), now_s());
    if budget_paused {
        return CiStatus { gh: if known.is_some() { "ok" } else { "error" }, user: known, budget_paused, cf_budget_paused };
    }
    let (gh, user) = match known.map(Ok).unwrap_or_else(check_user) {
        Ok(u) => ("ok", Some(u)),
        Err(kind) => (kind, None),
    };
    CiStatus { gh, user, budget_paused, cf_budget_paused }
}

#[tauri::command]
pub async fn ci_status() -> CiStatus {
    tauri::async_runtime::spawn_blocking(status_now).await.unwrap_or(CiStatus { gh: "error", user: None, budget_paused: false, cf_budget_paused: false })
}

#[tauri::command]
pub fn ci_current() -> Vec<Run> {
    CURRENT.lock().unwrap().clone()
}

// ---------- Cloudflare: HTTP ----------

const CF_API: &str = "https://api.cloudflare.com/client/v4";
const CF_TIMEOUT: Duration = Duration::from_secs(15);
const PAGES_READ: &str = "Cloudflare Pages Read";
const SCRIPTS_READ: &str = "Workers Scripts Read";
const CI_READ: &str = "Workers CI Read";

/// Tokenul și ID-ul contului, din Setări (tăiate); doar dacă ambele sunt completate.
struct Cf {
    token: String,
    account: String,
}

impl Cf {
    fn from_settings(app: &AppHandle) -> Option<Cf> {
        let s = app.state::<SettingsState>().get();
        let cf = Cf { token: s.cloudflare_token.trim().into(), account: s.cloudflare_account.trim().into() };
        cf_ready(&cf.token, &cf.account).then_some(cf)
    }

    /// Calea unei resurse a contului.
    fn acct(&self, tail: &str) -> String {
        format!("/accounts/{}{tail}", self.account)
    }
}

/// `GET` către Cloudflare (numărat în bugetul Cloudflare). Erorile sunt coduri scurte pe care Setările le traduc:
/// nici tokenul, nici adresa, nici textul serverului (vezi `cf_error`; în plus `budget`, `network`, `bad_response`).
fn cf_get(cf: &Cf, path: &str, need: &str) -> Result<String, String> {
    {
        let mut b = CF_BUDGET.lock().unwrap();
        if !b.ok(now_s()) {
            return Err("budget".into());
        }
        b.record(now_s());
    }
    match ureq::get(&format!("{CF_API}{path}")).set("Authorization", &format!("Bearer {}", cf.token)).timeout(CF_TIMEOUT).call() {
        Ok(r) => r.into_string().map_err(|_| "bad_response".to_string()),
        Err(ureq::Error::Status(code, _)) => {
            log::warn!("ci: Cloudflare a răspuns {code}");
            Err(cf_error(code, need))
        }
        Err(ureq::Error::Transport(_)) => Err("network".into()),
    }
}

/// „Verifică” din Setări: tokenul (de cont, apoi de utilizator), apoi cele trei citiri de care are nevoie bucla.
/// Ok = ce a găsit; Err = un cod de eroare. Un Worker fără Workers Builds (404) nu e o eroare a tokenului.
fn check_cloudflare(cf: &Cf) -> Result<String, String> {
    cf_get(cf, &cf.acct("/tokens/verify"), "API Tokens").or_else(|_| cf_get(cf, "/user/tokens/verify", "API Tokens"))?;
    let pages = cf_list(&cf_get(cf, &cf.acct("/pages/projects?per_page=100"), PAGES_READ)?)?.len();
    let scripts = cf_list(&cf_get(cf, &cf.acct("/workers/scripts"), SCRIPTS_READ)?)?;
    if let Some(tag) = scripts.iter().find_map(|w| w.get("tag")?.as_str().filter(|t| cf_safe(t))) {
        match cf_get(cf, &cf.acct(&format!("/builds/workers/{tag}/builds?per_page=1")), CI_READ) {
            Err(e) if e == "account_not_found" => {}
            other => {
                other?;
            }
        }
    }
    Ok(format!("Pages: {pages} · Workers: {}", scripts.len()))
}

#[tauri::command]
pub async fn ci_check_cloudflare(app: AppHandle) -> Result<String, String> {
    let cf = Cf::from_settings(&app).ok_or("not_configured")?;
    tauri::async_runtime::spawn_blocking(move || check_cloudflare(&cf)).await.map_err(|_| "internal".to_string())?
}

// ---------- bucla ----------

/// Un proiect Pages sau un Worker urmărit, cu rulările de la ultima verificare.
#[derive(Clone)]
struct CfTarget {
    worker: bool,
    /// Pages: numele proiectului; Worker: tag-ul (API-ul Builds cere tag-ul, nu numele)
    key: String,
    project: String,
    last: Vec<Run>,
    polled_at: Option<u64>,
    /// lista de build-uri a răspuns cel puțin o dată
    seen: bool,
    /// un Worker fără Workers Builds (404 la prima listare): sărit până la următoarea reîmprospătare a listei
    unconfigured: bool,
}

#[derive(Default)]
struct Poller {
    active: bool,
    since: u64,
    gh_ok: bool,
    paused: bool,
    cf_was_paused: bool,
    /// `gh` lipsește: nu-l mai căutăm la fiecare ciclu
    gh_retry_at: u64,
    prev: Vec<Run>,
    shown: Vec<Run>,
    announced: VecDeque<String>,
    repos: Vec<String>,
    repos_at: Option<u64>,
    /// ultima verificare a tuturor repo-urilor
    full_at: Option<u64>,
    /// contul pentru care sunt `cf_targets` (altul în Setări → lista se reface)
    cf_for: String,
    cf_targets: Vec<CfTarget>,
    cf_at: Option<u64>,
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
                *LOGIN.lock().unwrap() = None;
            }
            kind.to_string()
        })
    }

    /// Rulările recente din repo-urile proprii cu push în ultimele 7 zile. Repo-urile cu o rulare în curs se
    /// verifică la fiecare ciclu (30 s), restul cel mult la 55 s; ale celor sărite rămân rulările de data trecută.
    fn github(&mut self, now: u64) -> Result<Vec<Run>, String> {
        if self.repos_at.is_none_or(|t| now.saturating_sub(t) >= REPOS_EVERY) {
            // lista veche rămâne dacă apelul eșuează (se reîncearcă la ciclul următor)
            match self.gh(&["api", "/user/repos?affiliation=owner&sort=pushed&per_page=10"]) {
                Ok(json) => {
                    self.repos = active_repos(&json, now);
                    self.repos_at = Some(now);
                }
                Err(e) if self.repos.is_empty() => return Err(e),
                Err(_) => {}
            }
            if !self.gh_ok {
                return Err("gh indisponibil".into()); // pierdut sau neautentificat: nu mai încercăm repo-urile
            }
        }
        let full = self.full_at.is_none_or(|t| now.saturating_sub(t) >= FULL_EVERY);
        let (mut all, mut polled, mut failed) = (Vec::new(), 0, 0);
        for full_name in self.repos.clone() {
            let project = full_name.rsplit('/').next().unwrap_or(&full_name).to_string();
            let before: Vec<Run> = self.prev.iter().filter(|r| r.source == "github" && r.project == project).cloned().collect();
            if !full && !before.iter().any(|r| r.state == "running") {
                all.extend(before);
                continue;
            }
            if !self.gh_ok {
                break;
            }
            polled += 1;
            match self.gh(&["api", &format!("/repos/{full_name}/actions/runs?per_page=5")]).and_then(|j| parse_runs(&j, &project)) {
                Ok(runs) => all.extend(runs),
                Err(_) => {
                    failed += 1;
                    all.extend(before);
                }
            }
        }
        if !self.gh_ok {
            return Err("gh indisponibil".into());
        }
        if full {
            self.full_at = Some(now);
        }
        if polled > 0 && failed == polled {
            return Err("toate apelurile au eșuat".into());
        }
        Ok(all)
    }

    /// Build-urile Pages și Workers: cele cu ceva în curs la fiecare ciclu (30 s), cele cu un build în ultima oră
    /// la ~55 s, restul la 120 s (`cf_due`); lista proiectelor se reface la 2 min (și după o eșuare, nu la fiecare ciclu).
    fn cloudflare(&mut self, now: u64, cf: &Cf) -> Result<Vec<Run>, String> {
        if self.cf_for != cf.account {
            self.cf_for = cf.account.clone();
            self.cf_targets.clear();
            self.cf_at = None;
        }
        if self.cf_at.is_none_or(|t| now.saturating_sub(t) >= REPOS_EVERY) {
            self.cf_at = Some(now);
            let pages = cf_get(cf, &cf.acct("/pages/projects?per_page=100"), PAGES_READ).map(|j| cf_active_pages(&j, now));
            let workers = cf_get(cf, &cf.acct("/workers/scripts"), SCRIPTS_READ).map(|j| cf_recent_workers(&j));
            if let (Err(e), Err(_)) = (&pages, &workers) {
                if self.cf_targets.is_empty() {
                    return Err(e.clone());
                }
            }
            // (worker?, [(key, project)]); o listă care a eșuat își păstrează proiectele de data trecută
            let found = [
                (false, pages.map(|v| v.into_iter().map(|n| (n.clone(), n)).collect::<Vec<_>>())),
                (true, workers.map(|v| v.into_iter().map(|(name, tag)| (tag, name)).collect())),
            ];
            let old = std::mem::take(&mut self.cf_targets);
            for (worker, list) in found {
                match list {
                    Ok(list) => self.cf_targets.extend(list.into_iter().map(|(key, project)| {
                        // ce se știe despre proiect rămâne; un Worker marcat „fără Builds” e reîncercat
                        let (last, polled_at, seen) = old.iter().find(|t| t.worker == worker && t.key == key).map(|t| (t.last.clone(), t.polled_at, t.seen)).unwrap_or_default();
                        CfTarget { worker, key, project, last, polled_at, seen, unconfigured: false }
                    })),
                    Err(_) => self.cf_targets.extend(old.iter().filter(|t| t.worker == worker).cloned().map(|t| CfTarget { unconfigured: false, ..t })),
                }
            }
        }
        let (mut all, mut polled, mut failed, mut last_err) = (Vec::new(), 0, 0, String::new());
        for t in &mut self.cf_targets {
            if t.unconfigured || !cf_due(&t.last, t.polled_at, now) {
                all.extend(t.last.clone());
                continue;
            }
            t.polled_at = Some(now);
            polled += 1;
            let (path, need) = if t.worker {
                (cf.acct(&format!("/builds/workers/{}/builds?per_page=5", t.key)), CI_READ)
            } else {
                (cf.acct(&format!("/pages/projects/{}/deployments?per_page=5", t.key)), PAGES_READ)
            };
            let res = cf_get(cf, &path, need).and_then(|j| if t.worker { parse_worker_builds(&j, &t.project) } else { parse_pages(&j, &t.project) });
            match res {
                Ok(runs) => {
                    t.seen = true;
                    t.last = runs.clone();
                    all.extend(runs);
                }
                // 404 la prima listare a unui Worker: nu are Workers Builds, nu mai e întrebat până la reîmprospătarea listei
                Err(e) if t.worker && !t.seen && e == "account_not_found" => {
                    t.unconfigured = true;
                    polled -= 1;
                    log::info!("ci: un Worker fără Workers Builds (builds_not_configured)");
                }
                Err(e) => {
                    failed += 1;
                    last_err = e;
                    all.extend(t.last.clone());
                }
            }
        }
        if polled > 0 && failed == polled {
            return Err(last_err);
        }
        Ok(all)
    }

    /// O verificare; întoarce secundele până la următoarea.
    fn cycle(&mut self, app: &AppHandle) -> u64 {
        let (paused, cf_paused) = paused_flags(&BUDGET.lock().unwrap(), &CF_BUDGET.lock().unwrap(), now_s());
        for (flag, was, name) in [(paused, &mut self.paused, "GitHub"), (cf_paused, &mut self.cf_was_paused, "Cloudflare")] {
            if flag != *was {
                *was = flag;
                log::warn!("ci: {name}: {}", if flag { "plafon de 1000 apeluri/oră atins, verificările se opresc" } else { "verificările reiau" });
            }
        }
        let cf_cfg = Cf::from_settings(app);
        let cf = cf_cfg.as_ref().filter(|_| !cf_paused);
        if paused && cf.is_none() {
            return 30;
        }
        let now = now_s();
        if !paused && !self.gh_ok && now >= self.gh_retry_at {
            match check_user() {
                Ok(_) => self.gh_ok = true,
                Err(kind) => {
                    self.gh_retry_at = now + gh_retry_after(kind);
                    log::info!("ci: gh indisponibil ({kind})");
                }
            }
        }
        let gh_live = self.gh_ok && !paused;
        if !gh_live && cf.is_none() {
            return 60;
        }
        // o sursă oprită (plafon, fără gh) își păstrează rulările de data trecută; Cloudflare merge și fără gh
        let kept = |prev: &[Run], source: &str| -> Result<Vec<Run>, String> { Ok(prev.iter().filter(|r| r.source == source).cloned().collect()) };
        let github = if gh_live { self.github(now) } else { kept(&self.prev, "github") };
        let mut fetched = vec![("github", github)];
        if let Some(cf) = cf {
            fetched.push(("cloudflare", self.cloudflare(now, cf)));
        } else if cf_cfg.is_some() {
            fetched.push(("cloudflare", kept(&self.prev, "cloudflare")));
        }
        let cur = merge(&self.prev, fetched);
        for (run, kind) in first_time(&mut self.announced, transitions(&self.prev, &cur, self.since)) {
            if kind == "started" {
                log::info!("ci: o rulare a pornit");
            } else {
                log::info!("ci: o rulare s-a terminat ({kind})");
                let _ = app.emit("ci-done", &run);
            }
        }
        let running: Vec<Run> = cur.iter().filter(|r| r.state == "running").cloned().collect();
        if running != self.shown {
            *CURRENT.lock().unwrap() = running.clone();
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
                *LOGIN.lock().unwrap() = None;
                if std::mem::take(&mut p.active) && !p.shown.is_empty() {
                    CURRENT.lock().unwrap().clear();
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

    #[test]
    fn gh_se_cauta_si_in_instalarea_per_utilizator() {
        let c = gh_install_candidates(Some(r"C:\Program Files".into()), Some(r"C:\Users\A\AppData\Local".into()));
        assert_eq!(c.len(), 2);
        assert!(c[0].ends_with(r"GitHub CLI\gh.exe"));
        assert!(c[1].ends_with(r"Programs\GitHub CLI\gh.exe"));
        assert!(gh_install_candidates(None, None).is_empty());
    }

    fn run(id: &str, state: &str, finished_at: Option<u64>) -> Run {
        Run {
            id: id.into(),
            source: "github".into(),
            project: "p".into(),
            name: "Teste".into(),
            state: state.into(),
            url: format!("https://github.com/o/p/actions/runs/{id}"),
            finished_at,
            run_attempt: 1,
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

    /// O rulare GitHub minimală, cu câmpurile date.
    fn gh_run(id: u32, fields: &str) -> String {
        format!(r#"{{"id":{id},"html_url":"https://github.com/o/p/actions/runs/{id}","updated_at":"2026-10-14T14:44:21Z",{fields}}}"#)
    }

    fn parsed(runs: &[String], now: u64) -> Vec<(String, String)> {
        let json = format!(r#"{{"workflow_runs":[{}]}}"#, runs.join(","));
        parse_runs_at(&json, "p", now).unwrap().into_iter().map(|r| (r.id, r.state)).collect()
    }

    #[test]
    fn concluzii_cancelled_skipped_neutral() {
        let done = |id, c: &str| gh_run(id, &format!(r#""name":"W","status":"completed","conclusion":"{c}""#));
        let all = [done(1, "success"), done(2, "neutral"), done(3, "cancelled"), done(4, "skipped"), done(5, "failure"), done(6, "timed_out"), done(7, "action_required")];
        let got = parsed(&all, 0);
        let s = |id: &str, st: &str| (id.to_string(), st.to_string());
        assert_eq!(got, vec![s("1", "ok"), s("2", "ok"), s("5", "failed"), s("6", "failed"), s("7", "failed")]);
    }

    #[test]
    fn doar_in_curs_si_queued_proaspat_au_inel() {
        let now = parse_iso("2026-10-14T15:00:00Z").unwrap();
        let st = |id, status: &str, created: &str| gh_run(id, &format!(r#""name":"W","status":"{status}","conclusion":null,"created_at":"{created}""#));
        let got = parsed(
            &[
                st(1, "in_progress", "2026-10-14T10:00:00Z"),
                st(2, "queued", "2026-10-14T14:55:00Z"),
                st(3, "queued", "2026-10-14T14:40:00Z"),
                st(4, "waiting", "2026-10-14T14:59:00Z"),
                st(5, "requested", "2026-10-14T14:59:00Z"),
                st(6, "pending", "2026-10-14T14:59:00Z"),
            ],
            now,
        );
        assert_eq!(got, vec![("1".into(), "running".into()), ("2".into(), "running".into())]);
    }

    #[test]
    fn numele_cade_pe_titlul_rularii() {
        let w = |extra: &str| gh_run(1, &format!(r#""status":"completed","conclusion":"success",{extra}"#));
        let name = |r: String| parse_runs_at(&format!(r#"{{"workflow_runs":[{r}]}}"#), "p", 0).unwrap().first().map(|r| r.name.clone());
        assert_eq!(name(w(r#""name":null,"display_title":"Fix #3""#)), Some("Fix #3".into()));
        assert_eq!(name(w(r#""display_title":"Fix #3""#)), Some("Fix #3".into()));
        assert_eq!(name(w(r#""name":"CI","display_title":"Fix #3""#)), Some("CI".into()));
        assert_eq!(name(w(r#""name":null"#)), None);
    }

    #[test]
    fn o_rulare_nu_se_anunta_de_doua_ori() {
        let mut seen = VecDeque::new();
        let r = run("9", "ok", Some(9000));
        let first = first_time(&mut seen, vec![(r.clone(), "ok")]);
        assert_eq!(first.len(), 1);
        // a ieșit din listă și a reintrat (alt ciclu): aceeași rulare, același anunț
        assert!(first_time(&mut seen, vec![(r.clone(), "ok")]).is_empty());
        // „started” nu e anunț și nu consumă locul
        assert_eq!(first_time(&mut seen, vec![(run("10", "running", None), "started")]).len(), 1);
        // memoria e mărginită: după 200 de rulări noi, prima se poate anunța iar
        for i in 0..ANNOUNCED_KEEP {
            first_time(&mut seen, vec![(run(&format!("x{i}"), "ok", Some(9000)), "ok")]);
        }
        assert!(seen.len() <= ANNOUNCED_KEEP);
        assert_eq!(first_time(&mut seen, vec![(r, "ok")]).len(), 1);
    }

    #[test]
    fn re_run_cu_acelasi_id_se_anunta_din_nou() {
        let first = run("11", "failed", Some(7000));
        let mut again = run("11", "running", None);
        again.run_attempt = 2;
        let mut finished = run("11", "ok", Some(9000));
        finished.run_attempt = 2;
        // încercarea 2 pornește (același id), apoi se termină
        assert_eq!(transitions(&[first.clone()], &[again.clone()], 5000), vec![(again.clone(), "started")]);
        assert_eq!(transitions(&[again], &[finished.clone()], 5000), vec![(finished.clone(), "ok")]);
        // terminată între două verificări: tot se anunță
        assert_eq!(transitions(&[first.clone()], &[finished.clone()], 5000), vec![(finished.clone(), "ok")]);
        // ambele încercări se anunță o dată fiecare, nu de două ori
        let mut seen = VecDeque::new();
        assert_eq!(first_time(&mut seen, vec![(first.clone(), "failed")]).len(), 1);
        assert_eq!(first_time(&mut seen, vec![(finished.clone(), "ok")]).len(), 1);
        assert!(first_time(&mut seen, vec![(first, "failed")]).is_empty());
        assert!(first_time(&mut seen, vec![(finished, "ok")]).is_empty());
        // încercarea vine din API, implicit 1
        let json = r#"{"workflow_runs":[{"id":1,"name":"W","status":"in_progress","html_url":"https://x","run_attempt":3},{"id":2,"name":"W","status":"in_progress","html_url":"https://x"}]}"#;
        let runs = parse_runs_at(json, "p", 0).unwrap();
        assert_eq!((runs[0].run_attempt, runs[1].run_attempt), (3, 1));
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

    // ---------- Cloudflare (JSON în forma din documentația API) ----------

    /// Un deployment Pages (`GET /accounts/{id}/pages/projects/{p}/deployments`) cu etapa curentă dată.
    fn pages_dep(id: &str, stage: &str, status: &str, extra: &str) -> String {
        format!(
            r#"{{"id":"{id}","short_id":"f64788e9","project_name":"site","environment":"production","url":"https://f64788e9.site.pages.dev",
            "created_on":"2026-10-17T10:00:00.923456Z","modified_on":"2026-10-17T10:05:00.045655Z","is_skipped":false,
            "latest_stage":{{"name":"{stage}","status":"{status}","started_on":"2026-10-17T10:00:00.1Z","ended_on":null}},
            "deployment_trigger":{{"type":"github:push","metadata":{{"branch":"main","commit_hash":"ad9c","commit_message":"Update index.html"}}}}{extra}}}"#
        )
    }

    fn pages_json(deps: &[String]) -> String {
        format!(r#"{{"success":true,"errors":[],"messages":[],"result":[{}],"result_info":{{"count":1,"page":1,"per_page":5,"total_count":1,"total_pages":1}}}}"#, deps.join(","))
    }

    /// „Acum” pentru fixture-uri: la 10 minute după crearea lor (10:00).
    fn cf_now() -> u64 {
        parse_iso("2026-10-17T10:10:00Z").unwrap()
    }

    fn states(runs: &[Run]) -> Vec<(&str, &str)> {
        runs.iter().map(|r| (r.id.as_str(), r.state.as_str())).collect()
    }

    #[test]
    fn pages_stari() {
        let json = pages_json(&[
            pages_dep("a", "build", "active", ""),
            pages_dep("b", "queued", "idle", ""),
            pages_dep("c", "build", "success", ""), // build gata, urmează deploy-ul: tot în curs
            pages_dep("d", "deploy", "active", ""),
            pages_dep("e", "deploy", "success", ""),
            pages_dep("f", "build", "failure", ""),
            pages_dep("g", "deploy", "failure", ""),
            pages_dep("h", "build", "canceled", ""),
            pages_dep("i", "deploy", "skipped", ""),
            pages_dep("j", "deploy", "success", r#","is_skipped":true"#),
        ]);
        let runs = parse_pages_at(&json, "site", cf_now()).unwrap();
        assert_eq!(states(&runs), vec![("a", "running"), ("b", "running"), ("c", "running"), ("d", "running"), ("e", "ok"), ("f", "failed"), ("g", "failed")]);
        assert!(runs.iter().all(|r| r.source == "cloudflare" && r.project == "site" && r.run_attempt == 1));
    }

    #[test]
    fn pages_campuri() {
        let runs = parse_pages_at(&pages_json(&[pages_dep("e", "deploy", "success", ""), pages_dep("a", "build", "active", ""), pages_dep("f", "build", "failure", "")]), "site", cf_now()).unwrap();
        // în curs: fără sfârșit; terminat: modified_on (2026-10-17T10:05:00Z)
        assert_eq!((runs[0].finished_at, runs[1].finished_at, runs[2].finished_at), (Some(1_792_231_500), None, Some(1_792_231_500)));
        assert_eq!(runs[0].name, "main");
        // reușit: site-ul; în curs / eșuat: pagina din panou (jurnalul build-ului)
        assert_eq!(runs[0].url, "https://f64788e9.site.pages.dev");
        assert_eq!(runs[1].url, "https://dash.cloudflare.com/?to=/:account/pages/view/site/a");
        assert_eq!(runs[2].url, "https://dash.cloudflare.com/?to=/:account/pages/view/site/f");
    }

    #[test]
    fn pages_raspunsuri_ciudate() {
        assert_eq!(parse_pages(&pages_json(&[]), "site").unwrap(), vec![]);
        assert!(parse_pages("nu e json", "site").is_err());
        assert!(parse_pages(r#"{"success":true,"errors":[],"messages":[],"result":null}"#, "site").is_err());
        let err = r#"{"success":false,"errors":[{"code":10000,"message":"Authentication error"}],"messages":[],"result":null}"#;
        let e = parse_pages(err, "site").unwrap_err();
        assert!(!e.contains("Authentication"), "textul serverului nu ajunge în mesaj: {e}");
        // un deployment fără id sau fără etapă se sare, restul rămân
        let bad = r#"{"success":true,"result":[{"url":"https://x"},{"id":"z"}]}"#;
        assert_eq!(parse_pages(bad, "site").unwrap(), vec![]);
    }

    /// Un build Workers (`GET /accounts/{id}/builds/workers/{tag}/builds`).
    fn build(uuid: &str, status: &str, outcome: Option<&str>) -> String {
        let outcome = outcome.map(|o| format!(r#""build_outcome":"{o}","#)).unwrap_or_default();
        format!(
            r#"{{"build_uuid":"{uuid}","status":"{status}",{outcome}"created_on":"2026-10-17T10:00:00.117Z","initializing_on":"2026-10-17T10:00:01.117Z",
            "running_on":"2026-10-17T10:00:02.117Z","stopped_on":"2026-10-17T10:03:00.117Z","modified_on":"2026-10-17T10:03:01.117Z",
            "build_trigger_metadata":{{"branch":"main","build_trigger_source":"push","commit_hash":"abc","commit_message":"Add","provider_type":"github","repo_name":"r"}}}}"#
        )
    }

    fn builds_json(items: &[String]) -> String {
        format!(r#"{{"success":true,"errors":[],"messages":[],"result":[{}],"result_info":{{"count":1,"page":1,"per_page":5,"total_count":1,"total_pages":1}}}}"#, items.join(","))
    }

    #[test]
    fn worker_builds_stari() {
        let json = builds_json(&[
            build("q", "queued", None),
            build("i", "initializing", None),
            build("r", "running", None),
            build("s", "stopped", Some("success")),
            build("f", "stopped", Some("fail")),
            build("t", "stopped", Some("terminated")),
            build("k", "stopped", Some("skipped")),
            build("c", "stopped", Some("cancelled")),
            build("n", "stopped", None),
        ]);
        let runs = parse_worker_builds_at(&json, "api", cf_now()).unwrap();
        assert_eq!(states(&runs), vec![("q", "running"), ("i", "running"), ("r", "running"), ("s", "ok"), ("f", "failed"), ("t", "failed")]);
        assert!(runs.iter().all(|r| r.source == "cloudflare" && r.project == "api" && r.run_attempt == 1 && r.name == "main"));
        assert_eq!(runs[0].finished_at, None);
        // terminat: stopped_on (2026-10-17T10:03:00Z)
        assert_eq!(runs[3].finished_at, Some(1_792_231_380));
        assert_eq!(runs[3].url, "https://dash.cloudflare.com/?to=/:account/workers/services/view/api/production/builds/s");
    }

    #[test]
    fn worker_builds_raspunsuri_ciudate() {
        assert_eq!(parse_worker_builds(&builds_json(&[]), "api").unwrap(), vec![]);
        assert!(parse_worker_builds("nu e json", "api").is_err());
        assert!(parse_worker_builds(r#"{"success":true,"result":null}"#, "api").is_err());
        let err = r#"{"success":false,"errors":[{"code":12005,"message":"Missing token from authorization header"}],"messages":[],"result":null}"#;
        let e = parse_worker_builds(err, "api").unwrap_err();
        assert!(!e.contains("token"), "{e}");
    }

    #[test]
    fn cloudflare_proiecte_active() {
        let now = parse_iso("2026-10-17T12:00:00Z").unwrap();
        let p = |name: &str, modified: &str| format!(r#"{{"name":"{name}","latest_deployment":{{"modified_on":"{modified}","created_on":"2020-01-01T00:00:00Z"}}}}"#);
        let json = format!(
            r#"{{"success":true,"result":[{},{},{},{},{},{},{{"name":"gol","latest_deployment":null}}]}}"#,
            p("vechi", "2026-10-01T00:00:00.5Z"),
            p("a", "2026-10-16T00:00:00.5Z"),
            p("b", "2026-10-17T11:00:00.5Z"),
            p("c", "2026-10-15T00:00:00.5Z"),
            p("d", "2026-10-14T00:00:00.5Z"),
            p("e", "2026-10-13T00:00:00.5Z"),
        );
        // cele din ultimele 7 zile, cele mai noi întâi, cel mult 3
        assert_eq!(cf_active_pages(&json, now), vec!["b", "a", "c"]);
        assert!(cf_active_pages("nu e json", now).is_empty());
        assert!(cf_active_pages(r#"{"success":false,"result":null}"#, now).is_empty());
    }

    #[test]
    fn cloudflare_workers_recente() {
        let w = |id: &str, tag: &str, modified: &str| format!(r#"{{"id":"{id}","tag":"{tag}","modified_on":"{modified}"}}"#);
        let json = format!(
            r#"{{"success":true,"result":[{},{},{},{},{{"id":"fara-tag"}}]}}"#,
            w("v", "t1", "2026-01-01T00:00:00Z"),
            w("n", "t2", "2026-10-17T11:00:00Z"),
            w("m", "t3", "2026-10-10T00:00:00Z"),
            w("x", "t4", "2026-10-16T00:00:00Z"),
        );
        // cele mai recent modificate, cel mult 3 (fără limită de vârstă: un build care eșuează nu modifică scriptul)
        assert_eq!(cf_recent_workers(&json), vec![("n".to_string(), "t2".to_string()), ("x".into(), "t4".into()), ("m".into(), "t3".into())]);
        assert!(cf_recent_workers("nu e json").is_empty());
    }

    #[test]
    fn cloudflare_erori_ca_coduri() {
        // codurile ajung în Setări și se traduc acolo; fără token, adrese sau textul serverului
        assert_eq!(cf_error(403, "Cloudflare Pages Read"), "forbidden:Cloudflare Pages Read");
        assert_eq!(cf_error(401, "x"), "unauthorized");
        assert_eq!(cf_error(404, "x"), "account_not_found");
        assert_eq!(cf_error(429, "x"), "rate_limited");
        assert_eq!(cf_error(500, "x"), "http:500");
        // JSON stricat / success:false → același cod, fără textul serverului
        assert_eq!(parse_pages("nu e json", "p").unwrap_err(), "bad_response");
        assert_eq!(parse_worker_builds(r#"{"success":false,"errors":[{"message":"Authentication error"}],"result":null}"#, "p").unwrap_err(), "bad_response");
    }

    #[test]
    fn blocate_in_coada_nu_au_inel() {
        let at = |mins: u64| parse_iso("2026-10-17T10:00:00Z").unwrap() + mins * 60; // create la 10:00
        let st = |stage: &str, status: &str, now: u64| states_of(parse_pages_at(&pages_json(&[pages_dep("a", stage, status, "")]), "p", now).unwrap());
        // proaspăt: în curs; de peste 30 de minute în coadă / inactiv: nu mai e „în curs”
        assert_eq!(st("queued", "idle", at(20)), vec!["running"]);
        assert!(st("queued", "idle", at(31)).is_empty());
        assert!(st("build", "idle", at(120)).is_empty());
        // un build care chiar lucrează rămâne în curs oricât durează
        assert_eq!(st("build", "active", at(120)), vec!["running"]);
        let w = |status: &str, now: u64| states_of(parse_worker_builds_at(&builds_json(&[build("b", status, None)]), "w", now).unwrap());
        assert_eq!(w("queued", at(20)), vec!["running"]);
        assert!(w("queued", at(31)).is_empty());
        assert_eq!(w("running", at(120)), vec!["running"]);
    }

    fn states_of(runs: Vec<Run>) -> Vec<String> {
        runs.into_iter().map(|r| r.state).collect()
    }

    #[test]
    fn ritmul_proiectelor_cloudflare() {
        let done = |ago: u64| run_at("1", "ok", Some(10_000 - ago));
        let running = run_at("2", "running", None);
        // niciodată verificat: acum
        assert!(cf_due(&[], None, 10_000));
        // cu ceva în curs: la fiecare ciclu
        assert!(cf_due(&[running], Some(9_999), 10_000));
        // terminat în ultima oră: ~55 s
        assert!(!cf_due(&[done(600)], Some(9_950), 10_000));
        assert!(cf_due(&[done(600)], Some(9_945), 10_000));
        // nimic în ultima oră (sau nimic deloc): 120 s
        assert!(!cf_due(&[done(7_200)], Some(9_900), 10_000));
        assert!(cf_due(&[done(7_200)], Some(9_880), 10_000));
        assert!(!cf_due(&[], Some(9_900), 10_000));
    }

    fn run_at(id: &str, state: &str, finished_at: Option<u64>) -> Run {
        run(id, state, finished_at)
    }

    #[test]
    fn bugete_separate_pe_surse() {
        let (mut gh, cf) = (Budget::default(), Budget::default());
        for i in 0..1000 {
            gh.record(i);
        }
        // GitHub a atins plafonul, Cloudflare nu: fiecare e oprit singur
        assert_eq!(paused_flags(&gh, &cf, 1000), (true, false));
        assert_eq!(paused_flags(&cf, &gh, 1000), (false, true));
        assert_eq!(paused_flags(&gh, &gh, 1000), (true, true));
    }

    #[test]
    fn gh_lipsa_se_reincearca_rar() {
        assert_eq!(gh_retry_after("missing"), 600);
        assert_eq!(gh_retry_after("unauthenticated"), 0);
        assert_eq!(gh_retry_after("error"), 0);
    }

    #[test]
    fn cloudflare_doar_cu_ambele_campuri() {
        assert!(cf_ready("tok", "0123456789abcdef0123456789abcdef"));
        assert!(!cf_ready("", "0123abcd"));
        assert!(!cf_ready("tok", ""));
        assert!(!cf_ready("  ", "0123abcd"));
        // ID-ul intră în adresă: nimic în afară de litere și cifre
        assert!(!cf_ready("tok", "abc/../x"));
        assert!(!cf_ready("tok", "abc?x=1"));
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
