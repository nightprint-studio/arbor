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
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
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
use crate::test_report::RunEnd;

// ── event topics (the wire contract for the FE) ────────────────────────────────

/// A line of test-runner output. Shared with the cargo runner: the console under the tree shows
/// the run's output whichever build system produced it, so one topic feeds it.
pub(crate) const EVT_TEST_OUTPUT: &str = "arbor://bennu/test-output";
/// Surefire announced a class — it is running now.
const EVT_TEST_RUNNING: &str = "arbor://bennu/test-running";
/// A class finished; carries its full parsed report.
const EVT_TEST_CLASS: &str = "arbor://bennu/test-class";
/// The run ended — exit code, whether it was cancelled, and the runner's own totals. Shared with
/// the cargo runner, which maps libtest's counts onto the same four numbers.
pub(crate) const EVT_TEST_EXIT: &str = "arbor://bennu/test-exit";

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
pub(crate) fn bennu_discover_tests(
    _ctx: &BennuState,
    args: DiscoverTestsArgs,
) -> Result<Vec<DiscoveredTest>, String> {
    let root = PathBuf::from(&args.root);
    let encoding = crate::index_service::encoding_plan(&args.root);

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
fn discover_file(
    root: &Path,
    file: &Path,
    encoding: &bennu_project::prelude::EncodingPlan,
) -> Vec<DiscoveredTest> {
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

/// Args for [`bennu_test_selection_pinned`].
#[derive(Deserialize)]
pub struct PinnedArgs {
    /// Absolute path to the project root.
    pub root: String,
}

/// The literal `<test>` this project's pom pins on the Surefire plugin, when it pins one — the
/// answer to "will running one test actually run one test here".
///
/// `None` for every project that does not, which is nearly all of them, and also for the property
/// spellings: those are steerable, and a warning about them would be a warning about the case that
/// works. Read before anything is run, because the whole point is to say it before the click
/// rather than after the wrong run.
#[arbor_rpc::handler]
fn bennu_test_selection_pinned(
    _ctx: &BennuState,
    args: PinnedArgs,
) -> Result<Option<String>, String> {
    Ok(match surefire_pinned_test(Path::new(&args.root)) {
        Some(SurefireTest::Literal(pinned)) => Some(pinned),
        _ => None,
    })
}

/// Args for [`bennu_run_tests`].
#[derive(Deserialize)]
pub struct RunTestsArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// What to run: everything, a module, a set of classes, or individual cases.
    pub scope: TestScope,
    /// Run under the **debugger**: the forked test JVM connects back to a listener opened here and
    /// suspends until it does, so a breakpoint set before the run is honoured.
    #[serde(default)]
    pub debug: bool,
}

/// The handle correlating a live run with its event stream.
#[derive(Debug, Clone, Serialize)]
pub struct TestRunHandle {
    pub run_id: String,
    /// What is being run, in words (`OrderTest.computesTotal`, `12 classes`, `all tests`).
    pub label: String,
    /// Whether the run is under the debugger — the panel shows a different verb, and Stop has a
    /// session to end as well as a process.
    #[serde(default)]
    pub debugging: bool,
    /// Set when the selection was too large to express on one command line and the run was
    /// widened. The panel must show it — the user asked for a subset and is getting a
    /// superset.
    pub widened: Option<String>,
}

/// A launched Maven run, before anyone has waited on it.
///
/// Split from [`bennu_run_tests`] so the same run can be driven two ways: on a thread, for
/// a caller that wants the handle and will listen for the events, or **inline**, for a
/// caller that wants the answer. Both drive the identical loop — one pump per stream, one
/// report sweep per tick — because a second copy of it would be a second place for a class
/// that lands with the last line of output to go missing.
pub(crate) struct MavenRun {
    handle: TestRunHandle,
    command: String,
    guard: BuildGuard,
    child: Arc<Mutex<Child>>,
    stdout: Option<std::process::ChildStdout>,
    stderr: Option<std::process::ChildStderr>,
    root: PathBuf,
    run_id: String,
    sink: Arc<dyn EventSink>,
    seen: HashMap<PathBuf, Stamp>,
    classes: crate::log::ClassMap,
    totals: Arc<Mutex<Option<RunTotals>>>,
}

/// Spawn `mvn test` for `scope` and register it, without waiting for anything.
/// What a pom writes for Surefire's `<test>`, when it writes anything.
///
/// The difference decides whether a selection can be honoured at all. Measured on Surefire 3.5.6:
///
/// | written as | `-Dtest=X` on the command line | what the mojo runs |
/// |---|---|---|
/// | `<test>TestSuite</test>` | passed | `TestSuite` — the selection is discarded |
/// | `<test>${test}</test>` | passed | `X` |
/// | `<test>${suite}</test>` + `-Dsuite=X` | passed | `X` |
///
/// Maven gives a value written in the pom precedence over the user property that names it, so a
/// literal cannot be overridden from a command line at all. A property expression can — by setting
/// **that** property, whatever it is called.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SurefireTest {
    /// A literal. Nothing on a command line can move it.
    Literal(String),
    /// A single `${name}`. Setting `name` steers the run, and leaving it alone keeps whatever the
    /// pom defaults it to — so a plain `mvn test` still does what the team meant.
    Property(String),
}

