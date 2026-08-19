//! `cargo test` domain — discovery and a live run for a Rust workspace.
//!
//! The Maven runner ([`crate::tests`]) watches a directory: Surefire writes one XML file per class
//! and the tree fills in as the files land. Cargo writes no files, so this one reads the output —
//! and the output arrives on **two pipes that each hold half of the answer**:
//!
//! - **stderr** is cargo's. `Compiling foo`, then one `Running <desc> (<exe>)` line per test binary.
//!   This is the only place a test's *target* is named.
//! - **stdout** is the test binary's. `running N tests`, one line per case as it finishes, the
//!   captured panic of each failure, and a `test result:` summary.
//!
//! Neither is usable alone: stdout says `test util::tests::works ... ok`, and in a twenty-crate
//! workspace four crates have a `util::tests::works`.
//!
//! ## Pairing the halves by index, not by arrival
//!
//! Cargo runs test binaries one at a time and prints its `Running` line before each; libtest prints
//! `running N tests` as each one starts. So the **k-th** `running N tests` on stdout belongs to the
//! **k-th** `Running` on stderr — regardless of how the two pipes happen to be scheduled. The
//! stdout pump therefore waits for entry `k` of the announced list rather than for "the most recent
//! one", which would be a race whose symptom is a test filed under the wrong crate.
//!
//! The wait is **bounded** ([`TARGET_WAIT`]). An old cargo that prints no description, or a
//! `Running` line we fail to recognise, must cost an unnamed group — not a pump that stops
//! emitting, which would look like a run that hung.
//!
//! ## Why the run is not `--offline`
//!
//! Same reasoning as the Maven side's onlineness: a workspace whose dev-dependencies have never
//! been fetched cannot compile its tests, and `--offline` turns that into a resolution error that
//! reads as a bug in Bennu. Cargo prefers the local cache anyway, so a warm `~/.cargo` costs
//! nothing.
//!
//! ## Cancel
//!
//! Through [`crate::child::kill_tree`], and registered in the **same** registry as the Maven runs
//! ([`crate::tests::registry`]) so `bennu_cancel_tests` stops either kind. Cargo is a parent of the
//! test binaries it spawns, so killing only the handle would leave a test process holding
//! `target/`.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, RwLock};
use std::time::Duration;

