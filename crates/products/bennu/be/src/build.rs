//! `build` domain — `bennu_build` / `bennu_run` / `bennu_cancel_run` (docs §4 "il
//! fondo": the BUILD and RUN that make the Run/Debug buttons real + feed
//! `target/classes` to the index).
//!
//! ## Why the build is not just "run Maven"
//!
//! Maven's floor is seconds *with nothing to do*: every invocation starts a JVM, re-reads and
//! interpolates every pom in the reactor, re-resolves each module's plugins and re-runs its
//! up-to-date checks before concluding there was nothing to compile. An IDE feels instant
//! because it does not ask the build tool that question — it keeps its own model of what
//! changed and invokes a compiler only when something has.
//!
//! So this module does the same two things, in order:
//!
//! 1. [`up_to_date`] — a few hundred `stat` calls over the modules' `src/main/{java,resources}`
//!    against the stamp of the last successful compile. Unchanged → no Maven at all.
//! 2. `-pl <module> -am` — when it must compile, only the module being run and the ones it is
//!    built from, not the reactor.
//!
//! Note that `spring-boot:run` would be *slower*, not faster: it is `mvn compile` plus a
//! plugin to resolve, plus a lifecycle fork, plus a second JVM between us and the program —
//! which also puts a process between Stop and what it has to kill.
//!
//! - `bennu_build` compiles the project with the toolchain its manifest implies:
//!   * **Maven root** → **`mvn -q -o compile`** (offline, the project's JDK via
//!     `JAVA_HOME`); if the `mvn` launcher can't be spawned it falls back to **`javac`**
//!     over the Maven source roots.
//!   * **Cargo root** → **`cargo check --workspace --message-format=short`**. `check`
//!     and not `build`: what the button is for is *the diagnostics*, and `check` reaches
//!     them without linking — several times faster on a workspace, which is the
//!     difference between a usable button and one nobody presses. `short` is chosen for
//!     one concrete reason: it renders as `file:line:col: error[E0308]: message`, the
//!     same shape [`parse_diagnostics`] already reads for `javac`, so one parser serves
//!     both toolchains instead of two that drift.
//!
//!   Either way it captures stdout/stderr, streams the raw log as
//!   `arbor://bennu/build-output` events, and PARSES compiler error lines into structured
//!   [`BuildDiagnostic`]s. After a **successful** Java compile it triggers a re-index of
//!   the project (so `target/classes` output is reflected in completion); a Cargo project
//!   has no index to refresh.
//! - `bennu_run` launches **`java -cp <classpath> <mainClass>`** — classpath = the
//!   project's `target/classes` + the Phase-2 `.m2`-resolved dependency jars — and
//!   streams stdout/stderr as `arbor://bennu/run-output`, ending with an
//!   `arbor://bennu/run-exit`. Returns a [`RunHandle`] the FE uses to correlate the
//!   stream, to `bennu_run_input` (the child's stdin is a pipe, so the console can answer
//!   a prompt) and to `bennu_cancel_run` (which kills the tree — see there).
//!
//! Threading: build shells out via short-lived **NoWindow** children. The serve loop
//! dispatches each request on its own thread (see `arbor_ipc::serve_stdio`), so a
//! sync handler that blocks on a child never stalls the IPC read loop or other requests.
//! Run spawns a detached-from-the-handler background thread that owns the child + the two
//! reader threads, so the launching RPC returns immediately.
//!
//! The pure **error parser** ([`parse_diagnostics`]) is the unit-tested core; the
//! shell-out + streaming is the glue around it.

use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use arbor_ipc::prelude::EventSink;
use arbor_process_ext::prelude::NoWindowExt;
use bennu_classpath::prelude::{find_jdk_home, MavenClasspathCache, MavenResolveOpts};
use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{BuildDiagnostic, BuildResult, RunHandle};
use serde::Deserialize;
use serde_json::json;

use crate::index_service::IndexService;
use crate::log::{class_map, ClassMap, LogAnnotator};

/// The JDK level to resolve `JAVA_HOME` against as a LAST resort — when the project isn't open
/// and has no override, so nothing has read its pom (the target stack is JDK 8 —
/// Struts2/Entando). A project that declares its level gets that level; see
/// [`project_jdk_level`].
const DEFAULT_JDK: &str = "1.8";

// ── event topics (the wire contract for the FE) ────────────────────────────────

/// A line of build (mvn/javac) output.
const EVT_BUILD_OUTPUT: &str = "arbor://bennu/build-output";
/// The build finished — carries `ok` + the parsed diagnostic count.
const EVT_BUILD_DONE: &str = "arbor://bennu/build-done";
/// A line of run (java) output.
const EVT_RUN_OUTPUT: &str = "arbor://bennu/run-output";
/// The run process exited — carries the exit code.
const EVT_RUN_EXIT: &str = "arbor://bennu/run-exit";

// ── single-run guard (build + project validation) ───────────────────────────────

/// `true` while a build or a project validation is running. Both acquire the same guard so only
/// **one** compile/validation runs at a time — two `mvn` processes (or a build racing a validation)
/// on the same tree would thrash `target/` and the index.
static BUILD_BUSY: AtomicBool = AtomicBool::new(false);

/// RAII lock over [`BUILD_BUSY`]. [`acquire`](BuildGuard::acquire) returns `None` when one is already
/// held; the flag is released on drop (so an early return / panic can't leave it stuck).
pub(crate) struct BuildGuard;

impl BuildGuard {
    /// Take the single-run lock, or `None` if a build/validation is already in progress.
    pub(crate) fn acquire() -> Option<Self> {
        BUILD_BUSY
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| BuildGuard)
    }
}

impl Drop for BuildGuard {
    fn drop(&mut self) {
        BUILD_BUSY.store(false, Ordering::Release);
    }
}

/// The message returned when a concurrent build/validation is refused.
pub(crate) const BUSY_MSG: &str = "A build or validation is already running";

// ── bennu_build ────────────────────────────────────────────────────────────────

/// Args for [`bennu_build`].
#[derive(Deserialize)]
pub struct BuildArgs {
    /// Absolute path to the project root (the dir holding the root `pom.xml`).
    pub root: String,
    /// Compile only this module (and the ones it is built from), relative to `root`. `None`
    /// = the whole reactor, which is what the Build button means.
    ///
    /// Set by the launch path, where the only compiled output that matters is the one the
    /// run's classpath uses. It is the difference between a launch waiting on every module
    /// in the project and one waiting on the module you are running.
    #[serde(default)]
    pub module: Option<String>,
}

/// Compile the project with the toolchain its manifest implies (see the module doc).
/// Streams the raw log as `arbor://bennu/build-output`, returns the parsed diagnostics,
/// and re-indexes a Java project on success. A *failed compile* is a normal result
/// carrying diagnostics — not an `Err` (which is reserved for "no compiler could run at
/// all").
#[arbor_rpc::handler]
fn bennu_build(ctx: &BennuState, args: BuildArgs) -> Result<BuildResult, String> {
    let outcome = compile_project(ctx, &args.root, args.module.as_deref())?;
    Ok(BuildResult { tool: outcome.tool, ok: outcome.ok, diagnostics: outcome.diagnostics })
}

/// The compile itself — everything [`bennu_build`] does, plus the raw log it discards.
///
/// Split out for the agent-facing facade, which has no build panel behind it and so needs the log
/// when the diagnostic parser recognised nothing. The handler stays the thin shape it was; the two
/// cannot diverge because there is only one of them.
pub(crate) fn compile_project(
    ctx: &BennuState,
    root_path: &str,
    module: Option<&str>,
) -> Result<CompileOutcome, String> {
    // Refuse to start a second build/validation while one is running (only one at a time).
    let _guard = BuildGuard::acquire().ok_or_else(|| BUSY_MSG.to_string())?;
    let sink = ctx.event_sink();
    let root = PathBuf::from(root_path);
    let module = module.map(str::trim).filter(|m| !m.is_empty());

    let outcome = if is_cargo_root(&root) {
        // Says what is running to whoever is waiting on the call — the only thing a caller with no
        // build panel would otherwise have during the minute this takes.
        sink.progress(
            &match module {
                Some(package) => format!("cargo check -p {package}"),
                None => "cargo check --workspace".to_string(),
            },
            None,
            None,
        );
        let (ok, raw) = run_cargo_check(&root, module)?;
        finish_compile("cargo", ok, raw, &sink, &root)
    } else {
        let java_home = resolve_java_home(root_path);
        // Nothing has changed since the last successful compile → say so and stop. This is
        // the whole difference between "press ▷ and wait" and "press ▷": Maven's floor is
        // seconds even with nothing to do, and the most common launch of all is the one where
        // you have changed nothing.
        match up_to_date(&root) {
            Some(stamp) => {
                sink.emit(EVT_BUILD_OUTPUT, json!({ "text": "Everything is up to date." }));
                CompileOutcome {
                    tool: "up-to-date".into(),
                    ok: true,
                    diagnostics: Vec::new(),
                    raw: String::new(),
                    stamp: Some(stamp),
                }
            }
            None => {
                let stamp = source_stamp(&root);
                let mut out =
                    compile(&root, module, &resolve_mvn(&root), java_home.as_deref(), &sink)?;
                out.stamp = Some(stamp);
                out
            }
        }
    };

    // Remember what was on disk when this compile succeeded, so the next launch can tell
    // whether anything has changed. Recorded from the stamp taken BEFORE compiling: a build
    // writes into `target/`, and stamping afterwards would record a tree that includes its
    // own output.
    if outcome.ok {
        if let Some(stamp) = outcome.stamp {
            build_stamps()
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .insert(root_path.to_string(), stamp);
        }
    }

    sink.emit(EVT_BUILD_DONE, json!({
        "root": root_path,
        "tool": &outcome.tool,
        "ok": outcome.ok,
        "diagnostics": outcome.diagnostics.len(),
    }));

    // A clean Java compile means fresh `target/classes` — re-index so completion picks it
    // up. The reindex emits `arbor://bennu/index-progress` on the same sink. A Cargo
    // project has no symbol index (see `bennu_open_project`), so there is nothing to
    // refresh and asking would light an "Indexing…" status over an empty build.
    // …but not when nothing was compiled: re-indexing after a no-op costs the user a whole
    // index rebuild for nothing, and — since a rebuild deliberately forgets the build stamp —
    // it would make the NEXT launch compile again. The skip would defeat itself.
    if outcome.ok && outcome.tool != "cargo" && outcome.tool != "up-to-date" {
        IndexService::global().reindex(root_path, ctx.event_sink());
    }

    Ok(outcome)
}