/// What the poms under `root` write for Surefire's `<test>`.
///
/// Read from the module poms rather than from an effective pom: a `<configuration>` inherited
/// through `<pluginManagement>` is the same problem, and both spellings live in the text.
fn surefire_pinned_test(root: &Path) -> Option<SurefireTest> {
    /// Matches the reactor depth the rest of the classpath work walks.
    const MAX_DEPTH: usize = 6;
    fn walk(dir: &Path, depth_left: usize) -> Option<SurefireTest> {
        if let Ok(xml) = std::fs::read_to_string(dir.join("pom.xml")) {
            if let Some(pinned) = pinned_in(&xml) {
                return Some(pinned);
            }
        }
        if depth_left == 0 {
            return None;
        }
        let entries = std::fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            if !entry.file_type().ok()?.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            // `target/` holds a copy of nothing anybody configures, and a dot-directory is not a
            // module.
            if name == "target" || name.starts_with('.') || name == "node_modules" {
                continue;
            }
            if let Some(pinned) = walk(&entry.path(), depth_left - 1) {
                return Some(pinned);
            }
        }
        None
    }
    walk(root, MAX_DEPTH)
}

/// The `<test>` inside a `maven-surefire-plugin` block of this pom's text.
fn pinned_in(xml: &str) -> Option<SurefireTest> {
    let at = xml.find("maven-surefire-plugin")?;
    // Bounded to the plugin's own element: a `<test>` further down the file belongs to something
    // else, and the plugin block is never megabytes long.
    let rest = &xml[at..];
    let end = rest.find("</plugin>").unwrap_or(rest.len());
    let block = &rest[..end];
    let open = block.find("<test>")?;
    let close = block[open..].find("</test>")? + open;
    let value = block[open + "<test>".len()..close].trim();
    if value.is_empty() {
        return None;
    }
    if let Some(name) = single_property(value) {
        return Some(SurefireTest::Property(name));
    }
    // Anything else containing a `${` is a value built from several parts, and no single property
    // steers it. Nothing is claimed about it in either direction: saying it is pinned would be a
    // guess, and so would saying it is not.
    if value.contains("${") {
        return None;
    }
    Some(SurefireTest::Literal(value.to_string()))
}

/// Whether any pom under `root` pins Surefire's `<forkCount>` to zero — tests in Maven's own JVM.
fn forkcount_zero(root: &Path) -> bool {
    const MAX_DEPTH: usize = 6;
    fn walk(dir: &Path, depth_left: usize) -> bool {
        if let Ok(xml) = std::fs::read_to_string(dir.join("pom.xml")) {
            if forkcount_zero_in(&xml) {
                return true;
            }
        }
        if depth_left == 0 {
            return false;
        }
        let Ok(entries) = std::fs::read_dir(dir) else { return false };
        for entry in entries.flatten() {
            let Ok(ft) = entry.file_type() else { continue };
            if !ft.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == "target" || name.starts_with('.') || name == "node_modules" {
                continue;
            }
            if walk(&entry.path(), depth_left - 1) {
                return true;
            }
        }
        false
    }
    walk(root, MAX_DEPTH)
}