use arbor_ipc::prelude::EventSink;
use arbor_process_ext::prelude::NoWindowExt;
use bennu_cargo::prelude::{read_workspace, CargoWorkspace};
use bennu_core::prelude::BennuState;
use bennu_test::prelude::{
    cargo_plan, compiling_crate, discover_rust_in_source, place_of, running_target, CargoTestScope,
    LibtestEvent, LibtestParser, RustTest, TestStatus, TestTarget,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::build::{BuildGuard, BUSY_MSG};
use crate::test_report::RunEnd;
use crate::tests::{next_run_id, registry, LiveRun, EVT_TEST_EXIT, EVT_TEST_OUTPUT};

// ── event topics ───────────────────────────────────────────────────────────────

/// A test binary started: which crate and target, and how many cases it holds.
const EVT_CARGO_TARGET: &str = "arbor://bennu/cargo-test-target";
/// One case finished.
const EVT_CARGO_CASE: &str = "arbor://bennu/cargo-test-case";
/// A test binary finished — libtest's own counts for it.
const EVT_CARGO_TARGET_DONE: &str = "arbor://bennu/cargo-test-target-done";
/// Cargo is compiling. Emitted so the panel can say so: on a cold workspace the first several
/// seconds of a test run produce no tests at all, and silence there reads as a hang.
const EVT_CARGO_COMPILING: &str = "arbor://bennu/cargo-test-compiling";

/// How long the stdout pump waits for cargo's `Running` line before giving up on naming a target.
/// Generous — the two lines are written microseconds apart in practice — but finite.
const TARGET_WAIT: Duration = Duration::from_millis(2_000);

/// Directories never walked for tests.
const SKIP_DIRS: &[&str] = &["target", "node_modules", ".git", ".idea", ".arbor"];

/// Cap on the files one discovery reads. Far above any real workspace; the point is that a
/// checked-in vendor tree cannot turn opening a panel into a minute of I/O.
const MAX_FILES: usize = 20_000;

// ── bennu_discover_cargo_tests ─────────────────────────────────────────────────

/// Args for [`bennu_discover_cargo_tests`].
#[derive(Deserialize)]
pub struct DiscoverArgs {
    /// Absolute path to the workspace root.
    pub root: String,
    /// Scan only this file, always freshly — the "run the test at the caret" path, where the
    /// answer must reflect the file as it is now rather than as a cache remembers it.
    #[serde(default)]
    pub file: Option<String>,
    /// Re-scan even if the workspace has been scanned before (the panel's Refresh).
    #[serde(default)]
    pub force: bool,
}

/// Every `#[test]` in the workspace — or, with `file`, in that one file.
///
/// Read from **disk**, not from the editor buffer, and for the same reason the Java side is: cargo
/// compiles from disk, so a test discovered in unsaved text is a test the runner cannot run, and a
/// catalogue that disagrees with what will execute is worse than one that lags by a save.
#[arbor_rpc::handler]
pub(crate) fn bennu_discover_cargo_tests(
    _ctx: &BennuState,
    args: DiscoverArgs,
) -> Result<Vec<RustTest>, String> {
    let root = PathBuf::from(&args.root);
    let ws = Arc::new(read_workspace(&root));

    if let Some(file) = &args.file {
        return Ok(discover_file(&root, Path::new(file), &ws));
    }
    if !args.force {
        if let Some(hit) = cache().read().ok().and_then(|c| c.get(&args.root).cloned()) {
            return Ok((*hit).clone());
        }
    }

    let mut paths = Vec::new();
    collect_rust(&root, &mut paths);
    paths.truncate(MAX_FILES);
    let found: Vec<RustTest> =
        paths.iter().flat_map(|p| discover_file(&root, p, &ws)).collect();

    if let Ok(mut c) = cache().write() {
        c.insert(args.root.clone(), Arc::new(found.clone()));
    }
    Ok(found)
}

/// One file's tests, placed in the build.
///
/// A file no crate owns yields nothing — that is a `.rs` outside every member, which cargo does not
/// compile and which therefore has no target a run could name.
fn discover_file(root: &Path, file: &Path, ws: &CargoWorkspace) -> Vec<RustTest> {
    let Ok(text) = std::fs::read_to_string(file) else { return Vec::new() };
    let path = file.to_string_lossy().replace('\\', "/");
    let Some((package, rel, has_lib)) = crate_of(root, &path, ws) else { return Vec::new() };
    let Some(place) = place_of(&rel, &package, has_lib) else { return Vec::new() };
    discover_rust_in_source(&path, &text, &place)
}

/// Which crate owns `file`, the path relative to that crate, and whether the crate has a library.
///
/// The **longest** matching `rel_path` wins, which is what makes a nested member (`crates/a/sub`)
/// beat its ancestor rather than every file in the tree belonging to the root crate.
fn crate_of(root: &Path, file: &str, ws: &CargoWorkspace) -> Option<(String, String, bool)> {
    let root_s = root.to_string_lossy().replace('\\', "/");
    let rel_to_root = file.strip_prefix(&root_s)?.trim_start_matches('/');
    let mut best: Option<(&str, &str, bool)> = None;
    for c in &ws.crates {
        let prefix = c.rel_path.trim_matches('/');
        let inside = match prefix.is_empty() {
            true => Some(rel_to_root),
            false => rel_to_root.strip_prefix(prefix).and_then(|r| r.strip_prefix('/')),
        };
        let Some(inside) = inside else { continue };
        let has_lib = c.targets.iter().any(|t| t.kind == "lib");
        // The shortest remainder means the longest matching prefix, which is the nested member.
        if best.is_none() || best.is_some_and(|(_, prev, _)| inside.len() < prev.len()) {
            best = Some((c.name.as_str(), inside, has_lib));
        }
    }
    best.map(|(name, rel, has_lib)| (name.to_string(), rel.to_string(), has_lib))
}

/// Every `.rs` file under `dir`, skipping build output and hidden trees.
fn collect_rust(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_dir() {
            if SKIP_DIRS.contains(&name) || name.starts_with('.') {
                continue;
            }
            collect_rust(&p, out);
        } else if name.ends_with(".rs") {
            out.push(p);
        }
    }
}

/// Whole-workspace discovery, per root. A `.rs` tree changes rarely and the walk parses every file
/// in it, so opening the panel must not pay for it twice.
fn cache() -> &'static RwLock<HashMap<String, Arc<Vec<RustTest>>>> {
    static CACHE: OnceLock<RwLock<HashMap<String, Arc<Vec<RustTest>>>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Drop a workspace's cached discovery — called after a save, so a newly written test appears
