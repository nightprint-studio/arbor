//! `tests` domain — `bennu_discover_tests` / `bennu_run_tests` / `bennu_cancel_tests`: the
//! unit-test runner behind Bennu's Tests tool window.
//!
//! [`bennu_test`] is the pure half (what is a test, what to ask Maven for, what the report
//! says). This module is the moving half: it spawns `mvn test`, streams its output, and
//! turns the reports Surefire drops on disk into a tree that fills in **while the run is
//! still going**.
//!
//! ## How a live tree is possible at all
//!
//! Maven says nothing structured until it ends. But Surefire writes
//! `target/surefire-reports/TEST-<class>.xml` **as each class finishes**, so the run thread
//! watches those directories on a tick and emits each class the moment its file lands. Two
//! details make that reliable:
//!
//! - **Fresh is decided by a before-snapshot, not by a clock.** Every existing report is
//!   stamped (mtime + length) before the run starts; a file is ours when its stamp differs
//!   from the snapshot. Comparing against "now" instead would depend on filesystem timestamp
//!   granularity, and would re-report the previous run's results on a fast rerun.
//! - **A half-written file is a non-event.** [`parse_report`] returns `None` rather than
//!   erroring, so a file caught mid-write is simply read again on the next tick.
//!
//! The console is read for the one thing the reports can't give: which class is running
//! *right now*, so a class that takes forty seconds shows as running instead of as missing.
//!
//! ## Cancel really kills
//!
//! On Windows the child is `mvn.cmd`, whose JVM is a **grandchild** — killing the handle
//! leaves the tests running, still holding `target/`. So cancellation goes through
//! [`crate::child::kill_tree`] (the same one the app run uses). The run thread polls with
//! `try_wait` rather than blocking on `wait`, so the handle is free for the canceller to take.
//!
//! ## Why the run is not offline
//!
//! Unlike `bennu_build`'s `-o`, a test run resolves online. A project that has only ever
//! been *compiled* has no Surefire plugin and no test-scope jars in `~/.m2`, and an offline
//! run then dies on plugin resolution — an error that reads as a bug in Bennu rather than as
//! a missing download. Maven still prefers the local cache, so a warm `.m2` costs nothing.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use arbor_ipc::prelude::EventSink;
use arbor_process_ext::prelude::NoWindowExt;
use bennu_core::prelude::BennuState;
use bennu_intel::prelude::{collect_java, read_source_for_index};
use bennu_test::prelude::{
    discover_in_source, parse_report, plan, run_totals, running_class, RunTotals, TestClass,
    TestScope,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::build::{BuildGuard, BUSY_MSG};

// ── event topics (the wire contract for the FE) ────────────────────────────────

/// A line of `mvn test` output.
const EVT_TEST_OUTPUT: &str = "arbor://bennu/test-output";
/// Surefire announced a class — it is running now.
const EVT_TEST_RUNNING: &str = "arbor://bennu/test-running";
/// A class finished; carries its full parsed report.
const EVT_TEST_CLASS: &str = "arbor://bennu/test-class";
/// The run ended — exit code, whether it was cancelled, and Maven's own totals.
const EVT_TEST_EXIT: &str = "arbor://bennu/test-exit";

/// How often the run thread sweeps the report directories and checks on the child. Fast
/// enough that a class appears to land as it finishes, slow enough to be free.
const POLL: Duration = Duration::from_millis(400);

// ── bennu_discover_tests ───────────────────────────────────────────────────────

/// A discovered test class plus where it lives in the build — the Maven module, which
/// discovery cannot know (it reads text) and the tree needs (it groups by it, and a module
/// run is `-pl`).
#[derive(Debug, Clone, Serialize)]
pub struct DiscoveredTest {
    #[serde(flatten)]
    pub class: TestClass,
    /// The enclosing Maven module, relative to the project root. `None` for the root module.
    pub module: Option<String>,
}

/// Args for [`bennu_discover_tests`].
#[derive(Deserialize)]
pub struct DiscoverTestsArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// Scan only this file (absolute path) instead of the whole project, always freshly.
    /// This is the "run the test at the caret" path: the answer must reflect the file as it
    /// is now, not as a cache remembers it.
    #[serde(default)]
    pub file: Option<String>,
    /// Re-scan the project even if it has been scanned before (the panel's Refresh).
    #[serde(default)]
    pub force: bool,
}