/// `<forkCount>0</forkCount>` inside a `maven-surefire-plugin` block of this pom's text.
fn forkcount_zero_in(xml: &str) -> bool {
    let Some(at) = xml.find("maven-surefire-plugin") else { return false };
    let rest = &xml[at..];
    let end = rest.find("</plugin>").unwrap_or(rest.len());
    let block = &rest[..end];
    let Some(open) = block.find("<forkCount>") else { return false };
    let Some(close) = block[open..].find("</forkCount>").map(|i| i + open) else { return false };
    block[open + "<forkCount>".len()..close].trim() == "0"
}

/// `"${suite}"` → `Some("suite")`. `None` for anything that is not exactly one property
/// expression.
fn single_property(value: &str) -> Option<String> {
    let inner = value.strip_prefix("${")?.strip_suffix('}')?;
    // One expression, not two run together (`${a}${b}`) — and not an empty `${}`.
    if inner.is_empty() || inner.contains('$') || inner.contains('{') || inner.contains('}') {
        return None;
    }
    Some(inner.to_string())
}

pub(crate) fn start_maven_run(ctx: &BennuState, args: &RunTestsArgs) -> Result<MavenRun, String> {
    // Same lock as the build: two Maven processes on one tree fight over `target/`.
    let guard = BuildGuard::acquire().ok_or_else(|| BUSY_MSG.to_string())?;

    let root = PathBuf::from(&args.root);
    // Online, not `-o` — see the module doc.
    let mut plan = plan(&args.scope, false);
    // A pom that pins Surefire's `<test>` makes the selection we just built inert. Maven gives a
    // plugin parameter written in the POM precedence over the user property that names it, so the
    // `-Dtest=…` in the plan is read and discarded, and the run does whatever the POM said — the
    // whole suite, usually. Measured on Surefire 3.5.6: with `<test>TestSuite</test>` the mojo's
    // effective `test` is `TestSuite` however the command line is written; with `<test>${test}</test>`
    // the command line reaches it. There is no CLI lever for the first, so the only honest thing is
    // to say what will actually run rather than run it silently and look broken.
    if let Some(filter) =
        plan.args.iter().find_map(|a| a.strip_prefix("-Dtest=")).map(str::to_string)
    {
        match surefire_pinned_test(&root) {
            // Steerable after all: the pom names a property, so set THAT one. Left alone it keeps
            // whatever the pom defaults it to, which is why a plain `mvn test` still runs what the
            // team meant — the selection only exists while something is asking for one.
            Some(SurefireTest::Property(name)) if name != "test" => {
                plan.args.push(format!("-D{name}={filter}"));
            }
            Some(SurefireTest::Property(_)) => {}
            // Not steerable at all. Maven gives a value written in the pom precedence over the user
            // property that names it, and there is no command line that changes that — so the only
            // honest thing is to say what will actually run.
            Some(SurefireTest::Literal(pinned)) => {
                plan.widened = Some(format!(
                    "This project's pom pins the Surefire plugin to <test>{pinned}</test>, so the \
                     selection cannot take effect — Maven gives a value written in the pom \
                     precedence over the -Dtest it is named by, and no command line changes that. \
                     Write it as <test>${{test}}</test>, or as <test>${{any.property}}</test> with \
                     the suite as that property's default: either spelling keeps a plain `mvn test` \
                     running the suite and lets one class or one case be chosen here."
                ));
            }
            None => {}
        }
    }
    // Under the debugger, the port has to be listening BEFORE the fork exists: the agent dials out
    // during VM initialization and aborts the launch if nothing answers. A port that cannot be
    // bound degrades to an ordinary run — tests that run without the debugger beat tests that do
    // not run. The same shape the application's Run/Debug uses (`crate::build`), for the same
    // reason: `server=n` means the JVM calls US, so there is no fixed port to collide on and no
    // race between the process starting and something attaching to it.
    if args.debug {
        // `forkCount=0` runs the tests inside Maven's OWN JVM, so there is no fork to put an agent
        // on and `maven.surefire.debug` does nothing at all. Refused rather than run: a debug run
        // that silently is not debugging wastes the whole run before you find out.
        if forkcount_zero(&root) {
            return Err(
                "This project sets <forkCount>0</forkCount> on the Surefire plugin, so the tests \
                 run inside Maven's own JVM and there is no forked process to attach to. Set it to \
                 1 to debug from here, or debug Maven itself with mvnDebug."
                    .to_string(),
            );
        }
    }
    let launch = args.debug.then(crate::debug::prepare).flatten();
    if let Some(l) = &launch {
        // Surefire hands this to the forked JVM verbatim, and it is a plain `${maven.surefire.debug}`
        // — a property expression, so the command line always reaches it. (Unlike `<test>`, which is
        // the trap this same file works around above.)
        plan.args.push(format!("-Dmaven.surefire.debug={}", crate::debug::agent_arg(l.port, true)));
        // One fork, because one listener accepts one connection. Surefire's own default is 1, but a
        // project that raised it would otherwise start several JVMs all dialling the same port and
        // debug whichever won the race.
        plan.args.push("-DforkCount=1".to_string());
    }
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
    // Its own process group, so Stop reaches the JVM Surefire forks to run the tests in — a
    // grandchild, and the process that actually matters. See `child::own_group`.
    crate::child::own_group(&mut cmd);

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
    let seen = snapshot_reports(&root);

    let child = Arc::new(Mutex::new(child));
    registry().lock().unwrap_or_else(|p| p.into_inner()).insert(
        run_id.clone(),
        LiveRun { child: child.clone(), cancelled: Arc::new(Mutex::new(false)) },
    );

    // Keyed by the RUN id, so the console tab, the debugger and Stop are the same thing to
    // everything that has to correlate them — which is why it can only start once the id exists.
    let debugging = launch.is_some();
    if let Some(launch) = launch {
        crate::debug::start(run_id.clone(), args.root.clone(), launch, sink.clone());
    }

    Ok(MavenRun {
        handle: TestRunHandle {
            run_id: run_id.clone(),
            label: plan.label,
            widened: plan.widened,
            debugging,
        },
        command: format!("{mvn} {}", plan.args.join(" ")),
        guard,
        child,
        stdout,
        stderr,
        root,
        run_id,
        sink,
        seen,
        // One lookup for the whole run — every frame of every failure resolves through it.
        classes: crate::log::class_map(&args.root),
        totals: Arc::new(Mutex::new(None)),
    })
}