/// without a restart.
pub(crate) fn forget_discovery(root: &str) {
    if let Ok(mut c) = cache().write() {
        c.remove(root);
    }
}

// ── bennu_run_cargo_tests ──────────────────────────────────────────────────────

/// Args for [`bennu_run_cargo_tests`].
#[derive(Deserialize)]
pub struct RunArgs {
    pub root: String,
    /// What to run: the workspace, a crate, a target, a module, or individual cases.
    pub scope: CargoTestScope,
    /// Also run the `#[ignore]`d ones.
    #[serde(default)]
    pub include_ignored: bool,
}

/// The handle correlating a live run with its event stream.
#[derive(Debug, Clone, Serialize)]
pub struct CargoRunHandle {
    pub run_id: String,
    /// What is being run, in words (`crate bennu-test`, `util::tests::works`, `all tests`).
    pub label: String,
    /// The command line, so the panel can show what it actually ran — a filter is easy to get
    /// subtly wrong and impossible to diagnose from a result tree alone.
    pub command: String,
    /// Set when the selection was too large to spell and the run was widened. The panel must show
    /// it: the user asked for a subset and is getting a superset.
    pub widened: Option<String>,
}

/// A launched `cargo test`, before anyone has waited on it.
///
/// The Maven runner's twin, and split for the same reason — see [`crate::tests::MavenRun`]:
/// one loop, driven either on a thread (the panel wants the handle) or inline (a caller
/// wants the answer).
pub(crate) struct CargoRun {
    handle: CargoRunHandle,
    guard: BuildGuard,
    child: Arc<Mutex<std::process::Child>>,
    stdout: Option<std::process::ChildStdout>,
    stderr: Option<std::process::ChildStderr>,
    run_id: String,
    sink: Arc<dyn EventSink>,
    ws: Arc<CargoWorkspace>,
    announced: Announced,
    totals: Arc<Mutex<Totals>>,
}

