//! Hook-ul Claude Code: instalează intrările în `~/.claude/settings.json` și,
//! rulat cu `--claude-hook`, scrie starea sesiunii într-un fișier mic și iese
//! înainte să pornească Tauri. Fișierele sunt citite de watcher-ul din aplicație.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::Read;
use std::path::{Path, PathBuf};
use windows::Win32::Foundation::{CloseHandle, BOOL, HWND, LPARAM};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowThreadProcessId, IsWindowVisible,
};

const MARKER: &str = "--claude-hook";
const EVENTS: [&str; 5] = ["UserPromptSubmit", "PostToolUse", "Notification", "Stop", "SessionEnd"];

/// Forma fișierului de sesiune (`<sessions_dir>/<session_id>.json`). `at` = ms unix;
/// `pid` / `hwnd` sunt 0 când nu au fost găsite.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SessionFile {
    pub state: String,
    pub cwd: String,
    pub hwnd: isize,
    pub pid: u32,
    pub at: u64,
}

/// `Dev`: aplicația rulează dintr-un build temporar (`.cargo-target`), deci hook-ul nu se instalează.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HookStatus {
    Installed,
    Missing,
    Invalid(String),
    Dev,
}

// ---------- logica pură ----------

pub fn state_for(event: &str, notification_type: Option<&str>) -> Option<&'static str> {
    match (event, notification_type) {
        // PostToolUse: după o aprobare sesiunea lucrează din nou (altfel ar rămâne „te așteaptă” până la Stop)
        ("UserPromptSubmit" | "PostToolUse", _) => Some("working"),
        ("Notification", Some("permission_prompt" | "elicitation_dialog")) => Some("waiting"),
        ("Stop", _) => Some("done"),
        ("SessionEnd", _) => Some("end"),
        _ => None,
    }
}

fn is_our_hook(h: &Value) -> bool {
    h["command"].as_str().is_some_and(|c| c.contains(MARKER))
}

fn entry_is_ours(entry: &Value) -> bool {
    entry["hooks"].as_array().is_some_and(|a| a.iter().any(is_our_hook))
}

pub fn has_hooks(v: &Value) -> bool {
    EVENTS.iter().all(|ev| v["hooks"][ev].as_array().is_some_and(|l| l.iter().any(entry_is_ours)))
}

/// Adaugă intrările noastre; dacă există deja (alt `exe`), doar le actualizează comanda.
pub fn add_hooks(mut v: Value, exe: &str) -> Value {
    let cmd = format!("\"{exe}\" {MARKER}");
    let Some(root) = v.as_object_mut() else { return v };
    let Some(hooks) = root.entry("hooks").or_insert_with(|| json!({})).as_object_mut() else { return v };
    for ev in EVENTS {
        let Some(list) = hooks.entry(ev).or_insert_with(|| json!([])).as_array_mut() else { continue };
        let mut found = false;
        for entry in list.iter_mut() {
            for h in entry.get_mut("hooks").and_then(Value::as_array_mut).into_iter().flatten() {
                if is_our_hook(h) {
                    h["command"] = json!(cmd);
                    found = true;
                }
            }
        }
        if !found {
            list.push(json!({ "hooks": [{ "type": "command", "command": cmd }] }));
        }
    }
    v
}

/// Scoate doar ce am adăugat noi; listele/obiectul golite de noi dispar, restul rămâne neatins.
pub fn remove_hooks(mut v: Value) -> Value {
    let Some(root) = v.as_object_mut() else { return v };
    let mut changed = false;
    if let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) {
        for ev in EVENTS {
            let Some(list) = hooks.get_mut(ev).and_then(Value::as_array_mut) else { continue };
            let mut touched = false;
            list.retain_mut(|entry| {
                let Some(inner) = entry.get_mut("hooks").and_then(Value::as_array_mut) else { return true };
                let n = inner.len();
                inner.retain(|h| !is_our_hook(h));
                touched |= n != inner.len();
                n == inner.len() || !inner.is_empty()
            });
            if touched && list.is_empty() {
                hooks.shift_remove(ev);
            }
            changed |= touched;
        }
        if changed && hooks.is_empty() {
            root.shift_remove("hooks");
        }
    }
    v
}