impl MavenRun {
    /// The handle the streaming caller returns before any of this has happened.
    pub(crate) fn handle(&self) -> TestRunHandle {
        self.handle.clone()
    }

    /// Pump the output, sweep the reports until the child exits, emit the exit event.
    ///
    /// Consumes the run and blocks for as long as Maven does. `collector`, when given, is
    /// filled with every class report as it lands — the events are emitted either way, so
    /// waiting for the answer never costs the panel its live tree.
    pub(crate) fn drive(mut self, collector: Option<&crate::test_report::Collector>) -> RunEnd {
        // The guard rides the run: the lock is held for as long as Maven does, not just for
        // as long as the handler that started it.
        let _guard = self.guard;
        self.sink.progress(&format!("mvn test — {}", self.handle.label), None, None);
        let mut pumps = Vec::new();
        if let Some(out) = self.stdout.take() {
            pumps.push(spawn_pump(
                out,
                "stdout",
                self.run_id.clone(),
                self.sink.clone(),
                self.totals.clone(),
                self.classes.clone(),
            ));
        }
        if let Some(err) = self.stderr.take() {
            pumps.push(spawn_pump(
                err,
                "stderr",
                self.run_id.clone(),
                self.sink.clone(),
                self.totals.clone(),
                self.classes.clone(),
            ));
        }

        let dirs = report_dirs(&self.root);
        let code = loop {
            sweep_reports(&dirs, &mut self.seen, &self.run_id, &self.sink, collector);
            let status = self.child.lock().unwrap_or_else(|p| p.into_inner()).try_wait();
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
        sweep_reports(&dirs, &mut self.seen, &self.run_id, &self.sink, collector);

        let cancelled = registry()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .remove(&self.run_id)
            .map(|r| *r.cancelled.lock().unwrap_or_else(|p| p.into_inner()))
            .unwrap_or(false);
        let totals = *self.totals.lock().unwrap_or_else(|p| p.into_inner());
        self.sink.emit(EVT_TEST_EXIT, json!({
            "run_id": self.run_id,
            "code": code,
            "cancelled": cancelled,
            "totals": totals.map(|t| json!({
                "run": t.run, "failures": t.failures, "errors": t.errors, "skipped": t.skipped,
            })),
        }));

        RunEnd {
            code,
            cancelled,
            command: self.command,
            label: self.handle.label,
            totals: totals.map(|t| (t.run, t.failures + t.errors, t.skipped)),
        }
    }
}

/// Launch `mvn test` for `scope`, streaming output and per-class results. Returns as soon as
/// the child is up; everything after that arrives as events.
#[arbor_rpc::handler]
fn bennu_run_tests(ctx: &BennuState, args: RunTestsArgs) -> Result<TestRunHandle, String> {
    let run = start_maven_run(ctx, &args)?;
    let handle = run.handle();
    std::thread::Builder::new()
        .name(format!("bennu-test-{}", handle.run_id))
        .spawn(move || {
            run.drive(None);
        })
        .map_err(|e| format!("spawn test thread: {e}"))?;
    Ok(handle)
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
    Ok(cancel_run(&args.run_id))
}

/// Kill a live run by id. `false` when the id is unknown or the run had already finished —
/// which is how a watchdog tells "I stopped it" from "it beat me to it", and the reason a
/// late deadline never reports a healthy run as timed out.
///
/// On Windows the child is `mvn.cmd`/`cargo.exe` and the real work is a grandchild, so this
/// goes through [`crate::child::kill_tree`]: killing the handle alone would leave the tests
/// running and still holding `target/`.
pub(crate) fn cancel_run(run_id: &str) -> bool {
    let live = {
        let reg = registry().lock().unwrap_or_else(|p| p.into_inner());
        reg.get(run_id).map(|r| (r.child.clone(), r.cancelled.clone()))
    };
    let Some((child, cancelled)) = live else { return false };
    *cancelled.lock().unwrap_or_else(|p| p.into_inner()) = true;
    {
        let mut child = child.lock().unwrap_or_else(|p| p.into_inner());
        crate::child::kill_tree(&mut child);
    }
    // A debugged run holds a second thing: the JDWP session, keyed by this same id. The socket
    // closing would end it on its own — the forked JVM is dead — but "would" is doing a lot of
    // work there, and until the reader thread notices, the editor is showing a stopped run whose
    // debugger is still paused at a breakpoint. Closing this end says so now, and is a no-op when
    // the run was not being debugged.
    if let Ok(session) = crate::debug_backend::get(run_id) {
        // On a thread of its own, because `detach` is a *polite* ending — it asks the VM to let go
        // and waits for the answer, and we have just killed that VM. The wait unblocks the moment
        // the socket teardown reaches the reader thread, which is immediate in practice and is
        // still not something the Stop button should be able to sit behind.
        std::thread::spawn(move || {
            let _ = session.detach();
        });
    }
    true
}

// ── The deadline ───────────────────────────────────────────────────────────────

/// A watchdog that kills a run which outlives its limit.
///
/// ## Perché esiste
///
/// Because the caller that *waits* has no hand on the stop button. The editor's runners
/// return the moment the child is up and a human watches the panel; the agent facade drives
/// the same run to its end, and a test that never terminates turns that into a call that
/// never answers — the failure mode that looks exactly like the tool being broken, for as
/// long as anyone is willing to wait.
///
/// ⚠️ **It is not a per-test timeout.** `cargo test` runs a target's cases in one process,
/// so there is no way from out here to kill the one case that hung and let the rest finish —
/// the whole run goes. `cargo nextest` gives each test its own process and can do exactly
/// that; wiring it up means a second output parser, and until then this is the honest floor:
/// the call comes back, and it says it was stopped rather than reporting a green.
pub(crate) struct Deadline {
    fired: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
    limit: Duration,
}

impl Deadline {
    /// Start the watchdog for `run_id`. A failure to spawn the thread is not fatal: the run
    /// simply has no deadline, which is where we were before.
    pub(crate) fn arm(run_id: String, limit: Duration) -> Self {
        let fired = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let (f, s) = (fired.clone(), stop.clone());
        let thread = std::thread::Builder::new()
            .name(format!("bennu-deadline-{run_id}"))
            .spawn(move || {
                // Sliced rather than one long sleep: a finished run must not leave a thread
                // parked for the rest of the limit, and ten minutes is a long time to park.
                let tick = Duration::from_millis(200);
                let mut waited = Duration::ZERO;
                while waited < limit {
                    if s.load(Ordering::Relaxed) {
                        return;
                    }
                    std::thread::sleep(tick);
                    waited += tick;
                }
                if !s.load(Ordering::Relaxed) && cancel_run(&run_id) {
                    f.store(true, Ordering::Relaxed);
                }
            })
            .ok();
        Self { fired, stop, thread, limit }
    }