/// Spawn `cargo test` for `scope` and register it, without waiting for anything.
pub(crate) fn start_cargo_run(ctx: &BennuState, args: &RunArgs) -> Result<CargoRun, String> {
    // The same lock the build takes: two cargo processes on one workspace queue on cargo's own
    // `target/` lock, which looks like a hang rather than like a queue.
    let guard = BuildGuard::acquire().ok_or_else(|| BUSY_MSG.to_string())?;

    let root = PathBuf::from(&args.root);
    let plan = cargo_plan(&args.scope, args.include_ignored);
    // Resolved rather than taken from `PATH`: a windowed app doesn't have `~/.cargo/bin` on it.
    let launcher = crate::cargo_cmd::cargo_launcher();
    let mut cmd = Command::new(&launcher);
    cmd.current_dir(&root)
        .args(&plan.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    // Belt and braces with `--color never`: a config file can turn colour on for every invocation,
    // and an escape sequence inside a `Running` line is a target the panel cannot name.
    cmd.env("CARGO_TERM_COLOR", "never");
    cmd.no_window();

    let mut child = cmd.spawn().map_err(|e| {
        // Name what was actually tried: "not on PATH" sends the reader to their shell config,
        // which is the wrong place when the app never reads it.
        format!("Could not run cargo ({}): {e}. Install Rust, or make cargo reachable from a \
                 windowed app — it does not inherit your shell's PATH.", launcher.to_string_lossy())
    })?;

    // Prefixed, because the exit event is shared with the Maven runner and both stores listen to
    // it: the prefix is how each one recognises its own run without a second topic.
    let run_id = format!("cargo-{}", next_run_id());
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let child = Arc::new(Mutex::new(child));
    registry().lock().unwrap_or_else(|p| p.into_inner()).insert(
        run_id.clone(),
        LiveRun { child: child.clone(), cancelled: Arc::new(Mutex::new(false)) },
    );

    Ok(CargoRun {
        handle: CargoRunHandle {
            run_id: run_id.clone(),
            label: plan.label,
            command: format!("cargo {}", plan.args.join(" ")),
            widened: plan.widened,
        },
        guard,
        child,
        stdout,
        stderr,
        run_id,
        sink: ctx.event_sink(),
        ws: Arc::new(read_workspace(&root)),
        announced: Arc::new((Mutex::new(Vec::new()), Condvar::new())),
        totals: Arc::new(Mutex::new(Totals::default())),
    })
}

impl CargoRun {
    /// The handle the streaming caller returns before any of this has happened.
    pub(crate) fn handle(&self) -> CargoRunHandle {
        self.handle.clone()
    }

    /// Pump both streams until cargo exits, emit the exit event. Blocks for as long as the
    /// run does. `collector`, when given, is filled with every case as it is announced.
    pub(crate) fn drive(mut self, collector: Option<Arc<crate::test_report::Collector>>) -> RunEnd {
        // The guard rides the run: the lock is held for as long as cargo runs, not just for
        // as long as the handler that started it.
        let _guard = self.guard;
        self.sink.progress(&format!("cargo test — {}", self.handle.label), None, None);
        let mut pumps = Vec::new();
        if let Some(err) = self.stderr.take() {
            pumps.push(spawn_stderr_pump(
                err,
                self.run_id.clone(),
                self.sink.clone(),
                self.announced.clone(),
                self.ws.clone(),
            ));
        }
        if let Some(out) = self.stdout.take() {
            pumps.push(spawn_stdout_pump(
                out,
                self.run_id.clone(),
                self.sink.clone(),
                self.announced.clone(),
                self.totals.clone(),
                collector,
            ));
        }

        let code =
            self.child.lock().unwrap_or_else(|p| p.into_inner()).wait().ok().and_then(|s| s.code());
        for p in pumps {
            let _ = p.join();
        }

        let cancelled = registry()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&self.run_id)
            .map(|r| *r.cancelled.lock().unwrap_or_else(|p| p.into_inner()))
            .unwrap_or(false);
        let t = *self.totals.lock().unwrap_or_else(|p| p.into_inner());
        self.sink.emit(EVT_TEST_EXIT, json!({
            "run_id": self.run_id,
            "code": code,
            "cancelled": cancelled,
            // Mapped onto the Maven runner's four numbers so the panel's exit handling is one
            // path: libtest has no notion of "error" distinct from "failure", so that stays 0.
            "totals": {
                "run": t.passed + t.failed + t.ignored,
                "failures": t.failed,
                "errors": 0,
                "skipped": t.ignored,
            },
        }));

        RunEnd {
            code,
            cancelled,
            command: self.handle.command,
            label: self.handle.label,
            totals: Some((t.passed + t.failed + t.ignored, t.failed, t.ignored)),
        }
    }
}

/// Launch `cargo test` for `scope`, streaming targets, cases and output as events.
#[arbor_rpc::handler]
fn bennu_run_cargo_tests(ctx: &BennuState, args: RunArgs) -> Result<CargoRunHandle, String> {
    let run = start_cargo_run(ctx, &args)?;
    let handle = run.handle();
    std::thread::Builder::new()
        .name(format!("bennu-cargo-test-{}", handle.run_id))
        .spawn(move || {
            run.drive(None);
        })
        .map_err(|e| format!("spawn cargo test thread: {e}"))?;
    Ok(handle)
}

/// Running totals across every target of one run.
#[derive(Debug, Clone, Copy, Default)]
struct Totals {
    passed: u32,
    failed: u32,
    ignored: u32,
}

/// The targets cargo has announced so far, in order, with a condvar so the stdout pump can wait
/// for the one it needs instead of polling.
type Announced = Arc<(Mutex<Vec<ResolvedTarget>>, Condvar)>;

/// A `Running` line, resolved against the workspace.
#[derive(Debug, Clone, Serialize)]
struct ResolvedTarget {
    /// Cargo's own words, kept as the fallback label when the path could not be placed.
    desc: String,
    package: String,
    target: Option<TestTarget>,
}

/// Read cargo's stderr: log every line, announce each test binary, and report compilation.
fn spawn_stderr_pump<R: std::io::Read + Send + 'static>(
    reader: R,
    run_id: String,
    sink: Arc<dyn EventSink>,
    announced: Announced,
    ws: Arc<CargoWorkspace>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let buf = BufReader::new(reader);
        for line in buf.lines().map_while(Result::ok) {
            if let Some(t) = running_target(&line) {
                let resolved = resolve_target(&t, &ws);
                let (lock, cv) = &*announced;
                if let Ok(mut list) = lock.lock() {
                    list.push(resolved);
                }
                cv.notify_all();
            } else if let Some(crate_name) = compiling_crate(&line) {
                sink.progress(&format!("Compiling {crate_name}"), None, None);
                sink.emit(
                    EVT_CARGO_COMPILING,
                    json!({ "run_id": run_id, "crate": crate_name }),
                );
            }
            sink.emit(
                EVT_TEST_OUTPUT,
                json!({ "run_id": run_id, "stream": "stderr", "text": line }),
            );
        }
    })
}

