//! I `#[test]` di una mod Patina (`.ptn`), lanciati dal binario `patina-test` e mostrati nel pannello dei test di Cargo.
//!
//! Un **adattatore**, non un pannello nuovo: il binario parla JSON a righe (`--format json`, vedi `patina-testkit/src/json.rs`)
//! e qui ogni riga diventa uno degli eventi che il pannello di `cargo_tests` gia' conosce (`cargo-test-target`, `cargo-test-case`,
//! `cargo-test-target-done`, `test-output`, `test-exit`). L'id della corsa e' prefissato `cargo-` come le altre, cosi' lo store lo riconosce
//! e Stop e' lo stesso verbo (`bennu_cancel_tests`: un registro solo).
//!
//! Il pacchetto del pannello e' l'id della mod, il target e' `lib`. Il tempo per prova non c'e' (la VM non lo misura): c'e' il gas, nella nota.
//!
//! Il binario si cerca, nell'ordine: l'argomento `binary` (l'impostazione dell'utente), `PATINA_TEST_BIN`, `engine/patina/.../target/release/patina-test`
//! salendo dalla cartella della mod, poi il `PATH`. Se il binario dice di essere piu' vecchio della libreria (`banner.stale`), l'avviso va nel registro
//! della corsa e in una riga `err`: gli errori su costrutti nuovi possono essere falsi.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use arbor_process_ext::prelude::NoWindowExt;
use bennu_core::prelude::BennuState;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::cargo_tests::CargoRunHandle;
use crate::tests::{next_run_id, registry, LiveRun, EVT_TEST_EXIT, EVT_TEST_OUTPUT};

const EVT_CARGO_TARGET: &str = "arbor://bennu/cargo-test-target";
const EVT_CARGO_CASE: &str = "arbor://bennu/cargo-test-case";
const EVT_CARGO_TARGET_DONE: &str = "arbor://bennu/cargo-test-target-done";

/// Dove sta il binario rispetto alla radice del workspace Rust che contiene `engine/patina`.
const BIN_REL: &str = "engine/patina/crates/patina-testkit/target/release/patina-test";

/// Argomenti di [`bennu_run_patina_tests`].
#[derive(Deserialize)]
pub struct PatinaRunArgs {
    /// La cartella della mod (con `manifest.ron` e `script/`).
    pub mod_dir: String,
    /// Solo queste prove (nomi come li stampa il banco); vuoto = tutte.
    #[serde(default)]
    pub only: Vec<String>,
    /// Il percorso del binario scelto dall'utente, se c'e'.
    #[serde(default)]
    pub binary: Option<String>,
    /// Con `--gas`? (il gas c'e' comunque nella nota di ogni prova)
    #[serde(default)]
    pub allow_unrealized: bool,
}

/// Il binario `patina-test`: argomento, variabile d'ambiente, albero, `PATH`.
pub(crate) fn find_binary(explicit: Option<&str>, mod_dir: &Path) -> Result<PathBuf, String> {
    if let Some(p) = explicit.filter(|p| !p.trim().is_empty()) {
        let p = PathBuf::from(p);
        return if p.is_file() { Ok(p) } else { Err(format!("patina-test non c'e' in {} (impostazione del percorso)", p.display())) };
    }
    if let Ok(p) = std::env::var("PATINA_TEST_BIN") {
        let p = PathBuf::from(p);
        if p.is_file() {
            return Ok(p);
        }
    }
    for a in mod_dir.ancestors() {
        let c = a.join(BIN_REL);
        if c.is_file() {
            return Ok(c);
        }
        // il workspace Rust sta sopra il repo del gioco: `workspace/rust/engine/patina` accanto a `workspace/rust/games/voriz`
        if let Some(rust) = a.parent() {
            let c = rust.join(BIN_REL);
            if c.is_file() {
                return Ok(c);
            }
        }
    }
    Err("patina-test non trovato: impostane il percorso, oppure costruiscilo con `cargo build --release --manifest-path engine/patina/crates/patina-testkit/Cargo.toml`".into())
}