/// Whether `root` is governed by Cargo — the same precedence `bennu-project`'s
/// `open_project` uses (Maven first: a polyglot root is the Java project).
fn is_cargo_root(root: &Path) -> bool {
    !root.join("pom.xml").is_file() && root.join("Cargo.toml").is_file()
}

// ── bennu_run ──────────────────────────────────────────────────────────────────

/// Args for [`bennu_run`].
#[derive(Deserialize)]
pub struct RunArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// The fully-qualified main class to launch. Required — main-class *discovery*
    /// (scanning for `public static void main`) is `bennu_main_classes`; the FE passes
    /// the resolved class here.
    pub main_class: String,
    /// Program arguments passed to the main class (after the main class on the argv).
    #[serde(default)]
    pub args: Vec<String>,
    /// JVM arguments (`-Xmx…`, `-D…`) placed BEFORE `-cp`/main class. Optional +
    /// back-compatible — a caller passing only `{ root, main_class, args }` still works.
    #[serde(default)]
    pub vm_args: Option<Vec<String>>,
    /// The Maven module the class belongs to, relative to `root` (`services/core`). Empty /
    /// `None` = the root module.
    ///
    /// It decides the classpath, and on a multi-module project there is no useful default:
    /// the root of a reactor usually compiles nothing at all, so a run configured without a
    /// module got a classpath whose only entry did not exist and died on
    /// `ClassNotFoundException`.
    #[serde(default)]
    pub module: Option<String>,
    /// Working directory for the child. Empty / `None` = the module's directory (the project
    /// root when there is no module) — which is what a program reading `./config` expects.
    #[serde(default)]
    pub working_dir: Option<String>,
    /// Extra environment variables applied to the child (merged over the inherited env).
    #[serde(default)]
    pub env: Option<std::collections::HashMap<String, String>>,
    /// The Maven scope the dependencies are resolved at — `"runtime"` (what a packaged
    /// application sees), `"compile"`, `"test"`, or `""` for every scope.
    ///
    /// `None` from an older caller means **runtime**, not "every scope": the every-scope
    /// classpath is the *index's*, and launching with it puts test- and provided-scoped
    /// libraries in front of the JVM that Maven would never have supplied. See
    /// [`bennu_proto::prelude::RunConfig::classpath_scope`].
    #[serde(default)]
    pub classpath_scope: Option<String>,
    /// Launch under the debugger: the JVM gets the JDWP agent and connects back to a port
    /// opened here first (see [`crate::debug`]), and the session carries this run's id.
    #[serde(default)]
    pub debug: bool,
    /// Hold the VM before `main` until the debugger has attached and installed everything.
    ///
    /// Off unless the run configuration says otherwise, and deliberately: it is the only way to
    /// stop in start-up code, and it means every launch begins frozen. Without it a breakpoint
    /// the program has already run past is simply missed, which is the right trade for the
    /// launch you press fifty times a day.
    #[serde(default)]
    pub debug_suspend: bool,
}

/// Launch `java <vm_args…> -cp <target/classes:deps> <main_class> <args...>` and stream
/// its stdout/stderr as `arbor://bennu/run-output`, ending with `arbor://bennu/run-exit`.
/// VM args (when given) precede `-cp`; the working dir + extra env (when given) are
/// applied to the child. Returns immediately with the [`RunHandle`] correlating the
/// stream; the child runs on a background thread.
///
/// **stdin is a pipe**, not `/dev/null`: a console you cannot answer is not a console, and a
/// program that stops at a prompt with no way to reply looks hung. [`bennu_run_input`] writes
/// to it.
#[arbor_rpc::handler]
fn bennu_run(ctx: &BennuState, args: RunArgs) -> Result<RunHandle, String> {
    let root = PathBuf::from(&args.root);
    let java_home = resolve_java_home(&args.root);
    let java = java_program(java_home.as_deref());
    // The module the class lives in, if any — an empty string is "the root", not a directory
    // called "".
    let module = args.module.as_deref().map(str::trim).filter(|m| !m.is_empty());
    // `None` is runtime, not every-scope: a caller that says nothing gets what Maven would give
    // it, which is the safe direction to default in.
    let scope = args.classpath_scope.as_deref().unwrap_or("runtime");
    let classpath = run_classpath(&root, module, java_home.as_deref(), scope);

    // Working dir: an explicit non-empty override, else the module's own directory (the root
    // when there is none) — a program that reads `./config` means its module's.
    let cwd = match args.working_dir.as_deref() {
        Some(d) if !d.trim().is_empty() => PathBuf::from(d),
        _ => module.map(|m| root.join(m)).unwrap_or_else(|| root.clone()),
    };

    let mut cmd = Command::new(&java);
    cmd.current_dir(&cwd);

    // Under the debugger, the port has to be listening BEFORE the process exists: the agent
    // connects back during VM initialization and aborts the launch if nothing answers. A port
    // that cannot be bound degrades to an ordinary run — a program that starts without the
    // debugger beats one that does not start.
    let launch = args.debug.then(crate::debug::prepare).flatten();

    // VM args come BEFORE -cp / main class (JVM options must precede the class). The agent goes
    // in with them rather than beside them, so the command line the console prints is the whole
    // truth about what ran.
    let mut vm_args = args.vm_args.clone().unwrap_or_default();
    if let Some(l) = &launch {
        vm_args.insert(0, crate::debug::agent_arg(l.port, args.debug_suspend));
    }
    for a in &vm_args {
        cmd.arg(a);
    }
    // How the classpath reaches the JVM — see `classpath_form`. On a real dependency tree it
    // does NOT fit on the command line.
    let base = module.map(|m| root.join(m)).unwrap_or_else(|| root.clone());
    let cp_form = classpath_form(&base, &classpath, launching_jdk_major(&args.root));
    match &cp_form {
        ClasspathForm::Inline => {
            cmd.arg("-cp").arg(&classpath);
        }
        ClasspathForm::ArgFile(path) => {
            cmd.arg(format!("@{}", path.display()));
        }
        ClasspathForm::Environment => {
            // `-cp` would override it, so it is deliberately not passed.
            cmd.env("CLASSPATH", &classpath);
        }
    }
    cmd.arg(&args.main_class)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::piped());
    for a in &args.args {
        cmd.arg(a);
    }
    // Extra env merged over the inherited environment (later entries win by key). After the
    // classpath, so a configuration that sets CLASSPATH itself wins over ours — it asked.
    if let Some(env) = &args.env {
        for (k, v) in env {
            cmd.env(k, v);
        }
    }
    // A run child is console-less; suppress the window on Windows.
    cmd.no_window();

    let command =
        display_command(&java, &vm_args, &classpath, &cp_form, &args.main_class, &args.args);
    let root_for_debug = args.root.clone();
    let sink = ctx.event_sink();
    spawn_streamed(
        cmd,
        args.main_class.clone(),
        command,
        cwd.display().to_string(),
        &args.root,
        sink.clone(),
        // The debug session is keyed by the RUN id, so the console tab and the debugger are the
        // same thing to everything that has to correlate them (Stop, the frames panel, the
        // gutter) — which means it can only be started once the id exists.
        |run_id| {
            if let Some(launch) = launch {
                crate::debug::start(run_id.to_string(), root_for_debug, launch, sink);
            }
        },
    )
    .map_err(|e| format!("spawn java ({java}): {e}"))
}