/// Place a `Running` line in the workspace: which crate, and which of its targets.
///
/// Derived from the **source path** cargo prints rather than from the manifest's declared target
/// list, because that is the one thing always present and always right — a target cargo inferred
/// by convention has no manifest entry to match against.
fn resolve_target(t: &bennu_test::prelude::RunningTarget, ws: &CargoWorkspace) -> ResolvedTarget {
    // `Doc-tests <crate>` names the package outright and its target is known.
    if t.doc {
        return ResolvedTarget {
            desc: format!("doc-tests {}", t.desc),
            package: t.desc.clone(),
            target: Some(TestTarget::Doc),
        };
    }
    // Cargo prints the path relative to its working directory, which is the workspace root.
    if let Some(src) = &t.src {
        let src = src.replace('\\', "/");
        let mut best: Option<(&str, &str, bool)> = None;
        for c in &ws.crates {
            let prefix = c.rel_path.trim_matches('/');
            let inside = match prefix.is_empty() {
                true => Some(src.as_str()),
                false => src.strip_prefix(prefix).and_then(|r| r.strip_prefix('/')),
            };
            let Some(inside) = inside else { continue };
            let has_lib = c.targets.iter().any(|x| x.kind == "lib");
            if best.is_none() || best.is_some_and(|(_, prev, _)| inside.len() < prev.len()) {
                best = Some((c.name.as_str(), inside, has_lib));
            }
        }
        if let Some((package, rel, has_lib)) = best {
            let target = place_of(rel, package, has_lib).map(|p| p.target);
            return ResolvedTarget {
                desc: t.desc.clone(),
                package: package.to_string(),
                target,
            };
        }
    }
    // No path (an old cargo prints only the executable): the hashed file name still begins with
    // the crate name, which is enough to group the rows under the right crate.
    let package = t
        .exe
        .as_deref()
        .and_then(|e| e.rsplit(['/', '\\']).next())
        .and_then(|f| f.rsplit_once('-'))
        .map(|(name, _)| name.replace('_', "-"))
        .unwrap_or_default();
    ResolvedTarget { desc: t.desc.clone(), package, target: None }
}