/// Gli id delle dipendenze dichiarate nel `manifest.ron` (senza `voriz`, che e' il gioco).
pub(crate) fn dependency_ids(manifest: &str) -> Vec<String> {
    let Some(at) = manifest.find("dependencies") else { return vec![] };
    let rest = &manifest[at..];
    let Some(open) = rest.find('[') else { return vec![] };
    // fino alla `]` che chiude l'elenco (le tuple interne non contengono `]`)
    let Some(close) = rest[open..].find(']') else { return vec![] };
    let body = &rest[open..open + close];
    let mut out = vec![];
    let mut s = body;
    while let Some(i) = s.find("id:") {
        s = &s[i + 3..];
        if let Some(q1) = s.find('"') {
            if let Some(q2) = s[q1 + 1..].find('"') {
                let id = &s[q1 + 1..q1 + 1 + q2];
                if id != "voriz" {
                    out.push(id.to_string());
                }
                s = &s[q1 + 1 + q2 + 1..];
            }
        }
    }
    out
}

/// Le cartelle di `--search`: se la mod dipende da altre mod, le sorelle (la cartella che la contiene).
pub(crate) fn search_dirs(mod_dir: &Path) -> Vec<PathBuf> {
    let manifest = std::fs::read_to_string(mod_dir.join("manifest.ron")).unwrap_or_default();
    if dependency_ids(&manifest).is_empty() {
        return vec![];
    }
    mod_dir.parent().map(|p| vec![p.to_path_buf()]).unwrap_or_default()
}

/// Una riga JSON del binario diventa zero o piu' eventi del pannello.
fn forward(sink: &Arc<dyn arbor_ipc::prelude::EventSink>, run_id: &str, package: &str, v: &Value, passed: &mut u32, failed: &mut u32, stale: &mut Option<String>) {
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("");
    let target = json!({ "kind": "lib" });
    match s("event") {
        "banner" => {
            if let Some(w) = v.get("stale").and_then(Value::as_str) {
                *stale = Some(w.to_string());
            }
        }
        "problem" => {
            let place = if s("file").is_empty() { String::new() } else { format!("{}:{}: ", s("file"), v.get("line").and_then(Value::as_u64).unwrap_or(0)) };
            let kind = s("kind");
            let text = format!("{kind}: {place}{}", s("message"));
            let is_err = v.get("error").and_then(Value::as_bool).unwrap_or(false);
            sink.emit(EVT_TEST_OUTPUT, json!({ "run_id": run_id, "stream": if is_err { "stderr" } else { "stdout" }, "text": text }));
        }
        "test" => {
            let status = match s("status") {
                "passed" => { *passed += 1; "passed" }
                "failed" => { *failed += 1; "failed" }
                "trapped" => { *failed += 1; "error" }
                _ => { *failed += 1; "error" }
            };
            let name = s("name");
            let (module, short) = match name.rsplit_once("::") { Some((m, n)) => (m, n), None => ("", name) };
            let note = format!("gas {}{}", v.get("gas").and_then(Value::as_u64).unwrap_or(0), if s("side").is_empty() { String::new() } else { format!(", {}", s("side").to_lowercase()) });
            sink.emit(EVT_CARGO_CASE, json!({
                "run_id": run_id, "index": 0, "package": package, "target": target,
                "module": module, "name": short, "path": name, "status": status, "note": note,
            }));
            if status != "passed" {
                let place = if s("file").is_empty() { String::new() } else { format!("\n  --> {}:{}", s("file"), v.get("line").and_then(Value::as_u64).unwrap_or(0)) };
                sink.emit(EVT_CARGO_CASE, json!({ "run_id": run_id, "index": 0, "path": name, "message": format!("{}{place}", s("message")) }));
            }
            sink.emit(EVT_TEST_OUTPUT, json!({ "run_id": run_id, "stream": "stdout", "text": format!("{} {name}", if status == "passed" { "ok      " } else { "FAILED  " }) }));
        }
        "summary" => {
            sink.emit(EVT_CARGO_TARGET_DONE, json!({
                "run_id": run_id, "index": 0,
                "result": { "passed": *passed, "failed": *failed, "ignored": 0, "measured": 0, "filtered_out": 0,
                            "time_ms": v.get("ms").and_then(Value::as_u64).unwrap_or(0), "ok": v.get("ok").and_then(Value::as_bool).unwrap_or(false) },
            }));
        }
        _ => {}
    }
}