/// Spawn `cmd` and stream it to the Run console — the one place a child becomes a *run*.
///
/// Everything the console needs is set up here: both pipes pumped on their own threads, the child
/// registered so Stop can kill its tree and the console can answer a prompt on its stdin, and the
/// exit event emitted when it goes.
///
/// Shared rather than written twice because the two callers differ only in how they build a command
/// line: [`bennu_run`] launches a JVM, [`crate::cargo_cmd`] launches a cargo subcommand. Everything
/// after the spawn — cancellation, input, the tab lifecycle, the log annotation — has to behave
/// *identically* for both, and "nearly identically" is what two copies of this would drift into.
///
/// `label` fills the [`RunHandle::main_class`] slot: for a JVM it is the main class, for a cargo
/// command the command itself. `after_register` runs once the run id exists and before the child is
/// pumped, for whatever has to be keyed by it.
///
/// The child must already have its three pipes set to [`Stdio::piped`]; without them the console has
/// nothing to show and nothing to write to.
pub(crate) fn spawn_streamed<F: FnOnce(&str)>(
    mut cmd: Command,
    label: String,
    command_line: String,
    working_dir: String,
    root: &str,
    sink: Arc<dyn EventSink>,
    after_register: F,
) -> Result<RunHandle, String> {
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;

    let run_id = next_run_id();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdin = child.stdin.take();

    let child = Arc::new(Mutex::new(child));
    RunRegistry::global().register(&run_id, child.clone(), stdin);
    after_register(&run_id);

    // Resolved once for the whole run, not once per line: the class index answers "which file
    // declares com.acme.Order" for every frame of every trace this program prints. Empty for a
    // project with no Java index, which costs nothing.
    let classes = class_map(root);

    let run_id_thread = run_id.clone();
    let child_thread = child.clone();
    std::thread::Builder::new()
        .name(format!("bennu-run-{run_id}"))
        .spawn(move || {
            // Pump both pipes on their own threads so a chatty stderr can't deadlock a
            // full stdout pipe (or vice-versa).
            let mut pumps = Vec::new();
            if let Some(out) = stdout {
                pumps.push(spawn_pump(
                    out,
                    "stdout",
                    run_id_thread.clone(),
                    sink.clone(),
                    classes.clone(),
                ));
            }
            if let Some(err) = stderr {
                pumps.push(spawn_pump(err, "stderr", run_id_thread.clone(), sink.clone(), classes));
            }
            // POLLED, not `wait()`: the canceller needs the same handle to kill the tree, and
            // a thread parked inside `wait()` holds the lock for the entire run.
            let code = loop {
                let status = {
                    let mut guard = child_thread.lock().unwrap_or_else(|p| p.into_inner());
                    guard.try_wait()
                };
                match status {
                    Ok(Some(s)) => break s.code(),
                    Ok(None) => std::thread::sleep(Duration::from_millis(80)),
                    Err(_) => break None,
                }
            };
            for p in pumps {
                let _ = p.join();
            }
            RunRegistry::global().finish(&run_id_thread);
            sink.emit(EVT_RUN_EXIT, json!({ "run_id": run_id_thread, "code": code }));
        })
        .map_err(|e| format!("spawn run thread: {e}"))?;

    Ok(RunHandle { run_id, main_class: label, command: command_line, working_dir })
}

/// The spawned command as one display line. Arguments containing spaces are quoted.
///
/// How the classpath appears depends on how it was passed, because the point of the line is
/// to be the truth about what ran: an **argument file** is named, and that line really is
/// pasteable; an inline one is abbreviated to a count, since a resolved `~/.m2` classpath is
/// tens of thousands of characters and would bury everything anyone reads.
fn display_command(
    java: &str,
    vm_args: &[String],
    classpath: &str,
    form: &ClasspathForm,
    main_class: &str,
    args: &[String],
) -> String {
    let entries = classpath.split(if cfg!(windows) { ';' } else { ':' }).count();
    let plural = if entries == 1 { "y" } else { "ies" };
    let mut parts: Vec<String> = vec![quoted(java)];
    parts.extend(vm_args.iter().map(|a| quoted(a)));
    match form {
        ClasspathForm::Inline => {
            parts.push("-cp".to_string());
            parts.push(format!("<classpath: {entries} entr{plural}>"));
        }
        ClasspathForm::ArgFile(p) => parts.push(format!("@{}", p.display())),
        ClasspathForm::Environment => {
            parts.push(format!("<classpath: {entries} entr{plural}, via CLASSPATH>"))
        }
    }
    parts.push(main_class.to_string());
    parts.extend(args.iter().map(|a| quoted(a)));
    parts.join(" ")
}

/// Wrap in double quotes when the token contains whitespace.
fn quoted(s: &str) -> String {
    if s.contains(char::is_whitespace) { format!("\"{s}\"") } else { s.to_string() }
}

/// Args for [`bennu_cancel_run`].
#[derive(Deserialize)]
pub struct CancelRunArgs {
    /// The run id returned by `bennu_run`.
    pub run_id: String,
}

/// Kill a live run — **the process tree**, not just the handle. Returns `true` when a run
/// was killed, `false` when the id is unknown or it had already finished.
///
/// This used to remove the id from a set of live ids and return `true`, killing nothing: the
/// panel said "stopped" and the JVM went on running, holding its port and writing to its
/// files, with no way left to stop it short of Task Manager. Stop now means stop.
#[arbor_rpc::handler]
fn bennu_cancel_run(_ctx: &BennuState, args: CancelRunArgs) -> Result<bool, String> {
    let Some(child) = RunRegistry::global().child_of(&args.run_id) else { return Ok(false) };
    let mut child = child.lock().unwrap_or_else(|p| p.into_inner());
    crate::child::kill_tree(&mut child);
    Ok(true)
}

/// Args for [`bennu_run_input`].
#[derive(Deserialize)]
pub struct RunInputArgs {
    /// The run id returned by `bennu_run`.
    pub run_id: String,
    /// One line to feed the program. The newline is added here — the console sends what was
    /// typed, and "did the caller include the terminator" is exactly the kind of question a
    /// wire contract should not have.
    pub text: String,
}

/// Write a line to a live run's stdin. `Err` when the run is unknown or has already exited —
/// which the console reports, because typing into a dead process and seeing nothing happen
/// is indistinguishable from the program ignoring you.
#[arbor_rpc::handler]
fn bennu_run_input(_ctx: &BennuState, args: RunInputArgs) -> Result<(), String> {
    let Some(stdin) = RunRegistry::global().stdin_of(&args.run_id) else {
        return Err("that run is no longer live".to_string());
    };
    let mut guard = stdin.lock().unwrap_or_else(|p| p.into_inner());
    let Some(pipe) = guard.as_mut() else { return Err("that run has no input pipe".to_string()) };
    writeln!(pipe, "{}", args.text).map_err(|e| format!("write to the program's input: {e}"))?;
    pipe.flush().map_err(|e| format!("flush the program's input: {e}"))
}

// ── compile (mvn → javac fallback) ─────────────────────────────────────────────

/// The outcome of a compile: the tool that ran, whether it exited 0, and the parsed
/// diagnostics. The raw log is streamed as events (not carried here).
pub(crate) struct CompileOutcome {
    pub(crate) tool: String,
    pub(crate) ok: bool,
    pub(crate) diagnostics: Vec<BuildDiagnostic>,
    /// The compiler's own output, kept rather than only streamed.
    ///
    /// The panel reads the `build-output` events and needs nothing here; a caller with no panel —
    /// an agent — has no other way to see a failure the diagnostic parser did not recognise, and
    /// "the build failed, no further information" is the least useful answer a build can give.
    pub(crate) raw: String,
    /// The source stamp this compile corresponds to, recorded on success so the next one can
    /// skip. `None` for a toolchain the staleness check doesn't cover (Cargo).
    stamp: Option<u64>,
}

/// Run `mvn -q -o compile`; if the launcher can't be spawned, fall back to `javac`.
/// Streams each captured line as a `build-output` event. `Err` only when neither tool
/// could run.
fn compile(
    root: &Path,
    module: Option<&str>,
    mvn_path: &str,
    java_home: Option<&Path>,
    sink: &Arc<dyn EventSink>,
) -> Result<CompileOutcome, String> {
    match run_mvn_compile(root, module, mvn_path, java_home) {
        Ok((ok, raw)) => Ok(finish_compile("mvn", ok, raw, sink, root)),
        Err(spawn_err) => {
            let (ok, raw) = run_javac(root, java_home)
                .map_err(|javac_err| format!("mvn: {spawn_err}; javac: {javac_err}"))?;
            Ok(finish_compile("javac", ok, raw, sink, root))
        }
    }
}

/// Stream the raw log line-by-line, parse it, and assemble the outcome.
///
/// The log is interpreted on the way out, the same way a run's output is: Maven's
/// `[ERROR]`s, the absolute paths in a compiler diagnostic and the qualified names in a
/// plugin's stack trace are all worth reading as what they are, and the parsed diagnostics
/// above the log only cover the compiler's own lines.
fn finish_compile(
    tool: &str,
    ok: bool,
    raw: String,
    sink: &Arc<dyn EventSink>,
    root: &Path,
) -> CompileOutcome {
    let mut log = LogAnnotator::for_root(&root.display().to_string());
    for line in raw.lines() {
        sink.emit(EVT_BUILD_OUTPUT, log.line(line));
    }
    CompileOutcome {
        tool: tool.to_string(),
        ok,
        diagnostics: parse_diagnostics(&raw),
        raw,
        stamp: None,
    }
}