pub fn session_file_name(id: &str) -> Option<String> {
    let ok = !id.is_empty() && id.len() <= 128 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    // nume rezervate Windows (CON, NUL, COM1…): `con.json` ar deschide dispozitivul, nu un fișier
    let lower = id.to_ascii_lowercase();
    let reserved = matches!(lower.as_str(), "con" | "prn" | "aux" | "nul")
        || (lower.len() == 4 && (lower.starts_with("com") || lower.starts_with("lpt")) && matches!(lower.as_bytes()[3], b'1'..=b'9'));
    (ok && !reserved).then(|| format!("{id}.json"))
}

pub fn sessions_dir() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join("ro.andrei.dynamicisland").join("claude")
}

// ---------- IO pe settings.json ----------

fn settings_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("USERPROFILE").ok_or("USERPROFILE lipsește")?;
    Ok(PathBuf::from(home).join(".claude").join("settings.json"))
}

/// Fișier lipsă sau gol → `{}`; JSON invalid sau rădăcină care nu e obiect → eroare (fișierul rămâne neatins).
fn load(path: &Path) -> Result<Value, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(json!({})),
        Err(e) => return Err(e.to_string()),
    };
    if text.trim().is_empty() {
        return Ok(json!({}));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if v.is_object() { Ok(v) } else { Err("settings.json nu e un obiect JSON".into()) }
}

fn write_atomic(path: &Path, v: &Value) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(v).map_err(|e| e.to_string())? + "\n";
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

pub fn install_at(path: &Path, exe: &str) -> Result<(), String> {
    let v = load(path)?;
    let out = add_hooks(v.clone(), exe);
    if !has_hooks(&out) {
        return Err("structura „hooks” din settings.json nu e un obiect de liste".into());
    }
    if out != v { write_atomic(path, &out) } else { Ok(()) }
}

pub fn uninstall_at(path: &Path) -> Result<(), String> {
    let v = load(path)?;
    let out = remove_hooks(v.clone());
    if out != v { write_atomic(path, &out) } else { Ok(()) }
}

pub fn install(exe: &str) -> Result<(), String> {
    install_at(&settings_path()?, exe)
}

pub fn uninstall() -> Result<(), String> {
    uninstall_at(&settings_path()?)
}

pub fn status() -> HookStatus {
    match settings_path().and_then(|p| load(&p)) {
        Ok(v) if has_hooks(&v) => HookStatus::Installed,
        Ok(_) => HookStatus::Missing,
        Err(e) => HookStatus::Invalid(e),
    }
}

// ---------- modul CLI ----------

#[derive(Deserialize)]
struct HookInput {
    session_id: String,
    hook_event_name: String,
    #[serde(default)]
    cwd: String,
    #[serde(default)]
    notification_type: Option<String>,
}

/// `Some(cod)` pentru `--claude-hook` / `--claude-unhook`, altfel `None`. Nu paniquiește și nu printează nimic.
pub fn run_cli(args: &[String]) -> Option<i32> {
    let has = |f: &str| args.iter().skip(1).any(|a| a == f);
    let (hook, unhook) = (has(MARKER), has("--claude-unhook"));
    if !hook && !unhook {
        return None;
    }
    std::panic::set_hook(Box::new(|_| {})); // procesul iese oricum imediat după
    let _ = std::panic::catch_unwind(|| {
        if hook {
            let _ = handle_hook();
        } else {
            let _ = uninstall();
        }
    });
    Some(0)
}