/// Read libtest's stdout: open a group per block, emit a row per case, and close each group with
/// its summary.
fn spawn_stdout_pump<R: std::io::Read + Send + 'static>(
    reader: R,
    run_id: String,
    sink: Arc<dyn EventSink>,
    announced: Announced,
    totals: Arc<Mutex<Totals>>,
    collector: Option<Arc<crate::test_report::Collector>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut parser = LibtestParser::new();
        // Which block we are in — the index that pairs with cargo's `Running` lines.
        let blocks = AtomicUsize::new(0);
        let mut current: Option<(usize, ResolvedTarget)> = None;
        // Failure output arrives AFTER the `FAILED` line it belongs to, so the message is attached
        // by a second event keyed on the case path rather than by holding rows back.
        let buf = BufReader::new(reader);
        for line in buf.lines().map_while(Result::ok) {
            for ev in parser.line(&line) {
                match ev {
                    LibtestEvent::Start { count } => {
                        let index = blocks.fetch_add(1, Ordering::Relaxed);
                        let target = await_target(&announced, index);
                        sink.progress(
                            &format!("{} — {count} tests", target.desc),
                            None,
                            Some(count as u64),
                        );
                        sink.emit(EVT_CARGO_TARGET, json!({
                            "run_id": run_id,
                            "index": index,
                            "count": count,
                            "package": target.package,
                            "target": target.target,
                            "desc": target.desc,
                        }));
                        current = Some((index, target));
                    }
                    LibtestEvent::Case { path, status, note } => {
                        let (index, target) = match &current {
                            Some((i, t)) => (*i, t.clone()),
                            // A case before any block header should not happen; filing it under an
                            // unnamed group is still better than dropping the result.
                            None => (0, ResolvedTarget {
                                desc: "tests".to_string(),
                                package: String::new(),
                                target: None,
                            }),
                        };
                        if let Ok(mut t) = totals.lock() {
                            match status {
                                TestStatus::Passed => t.passed += 1,
                                TestStatus::Skipped => t.ignored += 1,
                                _ => t.failed += 1,
                            }
                        }
                        if let Some(collector) = &collector {
                            collector.case(&path, status);
                        }
                        let (module, name) = split_path(&path);
                        sink.emit(EVT_CARGO_CASE, json!({
                            "run_id": run_id,
                            "index": index,
                            "package": target.package,
                            "target": target.target,
                            "module": module,
                            "name": name,
                            "path": path,
                            "status": status,
                            "note": note,
                        }));
                    }
                    LibtestEvent::Failure { path, output } => {
                        let index = current.as_ref().map(|(i, _)| *i).unwrap_or(0);
                        if let Some(collector) = &collector {
                            collector.message(&path, &output);
                        }
                        sink.emit(EVT_CARGO_CASE, json!({
                            "run_id": run_id,
                            "index": index,
                            "path": path,
                            // No status: this event *amends* the row the verdict already created,
                            // and re-sending a verdict here would race with it.
                            "message": output,
                        }));
                    }
                    LibtestEvent::Result(r) => {
                        let index = current.as_ref().map(|(i, _)| *i).unwrap_or(0);
                        sink.progress(
                            &format!("{} passed, {} failed", r.passed, r.failed),
                            None,
                            None,
                        );
                        sink.emit(EVT_CARGO_TARGET_DONE, json!({
                            "run_id": run_id,
                            "index": index,
                            "result": r,
                        }));
                    }
                }
            }
            sink.emit(
                EVT_TEST_OUTPUT,
                json!({ "run_id": run_id, "stream": "stdout", "text": line }),
            );
        }
        // A run killed mid-failure still has a message worth showing.
        if let Some(LibtestEvent::Failure { path, output }) = parser.flush() {
            if let Some(collector) = &collector {
                collector.message(&path, &output);
            }
            sink.emit(EVT_CARGO_CASE, json!({
                "run_id": run_id,
                "path": path,
                "message": output,
            }));
        }
    })
}

/// Wait for cargo to have announced target `index`, up to [`TARGET_WAIT`].
///
/// The bound is what keeps a `Running` line we failed to recognise from stopping the pump: the
/// group ends up unnamed, and every case still lands in the panel.
fn await_target(announced: &Announced, index: usize) -> ResolvedTarget {
    let (lock, cv) = &**announced;
    let mut list = match lock.lock() {
        Ok(l) => l,
        Err(p) => p.into_inner(),
    };
    let deadline = std::time::Instant::now() + TARGET_WAIT;
    while list.len() <= index {
        let left = deadline.saturating_duration_since(std::time::Instant::now());
        if left.is_zero() {
            break;
        }
        let (guard, _) = match cv.wait_timeout(list, left) {
            Ok(pair) => pair,
            Err(p) => p.into_inner(),
        };
        list = guard;
    }
    list.get(index).cloned().unwrap_or(ResolvedTarget {
        desc: "tests".to_string(),
        package: String::new(),
        target: None,
    })
}