/// Every test class in the project — or, with `file`, in that one file.
///
/// The source of truth is the file **on disk**, not the editor buffer, and deliberately so:
/// Maven compiles from disk, so a test discovered from unsaved text is a test the runner
/// cannot run. A discovery that disagrees with what will execute is worse than one that
/// lags by a save.
#[arbor_rpc::handler]
fn bennu_discover_tests(
    _ctx: &BennuState,
    args: DiscoverTestsArgs,
) -> Result<Vec<DiscoveredTest>, String> {
    let root = PathBuf::from(&args.root);
    let encoding = crate::index_service::resolve_index_encoding(&args.root);

    // Single file: never cached — this is the caret path, and it must be current.
    if let Some(file) = &args.file {
        return Ok(discover_file(&root, Path::new(file), &encoding));
    }

    if !args.force {
        if let Some(hit) = cache().read().ok().and_then(|c| c.get(&args.root).cloned()) {
            return Ok((*hit).clone());
        }
    }

    let mut paths = Vec::new();
    collect_java(&root, &mut paths);
    let found: Vec<DiscoveredTest> =
        paths.iter().flat_map(|p| discover_file(&root, p, &encoding)).collect();

    if let Ok(mut c) = cache().write() {
        c.insert(args.root.clone(), Arc::new(found.clone()));
    }
    Ok(found)
}

/// One file's test classes, decoded in the project's encoding (a legacy Cp1252 source still
/// yields its tests) and tagged with its module.
fn discover_file(root: &Path, file: &Path, encoding: &str) -> Vec<DiscoveredTest> {
    let Some(decoded) = read_source_for_index(file, encoding) else {
        return Vec::new();
    };
    let path = file.to_string_lossy().replace('\\', "/");
    let module = crate::main_classes::module_of(root, file);
    discover_in_source(&path, &decoded.text)
        .into_iter()
        .map(|class| DiscoveredTest { class, module: module.clone() })
        .collect()
}

/// Whole-project discovery results, per root. Test files change rarely and the walk is a
/// parse of every `.java` in the tree, so the panel opening must not pay for it twice.
fn cache() -> &'static RwLock<HashMap<String, Arc<Vec<DiscoveredTest>>>> {
    static CACHE: OnceLock<RwLock<HashMap<String, Arc<Vec<DiscoveredTest>>>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Drop a project's cached discovery — called when its index is rebuilt, so a newly written
/// test class doesn't need a restart to appear.
pub(crate) fn forget_discovery(root: &str) {
    if let Ok(mut c) = cache().write() {
        c.remove(root);
    }
}

// ── bennu_run_tests ────────────────────────────────────────────────────────────

/// Args for [`bennu_run_tests`].
#[derive(Deserialize)]
pub struct RunTestsArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// What to run: everything, a module, a set of classes, or individual cases.
    pub scope: TestScope,
}

/// The handle correlating a live run with its event stream.
#[derive(Debug, Clone, Serialize)]
pub struct TestRunHandle {
    pub run_id: String,
    /// What is being run, in words (`OrderTest.computesTotal`, `12 classes`, `all tests`).
    pub label: String,
    /// Set when the selection was too large to express on one command line and the run was
    /// widened. The panel must show it — the user asked for a subset and is getting a
    /// superset.
    pub widened: Option<String>,
}