/// `cargo check --message-format=short` in `root`, over `package` or the whole workspace.
///
/// `--workspace` when no package is named, because the root of a Cargo workspace is a
/// *virtual* manifest with no code of its own: without it, pressing Build on a workspace
/// checks nothing and reports success, which is the worst possible answer. `--color=never` so ANSI escapes don't end
/// up rendered as garbage in the build log.
///
/// `Err` only when the launcher can't be spawned (no `cargo` on `PATH`) — there is no
/// fallback compiler to try, unlike the `mvn` → `javac` path: `rustc` invoked by hand
/// cannot resolve a single dependency, so offering it would produce a wall of
/// unresolved-import errors that say nothing about the code.
fn run_cargo_check(root: &Path, package: Option<&str>) -> Result<(bool, String), String> {
    let launcher = cargo_launcher();
    let mut cmd = Command::new(&launcher);
    cmd.current_dir(root).arg("check");
    // One package instead of the workspace when the caller named one. On a workspace of twenty
    // crates that is the difference between a check you wait out and one you read — and after
    // editing a single crate it is also the only part of the answer that changed.
    match package {
        Some(package) => cmd.arg("-p").arg(package),
        None => cmd.arg("--workspace"),
    };
    cmd.arg("--message-format=short").arg("--color=never");
    cmd.no_window();
    let out = cmd
        .output()
        .map_err(|e| format!("spawn cargo ({}): {e}", launcher.to_string_lossy()))?;
    Ok((out.status.success(), merge_output(&out.stdout, &out.stderr)))
}

fn run_mvn_compile(
    root: &Path,
    module: Option<&str>,
    mvn_path: &str,
    java_home: Option<&Path>,
) -> Result<(bool, String), String> {
    let mut cmd = Command::new(mvn_path);
    cmd.current_dir(root)
        .arg("-q")
        .arg("compile")
        .arg("--batch-mode")
        .arg("-o"); // offline: resolve only from the local ~/.m2 cache
    // Scope the reactor when we know which module is about to run. `mvn compile` at the root
    // of a large reactor costs tens of seconds with NOTHING to do — Maven still scans every
    // pom, resolves every module's plugins and runs each one's up-to-date checks. `-pl` cuts
    // that to the module, `-am` keeps the ones it is built from, which is exactly the set the
    // run's classpath needs compiled.
    if let Some(m) = module {
        cmd.arg("-pl").arg(m).arg("-am");
    }
    if let Some(jh) = java_home {
        cmd.env("JAVA_HOME", jh);
    }
    cmd.no_window();
    let out = cmd.output().map_err(|e| format!("spawn mvn ({mvn_path}): {e}"))?;
    Ok((out.status.success(), merge_output(&out.stdout, &out.stderr)))
}

/// javac fallback: compile every `.java` under the source roots to `target/bennu-classes`
/// and capture the output. Classpath is best-effort (project sources only — the mvn path
/// gets the full dep classpath); its purpose is to still surface *syntax/type* errors as
/// diagnostics when Maven isn't available.
fn run_javac(root: &Path, java_home: Option<&Path>) -> Result<(bool, String), String> {
    let javac = javac_program(java_home);
    let mut sources = Vec::new();
    for sr in source_roots(root) {
        collect_java(&sr, &mut sources);
    }
    if sources.is_empty() {
        return Err(format!("no .java sources under {}", root.display()));
    }
    let out_dir = root.join("target").join("bennu-classes");
    let _ = std::fs::create_dir_all(&out_dir);

    let mut cmd = Command::new(&javac);
    cmd.current_dir(root)
        .arg("-d")
        .arg(&out_dir)
        .arg("-encoding")
        .arg("UTF-8");
    for s in &sources {
        cmd.arg(s);
    }
    cmd.no_window();
    let out = cmd.output().map_err(|e| format!("spawn javac ({javac}): {e}"))?;
    Ok((out.status.success(), merge_output(&out.stdout, &out.stderr)))
}

// ── getting a very long classpath to the JVM ───────────────────────────────────

/// How the classpath is handed over.
enum ClasspathForm {
    /// `-cp <classpath>` on the command line. Short ones, and the readable case.
    Inline,
    /// `@<file>` — a JDK 9+ argument file holding the `-cp`.
    ArgFile(PathBuf),
    /// The `CLASSPATH` environment variable, for a JDK 8 that has no argument files.
    Environment,
}

/// Past this many characters the classpath does not go on the command line.
///
/// Windows caps a whole command line at 32767 characters and fails the spawn with
/// `os error 206` — "the filename or extension is too long", which names neither the
/// classpath nor the limit and is why this took a report to find. A resolved `~/.m2`
/// classpath on a Spring project is hundreds of jars and passes that on its own. The
/// threshold is well under the cap because the command line also carries the JDK path, the VM
/// arguments, the main class and the program arguments.
const MAX_INLINE_CLASSPATH: usize = 8_000;

/// Decide how to pass `classpath`, writing the argument file if that is the answer.
///
/// An **argument file** is the JDK's own remedy (`java @file`, JDK 9+) and costs nothing but a
/// small write. Paths go in with forward slashes: the JVM accepts them on Windows, and a
/// backslash inside an argument file is an escape character — `C:\lib\x.jar` would arrive as
/// `C:libx.jar` and the run would die on a classpath that looks correct in every log.
///
/// A **JDK 8** has no argument files, so its classpath goes in the environment instead, which
/// the launcher reads when `-cp` is absent. That is the older, blunter tool: it caps out too,
/// just not on the same budget as the command line.
fn classpath_form(base: &Path, classpath: &str, major: u32) -> ClasspathForm {
    if classpath.len() <= MAX_INLINE_CLASSPATH {
        return ClasspathForm::Inline;
    }
    if major < 9 {
        return ClasspathForm::Environment;
    }
    let dir = base.join("target");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("bennu-run.args");
    let body = format!("-cp \"{}\"\n", classpath.replace('\\', "/"));
    match std::fs::write(&path, body) {
        Ok(()) => ClasspathForm::ArgFile(path),
        // Nowhere to write it — the environment is the remaining option, and a run that might
        // work beats one that certainly will not.
        Err(_) => ClasspathForm::Environment,
    }
}

/// The major version of the JDK a launch will actually use — which is the one that decides
/// whether argument files exist.
///
/// The project's declared level is the question; the INSTALLED JDK that answers it is what
/// matters, and the two differ whenever the exact level isn't installed. `jdk_status` resolves
/// it the same way the classpath tier does, so a project declaring 21 on a machine that only
/// has 17 is read as 17 — still an argument file, and still correct.
fn launching_jdk_major(root: &str) -> u32 {
    let level = project_jdk_level(root);
    bennu_classpath::prelude::jdk_status(&level)
        .resolved_major
        .unwrap_or_else(|| jdk_major(&level))
}

/// The major version of a language-level string (`"1.8"` → 8, `"21"` → 21). `0` when it is
/// not a version we recognise, which reads as "older than 9" and takes the conservative path.
fn jdk_major(level: &str) -> u32 {
    let v = level.trim();
    let head = v.strip_prefix("1.").unwrap_or(v);
    head.split(['.', '_', '-']).next().and_then(|s| s.parse().ok()).unwrap_or(0)
}

// ── "has anything changed since the last compile" ──────────────────────────────

/// The source stamp of the last SUCCESSFUL compile, per project root.
///
/// In memory only. Persisting it would mean trusting a stamp written by a different version
/// of Bennu, or one taken before someone ran `mvn clean` outside the editor — and the cost of
/// being wrong is a run against stale classes, which is the single most confusing failure a
/// build system can produce. One Maven invocation per session is a price worth paying for
/// never being wrong about it.
fn build_stamps() -> &'static Mutex<HashMap<String, u64>> {
    static STAMPS: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    STAMPS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// `Some(stamp)` when the project has not changed since its last successful compile — the
/// caller can skip the build and keep the stamp. `None` when it must compile.
///
/// The output has to still be there: `mvn clean` in a terminal, or a deleted `target/`, makes
/// a matching stamp a lie.
fn up_to_date(root: &Path) -> Option<u64> {
    let key = root.display().to_string();
    let previous = *build_stamps().lock().unwrap_or_else(|p| p.into_inner()).get(&key)?;
    if !any_output_exists(root) {
        return None;
    }
    (source_stamp(root) == previous).then_some(previous)
}

/// Whether any module of the project has compiled output at all.
fn any_output_exists(root: &Path) -> bool {
    if root.join("target").join("classes").is_dir() {
        return true;
    }
    module_dirs(root).iter().any(|m| m.join("target").join("classes").is_dir())
}

/// A hash of the project's compilable inputs: every file under each module's `src/main/java`
/// and `src/main/resources`, by relative path, size and modification time.
///
/// **Stats, not reads.** Nothing is opened and nothing is parsed, so this is a few hundred
/// `stat` calls on a large project — milliseconds against Maven's seconds. That difference is
/// the entire answer to "why is the IDE instant and this is not": an IDE keeps its own model
/// of what changed and asks the build tool only when something has, while every `mvn`
/// invocation re-reads every pom, re-resolves every plugin and re-checks every module before
/// discovering there was nothing to do.
///
/// Modification time and size together, rather than content hashing: reading every source to
/// decide whether to compile them would cost more than the compile it is trying to avoid. The
/// failure mode is an edit that preserves both, which is not something an editor produces.
///
/// Resources are in it deliberately — an edited `application.yml` changes what the program
/// does, and a stamp that ignored it would launch the old one.
fn source_stamp(root: &Path) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();

    let mut dirs = vec![root.to_path_buf()];
    dirs.extend(module_dirs(root));
    // Sorted so the hash does not depend on the order the reactor happens to be walked in.
    dirs.sort();

    for dir in dirs {
        for rel in ["src/main/java", "src/main/resources"] {
            let mut files = Vec::new();
            collect_files(&dir.join(rel), &mut files);
            files.sort();
            for f in files {
                f.to_string_lossy().hash(&mut hasher);
                if let Ok(md) = std::fs::metadata(&f) {
                    md.len().hash(&mut hasher);
                    if let Ok(t) = md.modified() {
                        if let Ok(d) = t.duration_since(std::time::UNIX_EPOCH) {
                            d.as_nanos().hash(&mut hasher);
                        }
                    }
                }
            }
        }
    }
    hasher.finish()
}