    /// Stop watching. `Some(limit)` when the watchdog had already killed the run — the
    /// caller turns that into the note that says so.
    pub(crate) fn disarm(mut self) -> Option<Duration> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        self.fired.load(Ordering::Relaxed).then_some(self.limit)
    }
}

/// A run the canceller can reach.
pub(crate) struct LiveRun {
    pub(crate) child: Arc<Mutex<Child>>,
    /// Set by [`bennu_cancel_tests`], read by the run thread when it reports the exit — so
    /// the panel can say "stopped" rather than "failed with no exit code".
    pub(crate) cancelled: Arc<Mutex<bool>>,
}

/// Live test runs, keyed by run id.
///
/// Shared with [`crate::cargo_tests`] deliberately: Stop is **one** verb to the panel, and a
/// second registry would mean a second cancel handler the frontend had to choose between — with
/// the wrong choice looking exactly like a Stop button that does nothing.
pub(crate) fn registry() -> &'static Mutex<HashMap<String, LiveRun>> {
    static REG: OnceLock<Mutex<HashMap<String, LiveRun>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A monotonically-increasing, process-unique run id — same shape as `bennu_run`'s.
pub(crate) fn next_run_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos =
        SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("test-{n}-{nanos}")
}

// ── output pump ────────────────────────────────────────────────────────────────