/// Launch `mvn test` for `scope`, streaming output and per-class results. Returns as soon as
/// the child is up; everything after that arrives as events.
#[arbor_rpc::handler]
fn bennu_run_tests(ctx: &BennuState, args: RunTestsArgs) -> Result<TestRunHandle, String> {
    // Same lock as the build: two Maven processes on one tree fight over `target/`.
    let guard = BuildGuard::acquire().ok_or_else(|| BUSY_MSG.to_string())?;

    let root = PathBuf::from(&args.root);
    // Online, not `-o` — see the module doc.
    let plan = plan(&args.scope, false);
    let java_home = crate::build::resolve_java_home(&args.root);
    // The resolved launcher, not the bare `"mvn"`: on Windows Maven ships `mvn.cmd` and a
    // bare spawn only ever finds `mvn.exe`.
    let mvn = crate::build::resolve_mvn(&root);

    let mut cmd = Command::new(&mvn);
    cmd.current_dir(&root)
        .args(&plan.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    if let Some(jh) = &java_home {
        cmd.env("JAVA_HOME", jh);
    }
    cmd.no_window();

    // Name the launcher that was actually tried: "mvn not found" is unactionable when the
    // user can run `mvn` in a terminal — what they need to know is which path we looked at.
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not run Maven ({mvn}): {e}. Is it on PATH, or is MAVEN_HOME set?"))?;

    let run_id = next_run_id();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let sink = ctx.event_sink();
    // Stamp every report that exists BEFORE the run, so "changed since" needs no clock.
    let mut seen = snapshot_reports(&root);
    let totals: Arc<Mutex<Option<RunTotals>>> = Arc::new(Mutex::new(None));

    let child = Arc::new(Mutex::new(child));
    registry().lock().unwrap_or_else(|p| p.into_inner()).insert(
        run_id.clone(),
        LiveRun { child: child.clone(), cancelled: Arc::new(Mutex::new(false)) },
    );

    let thread_id = run_id.clone();
    let thread_totals = totals.clone();
    std::thread::Builder::new()
        .name(format!("bennu-test-{run_id}"))
        .spawn(move || {
            // The guard rides the thread: the lock is held for as long as Maven runs, not
            // just for as long as the handler that started it.
            let _guard = guard;
            let mut pumps = Vec::new();
            if let Some(out) = stdout {
                pumps.push(spawn_pump(out, "stdout", thread_id.clone(), sink.clone(), thread_totals.clone()));
            }
            if let Some(err) = stderr {
                pumps.push(spawn_pump(err, "stderr", thread_id.clone(), sink.clone(), thread_totals.clone()));
            }

            let dirs = report_dirs(&root);
            let code = loop {
                sweep_reports(&dirs, &mut seen, &thread_id, &sink);
                let status = child.lock().unwrap_or_else(|p| p.into_inner()).try_wait();
                match status {
                    Ok(Some(s)) => break s.code(),
                    // The child vanished (killed hard). Not an error to report — Stop is a
                    // normal way for a test run to end.
                    Err(_) => break None,
                    Ok(None) => std::thread::sleep(POLL),
                }
            };

            // Drain the pipes before the final sweep: a class whose report lands with the
            // last line of output must still make it into the tree.
            for p in pumps {
                let _ = p.join();
            }
            sweep_reports(&dirs, &mut seen, &thread_id, &sink);

            let cancelled = registry()
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .remove(&thread_id)
                .map(|r| *r.cancelled.lock().unwrap_or_else(|p| p.into_inner()))
                .unwrap_or(false);
            let totals = *thread_totals.lock().unwrap_or_else(|p| p.into_inner());
            sink.emit(EVT_TEST_EXIT, json!({
                "run_id": thread_id,
                "code": code,
                "cancelled": cancelled,
                "totals": totals.map(|t| json!({
                    "run": t.run, "failures": t.failures, "errors": t.errors, "skipped": t.skipped,
                })),
            }));
        })
        .map_err(|e| format!("spawn test thread: {e}"))?;

    Ok(TestRunHandle { run_id, label: plan.label, widened: plan.widened })
}

// ── bennu_cancel_tests ─────────────────────────────────────────────────────────

/// Args for [`bennu_cancel_tests`].
#[derive(Deserialize)]
pub struct CancelTestsArgs {
    pub run_id: String,
}

/// Stop a live test run — for real. `true` when a run was killed, `false` when the id is
/// unknown or it had already finished.
#[arbor_rpc::handler]
fn bennu_cancel_tests(_ctx: &BennuState, args: CancelTestsArgs) -> Result<bool, String> {
    let live = {
        let reg = registry().lock().unwrap_or_else(|p| p.into_inner());
        reg.get(&args.run_id).map(|r| (r.child.clone(), r.cancelled.clone()))
    };
    let Some((child, cancelled)) = live else { return Ok(false) };
    *cancelled.lock().unwrap_or_else(|p| p.into_inner()) = true;
    let mut child = child.lock().unwrap_or_else(|p| p.into_inner());
    crate::child::kill_tree(&mut child);
    Ok(true)
}

/// A run the canceller can reach.
struct LiveRun {
    child: Arc<Mutex<Child>>,
    /// Set by [`bennu_cancel_tests`], read by the run thread when it reports the exit — so
    /// the panel can say "stopped" rather than "failed with no exit code".
    cancelled: Arc<Mutex<bool>>,
}

fn registry() -> &'static Mutex<HashMap<String, LiveRun>> {
    static REG: OnceLock<Mutex<HashMap<String, LiveRun>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A monotonically-increasing, process-unique run id — same shape as `bennu_run`'s.
fn next_run_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos =
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("test-{n}-{nanos}")
}

// ── output pump ────────────────────────────────────────────────────────────────