/// Every file under `dir`, recursively. Dot-directories are skipped — nothing under `.git`
/// is a compile input, and walking it would dwarf the rest.
fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with('.')) {
                continue;
            }
            collect_files(&p, out);
        } else {
            out.push(p);
        }
    }
}

/// Forget a project's build stamp, so the next build runs for real. Called when the index is
/// rebuilt — the moment the user has told us not to trust what we remember.
pub(crate) fn forget_build_stamp(root: &str) {
    build_stamps().lock().unwrap_or_else(|p| p.into_inner()).remove(root);
}

// ── run classpath ──────────────────────────────────────────────────────────────

/// The run classpath for a launch in `module` (`None` = the root module).
///
/// Order: **the module's own `target/classes` first**, then every OTHER reactor module's,
/// then the root's, then the `.m2`-resolved dependency jars (offline). Dep resolution
/// failure is non-fatal — the run degrades to the compiled output only.
///
/// Why the sibling modules are all there: on a reactor, `web` depends on `core`, and until
/// `core` is installed to `~/.m2` the only copy of its classes is `core/target/classes`. A
/// developer's inner loop is compile-and-run without installing, so a classpath that only
/// held the launched module's output would fail on the first call across a module boundary.
/// They come *after* the launched module so its own classes always win a name collision.
///
/// This used to take only the root, which on a multi-module project is the one directory
/// that never contains anything: a reactor root is packaging `pom` and compiles nothing.
///
/// `scope` is the Maven scope the dependencies are resolved at — `"runtime"` for what a
/// packaged application sees, `""` for every scope (the index's own view). See
/// [`bennu_proto::prelude::RunConfig::classpath_scope`].
fn run_classpath(
    root: &Path,
    module: Option<&str>,
    java_home: Option<&Path>,
    scope: &str,
) -> String {
    let sep = if cfg!(windows) { ";" } else { ":" };
    let base = module.map(|m| root.join(m)).unwrap_or_else(|| root.to_path_buf());

    let mut parts: Vec<String> = Vec::new();
    let mut push_classes = |dir: &Path, parts: &mut Vec<String>| {
        let classes = dir.join("target").join("classes");
        let s = classes.display().to_string();
        if !parts.contains(&s) {
            parts.push(s);
        }
    };
    push_classes(&base, &mut parts);
    for dir in module_dirs(root) {
        push_classes(&dir, &mut parts);
    }
    push_classes(root, &mut parts);

    // ── the dependency jars ────────────────────────────────────────────────────
    //
    // Which set depends on the scope asked for, and the distinction is the whole reason this
    // parameter exists:
    //
    //   * **every scope** — the index's own list, already in memory and free. It is what
    //     completion and navigation resolve against, so the run agrees with the editor.
    //   * **a narrower scope** — a resolve of its own. The index's list cannot be filtered
    //     down to it: it is a flat list of paths with the scopes already thrown away, and
    //     guessing which jars are test-only from their names is how you drop a real dependency.
    //
    // The narrow one is the default (`runtime`) because launching with the editing classpath
    // hands the JVM libraries Maven would never put there — see `RunConfig::classpath_scope`.
    let mut jars = if scope.is_empty() {
        IndexService::global().dep_jars_of(&root.display().to_string())
    } else {
        Vec::new()
    };

    if jars.is_empty() {
        let mut opts = MavenResolveOpts::default();
        opts.mvn_path = resolve_mvn(root);
        opts.offline = true;
        opts.scope = (!scope.is_empty()).then(|| scope.to_string());
        if let Some(jh) = java_home {
            opts.java_home = Some(jh.to_path_buf());
        }
        // The MODULE's own dependencies when it has a pom of its own; the root's otherwise.
        // Chosen by asking whether the pom EXISTS rather than by letting the resolve fail —
        // a failed resolve is a Maven invocation, and falling back on it means paying twice.
        let dir = if base.join("pom.xml").is_file() { base.as_path() } else { root };
        // A cache that OUTLIVES the call. It used to be built fresh here — a cache with nothing
        // in it, on every launch — so pressing ▷ shelled out to Maven and the run did not start
        // until it had finished, every single time. Keyed by pom **and scope**, so a runtime
        // resolve for a launch never becomes the answer the index gets.
        if let Ok(cp) = run_classpath_cache()
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .get(dir, &opts)
        {
            jars = cp.jars.iter().map(|j| j.display().to_string()).collect();
        }
    }
    parts.extend(jars);
    parts.join(sep)
}

/// The launch classpaths resolved so far, across launches.
///
/// Per session and per (pom, scope): the first ▷ of a configuration pays Maven once, every one
/// after it is instant until the pom is edited.
fn run_classpath_cache() -> &'static std::sync::Mutex<MavenClasspathCache> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<MavenClasspathCache>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(MavenClasspathCache::new()))
}

/// Every module directory of the Maven reactor rooted at `root`, absolute, in declaration
/// order and including nested ones.
///
/// Reads only `pom.xml` files (never walks the tree), so it costs one small read per module.
/// Bounded by [`MAX_MODULES`] against a pom that declares itself as its own module.
pub(crate) fn module_dirs(root: &Path) -> Vec<PathBuf> {
    /// Enough for the largest legacy reactor and small enough that a cycle stops quickly.
    const MAX_MODULES: usize = 300;

    let mut out: Vec<PathBuf> = Vec::new();
    let mut queue: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(dir) = queue.pop() {
        if out.len() >= MAX_MODULES {
            break;
        }
        let Ok(xml) = std::fs::read_to_string(dir.join("pom.xml")) else { continue };
        for name in bennu_project::prelude::parse_pom(&xml).modules {
            let name = name.trim();
            // `.` / `..` / empty name a directory that is not a new module. Skipped by NAME
            // rather than by comparing the joined paths, because `a/b/.` and `a/b` are
            // different `PathBuf`s and the walk would keep finding "new" modules.
            if name.is_empty() || name == "." || name == ".." {
                continue;
            }
            let child = dir.join(name);
            if child == dir || out.contains(&child) {
                continue; // a module reachable by two paths
            }
            out.push(child.clone());
            queue.push(child);
        }
    }
    out
}

// ── the PURE error parser (unit-tested; no I/O) ────────────────────────────────

/// Parse `javac` / `mvn` / `cargo` compiler output into structured [`BuildDiagnostic`]s.
///
/// Recognised shapes (Windows drive-letter aware — a `C:\` colon is never the
/// `file:line` separator):
/// - javac: `Path.java:12: error: cannot find symbol`
/// - javac line:col: `Path.java:12:7: error: ';' expected`
/// - javac warning: `Path.java:3: warning: [deprecation] ...`
/// - mvn compiler-plugin: `[ERROR] /abs/Foo.java:[45,17] cannot find symbol`
/// - rustc (`--message-format=short`): `src/lib.rs:12:5: error[E0308]: mismatched types`
///
/// The rustc form is the javac form with a lint code bracketed onto the severity, which
/// is why one parser covers both: the `[E0308]` is moved into the message (`E0308:
/// mismatched types`) so the code stays visible without inventing a field for it.
///
/// mvn often echoes the same javac error both raw and wrapped in `[ERROR]`; identical
/// diagnostics are de-duped.
///
/// javac's CONTINUATION lines are folded into the diagnostic they belong to. `cannot find symbol`
/// on its own says nothing — the name it could not find is on the `symbol:` line underneath, and
/// the place it looked on the `location:` line. Reading the output one line at a time dropped both,
/// so the marker in the buffer read `cannot find symbol  (build)` and left the reader to go and
/// find out which symbol.
pub fn parse_diagnostics(raw: &str) -> Vec<BuildDiagnostic> {
    let mut out: Vec<BuildDiagnostic> = Vec::new();
    let mut seen = HashSet::new();
    // The diagnostic the next continuation line belongs to. `None` after a line that was not a
    // diagnostic, and after a DUPLICATE one — mvn echoes each javac error twice, continuations and
    // all, and appending the echo's would say everything twice.
    let mut open: Option<usize> = None;
    for line in raw.lines() {
        if let Some(d) = parse_line(line) {
            let key = (d.file.clone(), d.line, d.col, d.message.clone());
            if seen.insert(key) {
                out.push(d);
                open = Some(out.len() - 1);
            } else {
                open = None;
            }
            continue;
        }
        match (open, continuation(line)) {
            (Some(i), Some(text)) => append_continuation(&mut out[i].message, &text),
            // A line that is neither a diagnostic nor a continuation ends the group: javac prints
            // the offending source line and a `^` caret between them, and the next error's
            // continuations must not land on this one.
            (_, None) => {}
            (None, Some(_)) => {}
        }
    }
    out
}