/// `util::tests::works` → (`util::tests`, `works`). A doc test's name has no `::` and is all name.
fn split_path(path: &str) -> (String, String) {
    match path.rsplit_once("::") {
        Some((module, name)) => (module.to_string(), name.to_string()),
        None => (String::new(), path.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_test::prelude::RunningTarget;

    fn workspace(crates: &[(&str, &str, bool)]) -> CargoWorkspace {
        let mut ws = read_workspace(Path::new("/definitely/not/a/workspace"));
        ws.crates = crates
            .iter()
            .map(|(name, rel, has_lib)| {
                let mut c = bennu_cargo::prelude::CargoCrate {
                    name: (*name).to_string(),
                    version: String::new(),
                    rel_path: (*rel).to_string(),
                    manifest: String::new(),
                    edition: String::new(),
                    description: String::new(),
                    is_root: rel.is_empty(),
                    publish: true,
                    targets: Vec::new(),
                    features: Vec::new(),
                    deps: 0,
                    dev_deps: 0,
                    build_deps: 0,
                };
                if *has_lib {
                    c.targets.push(bennu_cargo::prelude::CargoTarget {
                        name: (*name).to_string(),
                        kind: "lib".to_string(),
                        path: "src/lib.rs".to_string(),
                        declared: false,
                        proc_macro: false,
                        required_features: Vec::new(),
                    });
                }
                c
            })
            .collect();
        ws
    }

    /// The nested member must win over its ancestor, or every file in the tree belongs to the
    /// root crate and every test is filed under the wrong name.
    #[test]
    fn the_longest_matching_crate_owns_the_file() {
        let ws = workspace(&[("root", "", true), ("leaf", "crates/leaf", true)]);
        let got = crate_of(
            Path::new("/w"),
            "/w/crates/leaf/src/util.rs",
            &ws,
        )
        .expect("an owner");
        assert_eq!(got.0, "leaf");
        assert_eq!(got.1, "src/util.rs");
    }

    #[test]
    fn a_file_outside_every_crate_has_no_owner() {
        let ws = workspace(&[("leaf", "crates/leaf", true)]);
        assert!(crate_of(Path::new("/w"), "/w/scripts/gen.rs", &ws).is_none());
    }

    #[test]
    fn a_running_line_is_placed_in_its_crate_and_target() {
        let ws = workspace(&[("root", "", true), ("leaf", "crates/leaf", true)]);
        let t = RunningTarget {
            desc: "unittests crates/leaf/src/lib.rs".to_string(),
            src: Some("crates/leaf/src/lib.rs".to_string()),
            exe: Some("target/debug/deps/leaf-1a2b".to_string()),
            doc: false,
        };
        let r = resolve_target(&t, &ws);
        assert_eq!(r.package, "leaf");
        assert_eq!(r.target, Some(TestTarget::Lib));
    }

    #[test]
    fn an_integration_binary_is_placed_by_its_file_name() {
        let ws = workspace(&[("leaf", "crates/leaf", true)]);
        let t = RunningTarget {
            desc: "crates/leaf/tests/api.rs".to_string(),
            src: Some("crates/leaf/tests/api.rs".to_string()),
            exe: None,
            doc: false,
        };
        let r = resolve_target(&t, &ws);
        assert_eq!(r.package, "leaf");
        assert_eq!(r.target, Some(TestTarget::Test { name: "api".to_string() }));
    }

    #[test]
    fn doc_tests_name_their_package_outright() {
        let ws = workspace(&[("leaf", "crates/leaf", true)]);
        let t = RunningTarget {
            desc: "leaf".to_string(),
            src: None,
            exe: None,
            doc: true,
        };
        let r = resolve_target(&t, &ws);
        assert_eq!(r.package, "leaf");
        assert_eq!(r.target, Some(TestTarget::Doc));
    }

    /// An older cargo prints only the hashed executable. The crate name is still in it, which is
    /// enough to group the rows under the right crate.
    #[test]
    fn a_line_with_only_an_executable_still_names_the_crate() {
        let ws = workspace(&[("my-leaf", "crates/leaf", true)]);
        let t = RunningTarget {
            desc: "target/debug/deps/my_leaf-9f8e".to_string(),
            src: None,
            exe: Some("target/debug/deps/my_leaf-9f8e".to_string()),
            doc: false,
        };
        let r = resolve_target(&t, &ws);
        assert_eq!(r.package, "my-leaf");
        assert_eq!(r.target, None);
    }

    /// The pairing must not block forever on a target that never arrives — an unnamed group is a
    /// cosmetic loss, a pump that stops emitting looks like a hung run.
    #[test]
    fn waiting_for_a_target_that_never_comes_gives_up() {
        let announced: Announced = Arc::new((Mutex::new(Vec::new()), Condvar::new()));
        let started = std::time::Instant::now();
        // Cheat the wait down by asking for an index nobody will fill, with the real bound; the
        // test asserts it returns at all, and within a bound a human would not call a hang.
        let got = await_target(&announced, 0);
        assert_eq!(got.package, "");
        assert!(started.elapsed() < TARGET_WAIT * 2);
    }

    #[test]
    fn a_case_path_splits_into_module_and_name() {
        assert_eq!(split_path("util::tests::works"), ("util::tests".to_string(), "works".to_string()));
        assert_eq!(split_path("works"), (String::new(), "works".to_string()));
        assert_eq!(
            split_path("src/lib.rs - add (line 5)"),
            (String::new(), "src/lib.rs - add (line 5)".to_string())
        );
    }
}