fn handle_hook() -> Option<()> {
    let mut buf = Vec::new();
    std::io::stdin().take(8 * 1024 * 1024).read_to_end(&mut buf).ok()?;
    // PowerShell 5.1 pune un BOM UTF-8 în față când trimite text printr-un pipe
    let body = buf.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&buf);
    let input: HookInput = serde_json::from_slice(body).ok()?;
    let state = state_for(&input.hook_event_name, input.notification_type.as_deref())?;
    let path = sessions_dir().join(session_file_name(&input.session_id)?);
    if state == "end" {
        let _ = std::fs::remove_file(path);
        return Some(());
    }
    let (pid, hwnd) = find_claude();
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_millis() as u64;
    let rec = SessionFile { state: state.into(), cwd: input.cwd, hwnd, pid, at };
    write_atomic(&path, &serde_json::to_value(rec).ok()?).ok()
}

/// (pid-ul primului strămoș claude/node, hwnd-ul primei ferestre vizibile a unui strămoș); 0 când lipsesc.
fn find_claude() -> (u32, isize) {
    let procs = snapshot();
    let mut chain: Vec<&(u32, u32, String)> = Vec::new();
    let mut cur = std::process::id();
    for _ in 0..16 {
        let Some(me) = procs.iter().find(|p| p.0 == cur) else { break };
        let Some(parent) = procs.iter().find(|p| p.0 == me.1) else { break };
        chain.push(parent);
        cur = parent.0;
    }
    let pid = chain
        .iter()
        .find(|p| p.2.eq_ignore_ascii_case("claude.exe") || p.2.eq_ignore_ascii_case("node.exe"))
        .map_or(0, |p| p.0);
    let wins = visible_windows();
    let named: Vec<(u32, String)> = chain.iter().map(|p| (p.0, p.2.clone())).collect();
    (pid, pick_hwnd(&named, &wins))
}

/// Prima fereastră (hwnd, pid) a unui strămoș din lanț; mersul se oprește la explorer.exe,
/// a cărui fereastră e „Program Manager" (desktopul), nu terminalul sesiunii.
fn pick_hwnd(chain: &[(u32, String)], wins: &[(isize, u32)]) -> isize {
    for (pid, exe) in chain {
        if exe.eq_ignore_ascii_case("explorer.exe") {
            break;
        }
        if let Some(w) = wins.iter().find(|w| w.1 == *pid) {
            return w.0;
        }
    }
    0
}

/// (pid, ppid, nume exe) pentru toate procesele.
fn snapshot() -> Vec<(u32, u32, String)> {
    let mut out = Vec::new();
    unsafe {
        let Ok(h) = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) else { return out };
        let mut e = PROCESSENTRY32W { dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32, ..Default::default() };
        let mut ok = Process32FirstW(h, &mut e).is_ok();
        while ok {
            let n = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
            out.push((e.th32ProcessID, e.th32ParentProcessID, String::from_utf16_lossy(&e.szExeFile[..n])));
            ok = Process32NextW(h, &mut e).is_ok();
        }
        let _ = CloseHandle(h);
    }
    out
}