/// javac's continuation keys, normalised to `key: value` with single spaces. `None` for anything
/// else — the echoed source line, the `^` caret, blank lines, and every other build-tool line.
///
/// A closed set rather than "any indented line": build output is full of indented text, and
/// swallowing it would turn one unreadable message into a longer one.
fn continuation(line: &str) -> Option<String> {
    const KEYS: &[&str] = &["symbol", "location", "required", "found", "reason"];
    let (_, body) = strip_mvn_prefix(line.trim_end());
    // Indentation is what marks it as belonging to the line above; javac uses two spaces.
    if !body.starts_with(' ') && !body.starts_with('\t') {
        return None;
    }
    let body = body.trim();
    let (key, value) = body.split_once(':')?;
    let key = key.trim();
    if !KEYS.contains(&key) {
        return None;
    }
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.is_empty() {
        return None;
    }
    Some(format!("{key}: {value}"))
}

/// Fold one continuation into a message, on one line — a lint tooltip renders its text in a single
/// block, so a newline here would come out as a space anyway and read as a run-on.
///
/// `cannot find symbol` + `symbol: method build()` reads as `cannot find symbol: method build()`,
/// because repeating the word is what makes the pair look like two separate facts.
fn append_continuation(message: &mut String, text: &str) {
    if let Some(sym) = text.strip_prefix("symbol: ") {
        if message.ends_with("cannot find symbol") {
            message.push_str(": ");
            message.push_str(sym);
            return;
        }
    }
    message.push_str(" · ");
    message.push_str(text);
}

fn parse_line(line: &str) -> Option<BuildDiagnostic> {
    let (mvn_sev, body) = strip_mvn_prefix(line.trim());
    parse_bracketed(body, mvn_sev).or_else(|| parse_javac(body, mvn_sev))
}

/// Strip a `[ERROR]`/`[WARNING]`/`[INFO]` mvn prefix, returning the implied severity (if
/// the prefix named one) + the remaining body.
fn strip_mvn_prefix(line: &str) -> (Option<&'static str>, &str) {
    for (tag, sev) in
        [("[ERROR]", Some("error")), ("[WARNING]", Some("warning")), ("[INFO]", None)]
    {
        if let Some(rest) = line.strip_prefix(tag) {
            return (sev, rest.trim_start());
        }
    }
    (None, line)
}

/// `<file>:[line,col] message` (maven-compiler-plugin form).
fn parse_bracketed(body: &str, mvn_sev: Option<&'static str>) -> Option<BuildDiagnostic> {
    let lb = body.find(":[")?;
    let rb_rel = body[lb + 2..].find(']')?;
    let inside = &body[lb + 2..lb + 2 + rb_rel];
    let mut nums = inside.split(',');
    let line = nums.next()?.trim().parse::<u32>().ok()?;
    let col = nums.next().and_then(|c| c.trim().parse::<u32>().ok());
    let file = body[..lb].trim().to_string();
    let message = body[lb + 2 + rb_rel + 1..].trim().to_string();
    Some(BuildDiagnostic {
        file: non_empty(file),
        line: Some(line),
        col,
        severity: mvn_sev.unwrap_or("error").to_string(),
        message,
    })
}

/// `<file>:line[:col]: severity: message` (javac form). Drive-letter aware.
fn parse_javac(body: &str, mvn_sev: Option<&'static str>) -> Option<BuildDiagnostic> {
    let (file, after) = split_file_at_line_colon(body)?;
    let bytes = after.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return None;
    }
    let line = after[..i].parse::<u32>().ok()?;
    let mut rest = &after[i..];
    let mut col = None;
    if let Some(r) = rest.strip_prefix(':') {
        let cb = r.as_bytes();
        let mut j = 0;
        while j < cb.len() && cb[j].is_ascii_digit() {
            j += 1;
        }
        if j > 0 {
            col = r[..j].parse::<u32>().ok();
            rest = &r[j..];
        }
    }
    let rest = rest.strip_prefix(':')?.trim_start();
    let (severity, message) = split_severity(rest, mvn_sev);
    Some(BuildDiagnostic { file: non_empty(file.to_string()), line: Some(line), col, severity, message })
}

/// Split `severity: message` off the front; else the mvn-implied severity (or "error").
///
/// The severity may carry a bracketed lint code (`error[E0308]:` from rustc), which is
/// folded into the message as `E0308: …` — the code is the most searchable part of a Rust
/// diagnostic and dropping it would make the Problems row less useful than the raw log.
fn split_severity(rest: &str, mvn_sev: Option<&'static str>) -> (String, String) {
    for word in ["error", "warning", "note"] {
        let Some(after) = rest.strip_prefix(word) else { continue };
        // `error: msg`
        if let Some(msg) = after.strip_prefix(':') {
            return (word.to_string(), msg.trim().to_string());
        }
        // `error[E0308]: msg`
        if let Some((code, msg)) = after
            .strip_prefix('[')
            .and_then(|r| r.split_once(']'))
            .and_then(|(code, tail)| tail.strip_prefix(':').map(|msg| (code, msg)))
        {
            return (word.to_string(), format!("{code}: {}", msg.trim()));
        }
    }
    (mvn_sev.unwrap_or("error").to_string(), rest.to_string())
}

/// Split `body` at the `:` before the line-number digits; a `<letter>:\` / `<letter>:/`
/// drive prefix is part of the file, not a separator.
fn split_file_at_line_colon(body: &str) -> Option<(&str, &str)> {
    let bytes = body.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b != b':' {
            continue;
        }
        let is_drive = i == 1
            && bytes[0].is_ascii_alphabetic()
            && i + 1 < bytes.len()
            && (bytes[i + 1] == b'\\' || bytes[i + 1] == b'/');
        if is_drive {
            continue;
        }
        if i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
            return Some((&body[..i], &body[i + 1..]));
        }
    }
    None
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() { None } else { Some(s) }
}

// ── run registry (cancellation + input) ────────────────────────────────────────

/// A live run the rest of the domain can reach: the handle to kill and the pipe to write to.
///
/// Both are shared with the run thread rather than owned by it. The thread used to own the
/// `Child` outright and park inside `wait()`, which is why cancellation could only ever be
/// bookkeeping — there was no handle left to kill with.
struct LiveRun {
    child: Arc<Mutex<Child>>,
    /// `None` once the pipe is dropped. Behind its own lock so a write doesn't queue behind
    /// the poll loop holding the child.
    stdin: Arc<Mutex<Option<ChildStdin>>>,
}

/// The live `bennu_run` children, by run id.
struct RunRegistry {
    live: Mutex<HashMap<String, LiveRun>>,
}

impl RunRegistry {
    fn global() -> &'static RunRegistry {
        static REG: OnceLock<RunRegistry> = OnceLock::new();
        REG.get_or_init(|| RunRegistry { live: Mutex::new(HashMap::new()) })
    }

    fn register(&self, run_id: &str, child: Arc<Mutex<Child>>, stdin: Option<ChildStdin>) {
        self.live.lock().unwrap_or_else(|p| p.into_inner()).insert(
            run_id.to_string(),
            LiveRun { child, stdin: Arc::new(Mutex::new(stdin)) },
        );
    }

    fn finish(&self, run_id: &str) {
        self.live.lock().unwrap_or_else(|p| p.into_inner()).remove(run_id);
    }

    fn child_of(&self, run_id: &str) -> Option<Arc<Mutex<Child>>> {
        self.live.lock().unwrap_or_else(|p| p.into_inner()).get(run_id).map(|r| r.child.clone())
    }

    fn stdin_of(&self, run_id: &str) -> Option<Arc<Mutex<Option<ChildStdin>>>> {
        self.live.lock().unwrap_or_else(|p| p.into_inner()).get(run_id).map(|r| r.stdin.clone())
    }
}

/// A monotonically-increasing, process-unique run id (`run-<n>-<nanos>`), no `uuid` dep.
fn next_run_id() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("run-{n}-{nanos}")
}

// ── streaming pump ─────────────────────────────────────────────────────────────

/// Spawn a thread that reads `reader` line-by-line, **interprets** each line and emits it as
/// a `run-output` event tagged with the stream name. Lossy on non-UTF-8 (a run's stdout is
/// text).
///
/// The interpretation ([`LogAnnotator`]) travels with the line rather than being done in the
/// frontend: the level, the URLs and the paths are the same work whoever does them, and the
/// stack frames are not — resolving `com.acme.Order` to a file needs the class index, which
/// lives here. Each pump gets its own annotator (level inheritance is per stream) over the
/// shared class map.
fn spawn_pump<R: std::io::Read + Send + 'static>(
    reader: R,
    stream: &'static str,
    run_id: String,
    sink: Arc<dyn EventSink>,
    classes: ClassMap,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut log = LogAnnotator::new(classes);
        let buf = BufReader::new(reader);
        for line in buf.lines().map_while(Result::ok) {
            let mut payload = log.line(&line);
            if let Some(map) = payload.as_object_mut() {
                map.insert("run_id".into(), json!(run_id));
                map.insert("stream".into(), json!(stream));
            }
            sink.emit(EVT_RUN_OUTPUT, payload);
        }
    })
}

// ── program / path resolution ──────────────────────────────────────────────────