/// Read `reader` line by line: every line goes to the log, a `Running …` line also raises
/// the class it names, and the summary line is kept for the exit event.
fn spawn_pump<R: std::io::Read + Send + 'static>(
    reader: R,
    stream: &'static str,
    run_id: String,
    sink: Arc<dyn EventSink>,
    totals: Arc<Mutex<Option<RunTotals>>>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let buf = BufReader::new(reader);
        for line in buf.lines().map_while(Result::ok) {
            if let Some(class) = running_class(&line) {
                sink.emit(EVT_TEST_RUNNING, json!({ "run_id": run_id, "classname": class }));
            }
            if let Some(t) = run_totals(&line) {
                *totals.lock().unwrap_or_else(|p| p.into_inner()) = Some(t);
            }
            sink.emit(EVT_TEST_OUTPUT, json!({ "run_id": run_id, "stream": stream, "text": line }));
        }
    })
}

// ── report watching ────────────────────────────────────────────────────────────

/// A report file's identity for change detection: modification time and length. Both,
/// because a rerun of the same class can produce a file of identical length, and a coarse
/// filesystem clock can produce an identical mtime.
type Stamp = (u128, u64);

/// Every `target/surefire-reports` directory in the project — one per Maven module.
///
/// Derived from where the poms are rather than by walking for the directory itself: the
/// directories do not exist yet on a first-ever test run, and a watcher that only knows the
/// paths that existed at startup would report nothing at all that first time.
fn report_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![root.join("target").join("surefire-reports")];
    for module in module_dirs(root) {
        dirs.push(module.join("target").join("surefire-reports"));
    }
    dirs.sort();
    dirs.dedup();
    dirs
}

/// Directories under `root` holding a `pom.xml` (the Maven modules), skipping `target` and
/// hidden trees.
fn module_dirs(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if !p.is_dir() {
                continue;
            }
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "target" || name == "node_modules" || name.starts_with('.') {
                continue;
            }
            if p.join("pom.xml").is_file() {
                out.push(p.clone());
            }
            stack.push(p);
        }
    }
    out
}

/// Stamp every report that already exists, so the sweep can tell this run's output from the
/// last one's without consulting a clock.
fn snapshot_reports(root: &Path) -> HashMap<PathBuf, Stamp> {
    let mut seen = HashMap::new();
    for dir in report_dirs(root) {
        for (path, stamp) in report_files(&dir) {
            seen.insert(path, stamp);
        }
    }
    seen
}

/// The `TEST-*.xml` files in one reports directory, with their stamps. Empty (not an error)
/// when the directory doesn't exist — which is the normal state before the first run.
fn report_files(dir: &Path) -> Vec<(PathBuf, Stamp)> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    rd.flatten()
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_name()?.to_str()?;
            if !name.starts_with("TEST-") || !name.ends_with(".xml") {
                return None;
            }
            let meta = e.metadata().ok()?;
            let mtime = meta
                .modified()
                .ok()
                .and_then(|m| m.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_nanos())
                .unwrap_or(0);
            Some((path, (mtime, meta.len())))
        })
        .collect()
}

/// Emit every report that is new or has changed since `seen`, updating `seen` as it goes.
///
/// A file that fails to parse is left OUT of `seen`, which is what makes a half-written file
/// self-healing: it is simply retried on the next tick, when Surefire has finished with it.
fn sweep_reports(
    dirs: &[PathBuf],
    seen: &mut HashMap<PathBuf, Stamp>,
    run_id: &str,
    sink: &Arc<dyn EventSink>,
) {
    for dir in dirs {
        for (path, stamp) in report_files(dir) {
            if seen.get(&path) == Some(&stamp) {
                continue;
            }
            let Ok(xml) = std::fs::read_to_string(&path) else { continue };
            let Some(result) = parse_report(&xml) else { continue };
            seen.insert(path, stamp);
            sink.emit(EVT_TEST_CLASS, json!({ "run_id": run_id, "result": result }));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The root's own reports directory is always watched, whether or not it exists yet —
    /// a first-ever run has no `target/` at all, and a watcher built from what is on disk
    /// would see nothing that first time.
    #[test]
    fn report_dirs_include_the_root_before_target_exists() {
        let root = Path::new(if cfg!(windows) { r"C:\nope\proj" } else { "/nope/proj" });
        let dirs = report_dirs(root);
        assert_eq!(dirs.len(), 1);
        assert!(dirs[0].ends_with("surefire-reports"));
    }

    /// A missing directory is empty, not an error: that is the state before the first run.
    #[test]
    fn report_files_of_a_missing_dir_is_empty() {
        let dir = Path::new(if cfg!(windows) { r"C:\nope\reports" } else { "/nope/reports" });
        assert!(report_files(dir).is_empty());
    }

    #[test]
    fn run_ids_are_unique() {
        assert_ne!(next_run_id(), next_run_id());
    }
}