/// (hwnd, pid) pentru ferestrele de nivel superior vizibile și cu titlu, în ordinea Z.
fn visible_windows() -> Vec<(isize, u32)> {
    unsafe extern "system" fn on_window(h: HWND, lp: LPARAM) -> BOOL {
        let out = &mut *(lp.0 as *mut Vec<(isize, u32)>);
        if IsWindowVisible(h).as_bool() && GetWindowTextLengthW(h) > 0 {
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, Some(&mut pid));
            out.push((h.0 as isize, pid));
        }
        BOOL(1)
    }
    let mut out: Vec<(isize, u32)> = Vec::new();
    unsafe {
        let _ = EnumWindows(Some(on_window), LPARAM(&mut out as *mut _ as isize));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn stari_din_evenimente() {
        assert_eq!(state_for("UserPromptSubmit", None), Some("working"));
        assert_eq!(state_for("Notification", Some("permission_prompt")), Some("waiting"));
        assert_eq!(state_for("Notification", Some("elicitation_dialog")), Some("waiting"));
        assert_eq!(state_for("Notification", Some("idle_prompt")), None);
        assert_eq!(state_for("Stop", None), Some("done"));
        assert_eq!(state_for("SessionEnd", None), Some("end"));
        assert_eq!(state_for("PostToolUse", None), Some("working")); // după o aprobare, sesiunea revine la „lucrează”
        assert_eq!(state_for("PreToolUse", None), None);
    }

    #[test]
    fn set_vechi_de_4_evenimente_apare_ca_lipsa() {
        let mut v = add_hooks(json!({}), "di.exe");
        v["hooks"].as_object_mut().unwrap().shift_remove("PostToolUse");
        assert!(!has_hooks(&v));
        assert!(has_hooks(&add_hooks(v, "di.exe"))); // reinstalarea îl adaugă
    }

    #[test]
    fn hooks_adaugate_idempotent() {
        let v = add_hooks(json!({}), r"C:\X\dynamic-island.exe");
        assert!(has_hooks(&v));
        assert_eq!(add_hooks(v.clone(), r"C:\X\dynamic-island.exe"), v);
        assert_eq!(v["hooks"]["Stop"][0]["hooks"][0]["command"], "\"C:\\X\\dynamic-island.exe\" --claude-hook");
    }

    #[test]
    fn hooks_pastreaza_restul() {
        let orig = json!({ "theme": "dark", "env": { "A": "1" },
            "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "notify.exe" }] }] } });
        let v = add_hooks(orig.clone(), "di.exe");
        assert_eq!(v["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert_eq!(v["theme"], "dark");
        let back = remove_hooks(v);
        assert_eq!(back, orig); // inclusiv ordinea cheilor (preserve_order)
    }

    #[test]
    fn scoaterea_sterge_listele_goale() {
        let v = remove_hooks(add_hooks(json!({ "theme": "dark" }), "di.exe"));
        assert_eq!(v, json!({ "theme": "dark" }));
    }

    #[test]
    fn json_invalid_neatins() {
        let d = std::env::temp_dir().join(format!("di-hook-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("settings.json");
        std::fs::write(&p, "{ nu e json").unwrap();
        assert!(install_at(&p, "di.exe").is_err());
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "{ nu e json");
        std::fs::remove_file(&p).unwrap();
        install_at(&p, "di.exe").unwrap(); // lipsă → creat
        assert!(has_hooks(&serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()));
        uninstall_at(&p).unwrap();
        assert!(!has_hooks(&serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()));
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn nume_sesiune_nesigure_respinse() {
        assert!(session_file_name("abc-123_X").is_some());
        assert!(session_file_name("..\\x").is_none());
        assert!(session_file_name("a/b").is_none());
        assert!(session_file_name("").is_none());
        for n in ["con", "CON", "Prn", "aux", "nul", "com1", "COM9", "lpt1", "Lpt9"] {
            assert!(session_file_name(n).is_none(), "{n}");
        }
        assert!(session_file_name("com10").is_some());
        assert!(session_file_name("console").is_some());
    }

    #[test]
    fn hwnd_ocoleste_explorer() {
        let wins = [(111isize, 10u32), (222, 20), (333, 30)];
        let chain = |v: &[(u32, &str)]| v.iter().map(|(p, e)| (*p, e.to_string())).collect::<Vec<_>>();
        // fereastra primului strămoș care are una
        assert_eq!(pick_hwnd(&chain(&[(5, "node.exe"), (20, "WindowsTerminal.exe")]), &wins), 222);
        // conhost/cmd clasic ajunge la explorer („Program Manager"): hwnd rămâne 0
        assert_eq!(pick_hwnd(&chain(&[(5, "cmd.exe"), (10, "Explorer.EXE")]), &wins), 0);
        // mersul se oprește la explorer, chiar dacă un strămoș mai sus are fereastră
        assert_eq!(pick_hwnd(&chain(&[(10, "explorer.exe"), (20, "x.exe")]), &wins), 0);
        assert_eq!(pick_hwnd(&[], &wins), 0);
    }
}