/// Resolve `JAVA_HOME` for the project: [its own JDK level](project_jdk_level) → an installed
/// JDK home. `None` when no JDK of that level is installed — the child then inherits the
/// ambient `JAVA_HOME` / `PATH`, which on a developer machine is very likely the right one.
///
/// It used to read ONLY the config override and default to JDK 8 whenever there wasn't one —
/// so a project declaring Java 21 in its pom had `JAVA_HOME` forced to an installed JDK 8 and
/// Maven answered `invalid target release: 21`, on a machine whose own `JAVA_HOME` was 21.
/// Two resolutions of the same question, and the build used the one that never looked at the
/// project.
///
/// The `None` matters as much as the match: falling back to *some other* JDK when the requested
/// one is absent (as this did, to 8) guarantees a wrong compile with a confusing error, whereas
/// deferring to the environment at least fails the way running `mvn` by hand would.
pub(crate) fn resolve_java_home(root: &str) -> Option<PathBuf> {
    let version = project_jdk_level(root);
    let home = find_jdk_home(&version);
    if home.is_none() {
        // Not fatal — the child inherits the ambient JAVA_HOME — but worth a line, because the
        // failure it leads to (a version error from javac) never names the JDK that caused it.
        eprintln!("bennu-be: no installed JDK for level {version:?}; leaving JAVA_HOME to the environment");
    }
    home
}

/// The Java language level the project at `root` targets, from the open project's slot — the
/// same answer the index was built with and the titlebar badge shows (override → pom
/// `maven.compiler.release`/`source`/`target` → `java.version` → the compiler plugin). Falls
/// back to the config override, then to the target stack's JDK 8, for the window where no slot
/// exists yet.
fn project_jdk_level(root: &str) -> String {
    if let Some(v) = IndexService::global().jdk_version_of(root).filter(|v| !v.is_empty()) {
        return v;
    }
    let cfg = bennu_core::config::load();
    cfg.jdk_overrides.get(root).cloned().unwrap_or_else(|| DEFAULT_JDK.to_string())
}

/// The `mvn` launcher for `root` — the SAME resolution the dependency tier uses
/// ([`crate::dep_classpath::find_mvn_launcher`]): `PATH` preferring the Windows batch
/// launchers, then the well-known install dirs, then the project's `mvnw`.
///
/// It used to return the bare `"mvn"`, and on Windows that is not a launcher: Maven ships
/// `mvn.cmd`, and `Command::new("mvn")` only ever locates `mvn.exe`. The spawn therefore
/// failed on every Windows machine — invisibly, because [`compile`] treats a failed spawn as
/// "fall back to `javac`". The Build button appeared to work while quietly compiling without
/// the dependency classpath, and the bug only surfaced from the test runner, which has no
/// fallback to hide behind. Two ways of finding the same program, one of them wrong.
pub(crate) fn resolve_mvn(root: &Path) -> String {
    crate::dep_classpath::find_mvn_launcher(root)
}

/// The `cargo` launcher: `cargo` on `PATH`. A rustup install puts it there on every
/// platform, so unlike `mvn` there is no per-OS launcher name to resolve.
// Resolved, not taken from `PATH`: see `cargo_cmd::cargo_launcher`.
use crate::cargo_cmd::cargo_launcher;

/// The `javac` program under `JAVA_HOME/bin`, else `javac` on `PATH`.
fn javac_program(java_home: Option<&Path>) -> String {
    bin_under(java_home, "javac").unwrap_or_else(|| "javac".to_string())
}

/// The `java` program under `JAVA_HOME/bin`, else `java` on `PATH`.
fn java_program(java_home: Option<&Path>) -> String {
    bin_under(java_home, "java").unwrap_or_else(|| "java".to_string())
}

fn bin_under(java_home: Option<&Path>, tool: &str) -> Option<String> {
    let jh = java_home?;
    let exe = if cfg!(windows) { format!("{tool}.exe") } else { tool.to_string() };
    let p = jh.join("bin").join(exe);
    p.is_file().then(|| p.display().to_string())
}

// ── fs helpers ─────────────────────────────────────────────────────────────────

/// The Maven-standard source roots that exist under `root`; falls back to `root` itself
/// for a non-standard layout.
fn source_roots(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for rel in ["src/main/java", "src/main/resources"] {
        let p = root.join(rel);
        if p.is_dir() {
            out.push(p);
        }
    }
    if out.is_empty() {
        out.push(root.to_path_buf());
    }
    out
}

/// Recursively collect `.java` files under `dir`, skipping `target` / hidden dirs.
fn collect_java(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name == "target" || name.starts_with('.') {
                continue;
            }
            collect_java(&p, out);
        } else if p.extension().and_then(|x| x.to_str()) == Some("java") {
            out.push(p);
        }
    }
}