/// Lancia `patina-test` su una mod e racconta il rapporto al pannello dei test, come eventi.
#[arbor_rpc::handler]
fn bennu_run_patina_tests(ctx: &BennuState, args: PatinaRunArgs) -> Result<CargoRunHandle, String> {
    let mod_dir = PathBuf::from(&args.mod_dir);
    if !mod_dir.join("manifest.ron").is_file() {
        return Err(format!("{} non e' una mod (manca manifest.ron)", mod_dir.display()));
    }
    let bin = find_binary(args.binary.as_deref(), &mod_dir)?;
    let mut cmd = Command::new(&bin);
    cmd.arg(&mod_dir).args(["--format", "json"]);
    for d in search_dirs(&mod_dir) {
        cmd.arg("--search").arg(d);
    }
    for o in &args.only {
        cmd.arg("--only").arg(o);
    }
    if args.allow_unrealized {
        cmd.arg("--allow-unrealized");
    }
    cmd.current_dir(&mod_dir).stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null());
    cmd.no_window();
    crate::child::own_group(&mut cmd);
    let mut child = cmd.spawn().map_err(|e| format!("Could not run {}: {e}", bin.display()))?;

    let mod_id = mod_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let run_id = format!("cargo-{}", next_run_id());
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    crate::child::track(&child);
    let child = Arc::new(Mutex::new(child));
    registry().lock().unwrap_or_else(|p| p.into_inner()).insert(run_id.clone(), LiveRun { child: child.clone(), cancelled: Arc::new(Mutex::new(false)) });

    let command = format!("{} {}", bin.display(), cmd_line(&mod_dir, &args.only));
    let handle = CargoRunHandle {
        run_id: run_id.clone(),
        label: if args.only.is_empty() { format!("mod {mod_id}") } else { args.only.join(", ") },
        command,
        widened: None,
        // Patina's own test binary speaks libtest, which is what the cargo runner reads.
        runner: bennu_test::prelude::CargoRunner::Cargo,
        runner_note: None,
    };
    let sink = ctx.event_sink();
    let rid = run_id.clone();
    std::thread::Builder::new().name(format!("bennu-patina-test-{rid}")).spawn(move || {
        // il gruppo (mod) si apre subito: le righe `test` arrivano dopo, ma il pannello vuole il target prima dei casi
        sink.emit(EVT_CARGO_TARGET, json!({ "run_id": rid, "index": 0, "count": 0, "package": mod_id, "target": { "kind": "lib" }, "desc": format!("mod {mod_id}") }));
        let err_pump = stderr.map(|e| {
            let sink = sink.clone();
            let rid = rid.clone();
            std::thread::spawn(move || {
                for line in BufReader::new(e).lines().map_while(Result::ok) {
                    sink.emit(EVT_TEST_OUTPUT, json!({ "run_id": rid, "stream": "stderr", "text": line }));
                }
            })
        });
        let (mut passed, mut failed, mut stale) = (0u32, 0u32, None);
        if let Some(out) = stdout {
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                match serde_json::from_str::<Value>(&line) {
                    Ok(v) => forward(&sink, &rid, &mod_id, &v, &mut passed, &mut failed, &mut stale),
                    Err(_) => sink.emit(EVT_TEST_OUTPUT, json!({ "run_id": rid, "stream": "stdout", "text": line })),
                }
            }
        }
        if let Some(w) = &stale {
            sink.emit(EVT_TEST_OUTPUT, json!({ "run_id": rid, "stream": "stderr", "text": format!("ATTENZIONE: {w}") }));
        }
        let code = child.lock().unwrap_or_else(|p| p.into_inner()).wait().ok().and_then(|s| s.code());
        if let Some(p) = err_pump {
            let _ = p.join();
        }
        let cancelled = registry().lock().unwrap_or_else(|p| p.into_inner()).remove(&rid)
            .map(|r| { crate::child::untrack(&r.child); *r.cancelled.lock().unwrap_or_else(|p| p.into_inner()) })
            .unwrap_or(false);
        sink.emit(EVT_TEST_EXIT, json!({
            "run_id": rid, "code": code, "cancelled": cancelled,
            "totals": { "run": passed + failed, "failures": failed, "errors": 0, "skipped": 0 },
        }));
    }).map_err(|e| format!("spawn patina-test thread: {e}"))?;
    Ok(handle)
}

fn cmd_line(mod_dir: &Path, only: &[String]) -> String {
    let mut s = format!("{} --format json", mod_dir.display());
    for o in only {
        s.push_str(&format!(" --only {o}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependencies_skip_the_game() {
        let m = r#"( id: "x", dependencies: [(id: "voriz", min_version: Some((major: 0))), (id: "gamekit", min_version: None)], author: None )"#;
        assert_eq!(dependency_ids(m), vec!["gamekit".to_string()]);
        assert!(dependency_ids("( id: \"x\", dependencies: [] )").is_empty());
        assert!(dependency_ids("( id: \"x\" )").is_empty());
    }
}