/// Read `reader` line by line: every line goes to the log, a `Running …` line also raises
/// the class it names, and the summary line is kept for the exit event.
///
/// The log is interpreted on the way out, like the Run console's ([`crate::log`]) — a test
/// run is the log most worth reading as something other than text, since what you are
/// looking for in it is a stack trace, and the frames of one are links.
fn spawn_pump<R: std::io::Read + Send + 'static>(
    reader: R,
    stream: &'static str,
    run_id: String,
    sink: Arc<dyn EventSink>,
    totals: Arc<Mutex<Option<RunTotals>>>,
    classes: crate::log::ClassMap,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut log = crate::log::LogAnnotator::new(classes);
        let buf = BufReader::new(reader);
        for line in buf.lines().map_while(Result::ok) {
            if let Some(class) = running_class(&line) {
                sink.progress(&format!("Running {class}"), None, None);
                sink.emit(EVT_TEST_RUNNING, json!({ "run_id": run_id, "classname": class }));
            }
            if let Some(t) = run_totals(&line) {
                *totals.lock().unwrap_or_else(|p| p.into_inner()) = Some(t);
            }
            let mut payload = log.line(&line);
            if let Some(map) = payload.as_object_mut() {
                map.insert("run_id".into(), json!(run_id));
                map.insert("stream".into(), json!(stream));
            }
            sink.emit(EVT_TEST_OUTPUT, payload);
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
    collector: Option<&crate::test_report::Collector>,
) {
    for dir in dirs {
        for (path, stamp) in report_files(dir) {
            if seen.get(&path) == Some(&stamp) {
                continue;
            }
            let Ok(xml) = std::fs::read_to_string(&path) else { continue };
            let Some(result) = parse_report(&xml) else { continue };
            seen.insert(path, stamp);
            if let Some(collector) = collector {
                collector.class(&result);
            }
            sink.progress(
                &match result.is_bad() {
                    true => format!(
                        "{}: {} failed of {}",
                        result.classname,
                        result.failures + result.errors,
                        result.total
                    ),
                    false => format!("{}: {} passed", result.classname, result.total),
                },
                None,
                None,
            );
            sink.emit(EVT_TEST_CLASS, json!({ "run_id": run_id, "result": result }));
        }
    }
}

#[cfg(test)]
mod surefire_pin_tests {
    use super::{forkcount_zero_in, pinned_in, SurefireTest};

    const PINNED: &str = r#"<project><build><plugins>
        <plugin>
          <groupId>org.apache.maven.plugins</groupId>
          <artifactId>maven-surefire-plugin</artifactId>
          <version>3.5.6</version>
          <configuration><test>TestSuite</test></configuration>
        </plugin>
      </plugins></build></project>"#;