fn merge_output(stdout: &[u8], stderr: &[u8]) -> String {
    let mut s = String::from_utf8_lossy(stdout).into_owned();
    if !stderr.is_empty() {
        if !s.is_empty() && !s.ends_with('\n') {
            s.push('\n');
        }
        s.push_str(&String::from_utf8_lossy(stderr));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plain_javac_error() {
        let d = parse_diagnostics("src/main/java/it/foo/Bar.java:12: error: cannot find symbol");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].file.as_deref(), Some("src/main/java/it/foo/Bar.java"));
        assert_eq!(d[0].line, Some(12));
        assert_eq!(d[0].severity, "error");
        assert_eq!(d[0].message, "cannot find symbol");
    }

    /// `cannot find symbol` on its own names nothing. javac puts the name on the next line and the
    /// place it looked on the one after, and reading the output line by line dropped both — the
    /// marker in the buffer said `cannot find symbol  (build)` and left the reader to go and find
    /// out which symbol.
    #[test]
    fn javac_continuation_lines_are_folded_into_their_diagnostic() {
        let raw = "Foo.java:12: error: cannot find symbol\n\
                   \x20       Arrays.asList(xs)\n\
                   \x20       ^\n\
                   \x20 symbol:   variable Arrays\n\
                   \x20 location: class Foo\n";
        let d = parse_diagnostics(raw);
        assert_eq!(d.len(), 1, "the source echo and the caret are not diagnostics: {d:?}");
        assert_eq!(
            d[0].message,
            "cannot find symbol: variable Arrays · location: class Foo"
        );
    }

    /// mvn echoes each javac error twice — raw, then wrapped in `[ERROR]` — continuations and all.
    /// The duplicate is dropped, and so are ITS continuations: appending them to the surviving copy
    /// would say everything twice.
    #[test]
    fn a_duplicated_diagnostics_continuations_are_not_appended_twice() {
        let raw = "Foo.java:12: error: cannot find symbol\n\
                   \x20 symbol:   variable Arrays\n\
                   [ERROR] Foo.java:12: error: cannot find symbol\n\
                   [ERROR]   symbol:   variable Arrays\n";
        let d = parse_diagnostics(raw);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].message, "cannot find symbol: variable Arrays");
    }

    /// Only javac's own continuation keys are absorbed. Build output is full of indented text, and
    /// swallowing it would turn one unreadable message into a longer one.
    #[test]
    fn indented_build_noise_is_not_absorbed() {
        let raw = "Foo.java:12: error: cannot find symbol\n\
                   \x20 at org.apache.maven.Something.run(Something.java:1)\n\
                   \x20 Downloading from central: https://repo/x.jar\n";
        let d = parse_diagnostics(raw);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].message, "cannot find symbol");
    }

    /// The `required` / `found` pair, which is the other message javac splits across lines.
    #[test]
    fn a_type_mismatch_keeps_both_halves() {
        let raw = "Foo.java:4: error: incompatible types\n\
                   \x20 required: int\n\
                   \x20 found: java.lang.String\n";
        let d = parse_diagnostics(raw);
        assert_eq!(
            d[0].message,
            "incompatible types · required: int · found: java.lang.String"
        );
    }

    #[test]
    fn parses_javac_warning() {
        let d = parse_diagnostics(
            "Foo.java:3: warning: [deprecation] Thread.stop() in Thread has been deprecated",
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, "warning");
        assert_eq!(d[0].line, Some(3));
        assert!(d[0].message.contains("deprecation"));
    }

    #[test]
    fn parses_javac_line_col() {
        let d = parse_diagnostics("Foo.java:12:7: error: ';' expected");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].line, Some(12));
        assert_eq!(d[0].col, Some(7));
        assert_eq!(d[0].message, "';' expected");
    }

    #[test]
    fn parses_mvn_bracketed_error() {
        let d = parse_diagnostics(
            "[ERROR] /home/u/proj/src/main/java/Foo.java:[45,17] cannot find symbol",
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].file.as_deref(), Some("/home/u/proj/src/main/java/Foo.java"));
        assert_eq!(d[0].line, Some(45));
        assert_eq!(d[0].col, Some(17));
        assert_eq!(d[0].severity, "error");
        assert_eq!(d[0].message, "cannot find symbol");
    }

    #[test]
    fn parses_windows_drive_path() {
        let d = parse_diagnostics(
            r"C:\Sviluppo\proj\src\Foo.java:88: error: incompatible types: int cannot be converted to String",
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].file.as_deref(), Some(r"C:\Sviluppo\proj\src\Foo.java"));
        assert_eq!(d[0].line, Some(88));
        assert!(d[0].message.contains("incompatible types"));
    }

    #[test]
    fn parses_windows_drive_bracketed() {
        let d = parse_diagnostics(r"[ERROR] C:\proj\src\main\java\it\Foo.java:[10,5] ';' expected");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].file.as_deref(), Some(r"C:\proj\src\main\java\it\Foo.java"));
        assert_eq!(d[0].line, Some(10));
        assert_eq!(d[0].col, Some(5));
    }

    #[test]
    fn dedups_double_reported() {
        let d = parse_diagnostics(
            "Foo.java:12: error: cannot find symbol\n\
             [ERROR] Foo.java:12: error: cannot find symbol",
        );
        assert_eq!(d.len(), 1, "identical diagnostic should be de-duped");
    }

    #[test]
    fn parses_rustc_short_format() {
        let d = parse_diagnostics(
            "crates/products/bennu/be/src/build.rs:12:5: error[E0308]: mismatched types",
        );
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].file.as_deref(), Some("crates/products/bennu/be/src/build.rs"));
        assert_eq!(d[0].line, Some(12));
        assert_eq!(d[0].col, Some(5));
        assert_eq!(d[0].severity, "error");
        // The lint code is kept — it is the searchable half of a Rust diagnostic.
        assert_eq!(d[0].message, "E0308: mismatched types");
    }

    #[test]
    fn parses_rustc_warning_without_a_code() {
        let d = parse_diagnostics("src/lib.rs:7:9: warning: unused variable: `x`");
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].severity, "warning");
        assert_eq!(d[0].line, Some(7));
        assert_eq!(d[0].message, "unused variable: `x`");
    }

    #[test]
    fn ignores_cargo_summary_lines() {
        // Cargo's own chatter carries no `file:line`, so it must not become a diagnostic
        // pinned to nowhere.
        let d = parse_diagnostics(
            "    Checking bennu-project v0.3.0 (/p/crates/products/bennu/project)\n\
             error: could not compile `bennu-project` (lib) due to 1 previous error\n\
             warning: `bennu-be` (lib) generated 3 warnings",
        );
        assert!(d.is_empty(), "no source:line → no diagnostics, got {d:?}");
    }

    #[test]
    fn ignores_non_diagnostic_lines() {
        let d = parse_diagnostics(
            "[INFO] Building proj 1.0\n\
             [INFO] BUILD FAILURE\n\
             Downloading from central: https://repo/foo.jar\n\
             [INFO] Total time: 3.2 s",
        );
        assert!(d.is_empty(), "no source:line lines → no diagnostics, got {d:?}");
    }

    #[test]
    fn run_classpath_puts_target_classes_first() {
        // No mvn / no deps needed: a nonexistent project resolves no dep jars, so the
        // classpath is just target/classes — which is what we assert leads.
        let root = Path::new(if cfg!(windows) { r"C:\definitely\missing\proj" } else { "/definitely/missing/proj" });
        let cp = run_classpath(root, None, None, "runtime");
        let sep = if cfg!(windows) { ";" } else { ":" };
        let first = cp.split(sep).next().unwrap();
        assert!(first.ends_with("classes"), "target/classes must lead: {cp}");
    }

    /// The module's own output leads, and the root's is still there behind it. The bug this
    /// guards: on a reactor the root compiles nothing, so a classpath built from the root
    /// alone contains one directory that does not exist.
    #[test]
    fn run_classpath_leads_with_the_module() {
        let root = Path::new(if cfg!(windows) { r"C:\definitely\missing\proj" } else { "/definitely/missing/proj" });
        let cp = run_classpath(root, Some("services/core"), None, "runtime");
        let sep = if cfg!(windows) { ";" } else { ":" };
        let entries: Vec<&str> = cp.split(sep).collect();
        assert!(
            entries[0].replace('\\', "/").ends_with("services/core/target/classes"),
            "the launched module's classes must lead: {cp}",
        );
        assert!(
            entries.iter().any(|e| {
                let e = e.replace('\\', "/");
                e.ends_with("proj/target/classes")
            }),
            "the root's classes must still be on it: {cp}",
        );
    }

    /// A reactor is walked through its poms, and a pom naming itself as a module cannot
    /// spin the walk.
    #[test]
    fn module_dirs_reads_the_reactor_and_survives_a_cycle() {
        let dir = std::env::temp_dir().join(format!("bennu-reactor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |rel: &str, xml: &str| {
            let p = dir.join(rel);
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("pom.xml"), xml).unwrap();
        };
        // root → core, web; web → api (nested); api names itself (the cycle).
        write("", "<project><modules><module>core</module><module>web</module></modules></project>");
        write("core", "<project></project>");
        write("web", "<project><modules><module>api</module></modules></project>");
        write("web/api", "<project><modules><module>.</module></modules></project>");

        let mods = module_dirs(&dir);
        let rel: std::collections::HashSet<String> = mods
            .iter()
            .map(|p| p.strip_prefix(&dir).unwrap().to_string_lossy().replace('\\', "/"))
            .collect();
        assert!(rel.contains("core"), "{rel:?}");
        assert!(rel.contains("web"), "{rel:?}");
        assert!(rel.contains("web/api"), "the walk must descend into nested modules: {rel:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The console's first line: pasteable, and with the classpath summarised rather than
    /// spelled out — a resolved `~/.m2` classpath is tens of thousands of characters.
    #[test]
    fn display_command_is_readable_and_pasteable() {
        let sep = if cfg!(windows) { ";" } else { ":" };
        let cp = format!("target/classes{sep}/home/u/.m2/a.jar{sep}/home/u/.m2/b.jar");
        let line = display_command(
            "/opt/jdk21/bin/java",
            &["-Xmx512m".into()],
            &cp,
            &ClasspathForm::Inline,
            "com.acme.App",
            &["--port".into(), "input file.txt".into()],
        );
        assert_eq!(
            line,
            "/opt/jdk21/bin/java -Xmx512m -cp <classpath: 3 entries> com.acme.App --port \"input file.txt\"",
        );
        // The jar paths themselves must not be in there — that is the whole point.
        assert!(!line.contains(".m2"), "the classpath must be summarised: {line}");
    }

    /// The stamp is stable when nothing moves, and changes when a source OR a resource does.
    /// Resources matter as much as sources here: an edited `application.yml` changes what the
    /// program does, and a stamp that ignored it would launch the previous one.
    #[test]
    fn source_stamp_notices_sources_and_resources() {
        let dir = std::env::temp_dir().join(format!("bennu-stamp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let write = |rel: &str, text: &str| {
            let p = dir.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, text).unwrap();
        };
        write("pom.xml", "<project></project>");
        write("src/main/java/com/acme/App.java", "class App {}\n");

        let first = source_stamp(&dir);
        assert_eq!(first, source_stamp(&dir), "an untouched tree stamps the same twice");

        write("src/main/java/com/acme/App.java", "class App { void x() {} }\n");
        let after_source = source_stamp(&dir);
        assert_ne!(first, after_source, "an edited source must change the stamp");

        write("src/main/resources/application.yml", "server:\n  port: 8080\n");
        assert_ne!(after_source, source_stamp(&dir), "a new resource must change the stamp");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A short classpath stays on the command line, where it is readable.
    #[test]
    fn a_short_classpath_goes_inline() {
        let dir = std::env::temp_dir();
        assert!(matches!(classpath_form(&dir, "a.jar;b.jar", 21), ClasspathForm::Inline));
    }

    /// A long one on a modern JDK becomes an argument file — with FORWARD slashes, because a
    /// backslash inside an argument file is an escape character and `C:\lib\x.jar` would
    /// arrive as `C:libx.jar`.
    #[test]
    fn a_long_classpath_becomes_an_argfile_with_forward_slashes() {
        let base = std::env::temp_dir().join(format!("bennu-argfile-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let long = std::iter::repeat(r"C:\Users\u\.m2\repository\org\acme\lib.jar")
            .take(400)
            .collect::<Vec<_>>()
            .join(";");
        assert!(long.len() > MAX_INLINE_CLASSPATH);

        match classpath_form(&base, &long, 21) {
            ClasspathForm::ArgFile(p) => {
                let body = std::fs::read_to_string(&p).unwrap();
                assert!(body.starts_with("-cp \""), "the file holds the -cp: {body:.40}");
                assert!(!body.contains('\\'), "backslashes would be read as escapes");
                assert!(body.contains("C:/Users/u/.m2/repository/org/acme/lib.jar"));
            }
            _ => panic!("a classpath over the limit must not go on the command line"),
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A JDK 8 has no argument files, so the same classpath goes to the environment.
    #[test]
    fn a_long_classpath_on_java_8_goes_to_the_environment() {
        let dir = std::env::temp_dir();
        let long = "x".repeat(MAX_INLINE_CLASSPATH + 1);
        assert!(matches!(classpath_form(&dir, &long, 8), ClasspathForm::Environment));
    }

    #[test]
    fn jdk_major_reads_both_spellings() {
        assert_eq!(jdk_major("1.8"), 8);
        assert_eq!(jdk_major("8"), 8);
        assert_eq!(jdk_major("21"), 21);
        assert_eq!(jdk_major("21.0.6"), 21);
        // Unrecognised reads as "older than 9" — the conservative path.
        assert_eq!(jdk_major("toolchains"), 0);
    }

    #[test]
    fn quoting_only_kicks_in_for_whitespace() {
        assert_eq!(quoted("-Xmx512m"), "-Xmx512m");
        assert_eq!(quoted("C:/Program Files/jdk/bin/java.exe"), "\"C:/Program Files/jdk/bin/java.exe\"");
    }
}