    /// The reported case: the pom pins the selector, so `-Dtest=` is read and discarded and the
    /// whole suite runs whatever you clicked. Nothing on a command line can move it.
    #[test]
    fn a_literal_test_selector_is_a_literal() {
        assert_eq!(pinned_in(PINNED), Some(SurefireTest::Literal("TestSuite".into())));
    }

    /// The spelling that works out of the box: `${test}` is the property `-Dtest` already sets.
    #[test]
    fn the_test_property_is_recognised_as_a_property() {
        let xml = PINNED.replace("<test>TestSuite</test>", "<test>${test}</test>");
        assert_eq!(pinned_in(&xml), Some(SurefireTest::Property("test".into())));
    }

    /// And ANY property works, which is the answer to "can we keep the suite as the default?" —
    /// yes: the pom defaults it, so a plain `mvn test` runs the suite, and setting that property
    /// steers a single run. Measured on Surefire 3.5.6.
    #[test]
    fn any_property_name_is_recognised_and_can_be_driven() {
        let xml = PINNED.replace("<test>TestSuite</test>", "<test>${suite}</test>");
        assert_eq!(pinned_in(&xml), Some(SurefireTest::Property("suite".into())));
    }

    /// A value built from several parts is steered by no single property, and claiming it is
    /// pinned would be as much a guess as claiming it is not.
    #[test]
    fn a_composed_value_claims_nothing() {
        let xml = PINNED.replace("<test>TestSuite</test>", "<test>${a}${b}</test>");
        assert_eq!(pinned_in(&xml), None);
        let xml = PINNED.replace("<test>TestSuite</test>", "<test>Pre${a}</test>");
        assert_eq!(pinned_in(&xml), None);
    }

    /// `forkCount=0` means the tests run in Maven's own JVM, so there is no fork to put an agent
    /// on — a debug run is refused rather than run without one.
    #[test]
    fn a_zero_fork_count_is_recognised() {
        let xml = PINNED.replace(
            "<configuration>",
            "<configuration><forkCount>0</forkCount>",
        );
        assert!(forkcount_zero_in(&xml));
    }

    #[test]
    fn an_ordinary_fork_count_is_not_zero() {
        let xml = PINNED.replace(
            "<configuration>",
            "<configuration><forkCount>1</forkCount>",
        );
        assert!(!forkcount_zero_in(&xml));
        assert!(!forkcount_zero_in(PINNED));
    }

    #[test]
    fn a_pom_that_does_not_configure_surefire_pins_nothing() {
        assert_eq!(pinned_in("<project><build><plugins></plugins></build></project>"), None);
    }

    /// A `<test>` outside the plugin's own element is not Surefire's — the scan stops at
    /// `</plugin>`.
    #[test]
    fn a_test_element_belonging_to_something_else_is_ignored() {
        let xml = PINNED.replace("<configuration><test>TestSuite</test></configuration>", "")
            + "<other><test>Nope</test></other>";
        assert_eq!(pinned_in(&xml), None);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⚠️ **Un deadline disarmato non deve sparare.** È il caso normale — la corsa finisce
    /// prima — e se sparasse comunque ogni esito verrebbe marcato come interrotto.
    #[test]
    fn un_deadline_disarmato_non_riporta_niente() {
        let d = Deadline::arm("run-che-non-esiste".to_string(), Duration::from_millis(50));
        assert_eq!(d.disarm(), None, "disarmato subito: non ha avuto tempo di sparare");
    }

    /// ⚠️ **Una corsa già finita non è una corsa scaduta.** Il watchdog non sa quando la
    /// corsa termina; sa solo che il registro non la conosce più, ed è quella la differenza
    /// fra «l'ho fermata io» e «aveva già finito». Senza questo, ogni corsa più lunga del
    /// limite tornerebbe marcata come interrotta anche quando è arrivata in fondo.
    #[test]
    fn un_id_sconosciuto_non_diventa_un_timeout() {
        let d = Deadline::arm("run-che-non-esiste".to_string(), Duration::from_millis(50));
        std::thread::sleep(Duration::from_millis(400));
        assert_eq!(d.disarm(), None, "ha provato a fermarla e non c'era: niente da riportare");
    }

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
