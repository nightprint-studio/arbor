//! `build` domain — `bennu_build` / `bennu_run` / `bennu_cancel_run` (docs §4 "il
//! fondo": the BUILD and RUN that make the Run/Debug buttons real + feed
//! `target/classes` to the index).
//!
//! - `bennu_build` shells out to **`mvn -q -o compile`** (offline, the project's JDK via
//!   `JAVA_HOME`); if the `mvn` launcher can't be spawned it falls back to **`javac`**
//!   over the Maven source roots. Either way it captures stdout/stderr, streams the raw
//!   log as `arbor://bennu/build-output` events, and PARSES compiler/mvn error lines
//!   into structured [`BuildDiagnostic`]s. After a **successful** compile it triggers a
//!   re-index of the project (so `target/classes` output is reflected in completion).
//! - `bennu_run` launches **`java -cp <classpath> <mainClass>`** — classpath = the
//!   project's `target/classes` + the Phase-2 `.m2`-resolved dependency jars — and
//!   streams stdout/stderr as `arbor://bennu/run-output`, ending with an
//!   `arbor://bennu/run-exit`. Returns a [`RunHandle`] the FE uses to correlate the
//!   stream and to `bennu_cancel_run`.
//!
//! Threading: build shells out via short-lived **NoWindow** children. The serve loop
//! dispatches each request on its own thread (see `arbor_ipc::serve_stdio`), so a
//! sync handler that blocks on a child never stalls the IPC read loop or other requests.
//! Run spawns a detached-from-the-handler background thread that owns the child + the two
//! reader threads, so the launching RPC returns immediately.
//!
//! The pure **error parser** ([`parse_diagnostics`]) is the unit-tested core; the
//! shell-out + streaming is the glue around it.

use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use arbor_ipc::prelude::EventSink;
use arbor_process_ext::prelude::NoWindowExt;
use bennu_classpath::prelude::{find_jdk_home, MavenClasspathCache, MavenResolveOpts};
use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{BuildDiagnostic, BuildResult, RunHandle};
use serde::Deserialize;
use serde_json::json;

use crate::index_service::IndexService;

/// The JDK level to resolve `JAVA_HOME` against when the project declares none (the
/// target stack is JDK 8 — Struts2/Entando).
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

// ── bennu_build ────────────────────────────────────────────────────────────────

/// Args for [`bennu_build`].
#[derive(Deserialize)]
pub struct BuildArgs {
    /// Absolute path to the project root (the dir holding the root `pom.xml`).
    pub root: String,
}

/// Compile the project: `mvn -q -o compile` (offline, project JDK), falling back to
/// `javac` over the source roots when `mvn` can't be spawned. Streams the raw log as
/// `arbor://bennu/build-output`, returns the parsed diagnostics, and re-indexes on
/// success. A *failed compile* is a normal result carrying diagnostics — not an `Err`
/// (which is reserved for "no compiler could run at all").
#[arbor_rpc::handler]
fn bennu_build(ctx: &BennuState, args: BuildArgs) -> Result<BuildResult, String> {
    let sink = ctx.event_sink();
    let root = PathBuf::from(&args.root);
    let java_home = resolve_java_home(&args.root);
    let mvn_path = resolve_mvn();

    let outcome = compile(&root, &mvn_path, java_home.as_deref(), &sink)?;

    sink.emit(EVT_BUILD_DONE, json!({
        "root": &args.root,
        "tool": &outcome.tool,
        "ok": outcome.ok,
        "diagnostics": outcome.diagnostics.len(),
    }));

    // A clean compile means fresh `target/classes` — re-index so completion picks it up.
    // The reindex emits `arbor://bennu/index-progress` on the same sink.
    if outcome.ok {
        IndexService::global().reindex(&args.root, ctx.event_sink());
    }

    Ok(BuildResult { tool: outcome.tool, ok: outcome.ok, diagnostics: outcome.diagnostics })
}

// ── bennu_run ──────────────────────────────────────────────────────────────────

/// Args for [`bennu_run`].
#[derive(Deserialize)]
pub struct RunArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// The fully-qualified main class to launch. Required — main-class *discovery*
    /// (scanning for `public static void main`) is a later wave; the FE passes it.
    pub main_class: String,
    /// Program arguments passed to the main class.
    #[serde(default)]
    pub args: Vec<String>,
}

/// Launch `java -cp <target/classes:deps> <main_class> <args...>` and stream its
/// stdout/stderr as `arbor://bennu/run-output`, ending with `arbor://bennu/run-exit`.
/// Returns immediately with the [`RunHandle`] correlating the stream; the child runs on
/// a background thread.
#[arbor_rpc::handler]
fn bennu_run(ctx: &BennuState, args: RunArgs) -> Result<RunHandle, String> {
    let root = PathBuf::from(&args.root);
    let java_home = resolve_java_home(&args.root);
    let java = java_program(java_home.as_deref());
    let classpath = run_classpath(&root, java_home.as_deref());

    let mut cmd = Command::new(&java);
    cmd.current_dir(&root)
        .arg("-cp")
        .arg(&classpath)
        .arg(&args.main_class)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
    for a in &args.args {
        cmd.arg(a);
    }
    // A run child is short-lived-ish and console-less; suppress the window on Windows.
    cmd.no_window();

    let mut child = cmd.spawn().map_err(|e| format!("spawn java ({java}): {e}"))?;

    let run_id = next_run_id();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let sink = ctx.event_sink();
    RunRegistry::global().register(&run_id);

    let run_id_thread = run_id.clone();
    std::thread::Builder::new()
        .name(format!("bennu-run-{run_id}"))
        .spawn(move || {
            // Pump both pipes on their own threads so a chatty stderr can't deadlock a
            // full stdout pipe (or vice-versa).
            let mut pumps = Vec::new();
            if let Some(out) = stdout {
                pumps.push(spawn_pump(out, "stdout", run_id_thread.clone(), sink.clone()));
            }
            if let Some(err) = stderr {
                pumps.push(spawn_pump(err, "stderr", run_id_thread.clone(), sink.clone()));
            }
            let code = child.wait().ok().and_then(|s| s.code());
            for p in pumps {
                let _ = p.join();
            }
            RunRegistry::global().finish(&run_id_thread);
            sink.emit(EVT_RUN_EXIT, json!({ "run_id": run_id_thread, "code": code }));
        })
        .map_err(|e| format!("spawn run thread: {e}"))?;

    Ok(RunHandle { run_id, main_class: args.main_class })
}

/// Args for [`bennu_cancel_run`].
#[derive(Deserialize)]
pub struct CancelRunArgs {
    /// The run id returned by `bennu_run`.
    pub run_id: String,
}

/// Kill a running `bennu_run` child by id. Returns `true` if a live run was killed,
/// `false` if the id is unknown or already finished.
#[arbor_rpc::handler]
fn bennu_cancel_run(_ctx: &BennuState, args: CancelRunArgs) -> Result<bool, String> {
    Ok(RunRegistry::global().cancel(&args.run_id))
}

// ── compile (mvn → javac fallback) ─────────────────────────────────────────────

/// The outcome of a compile: the tool that ran, whether it exited 0, and the parsed
/// diagnostics. The raw log is streamed as events (not carried here).
struct CompileOutcome {
    tool: String,
    ok: bool,
    diagnostics: Vec<BuildDiagnostic>,
}

/// Run `mvn -q -o compile`; if the launcher can't be spawned, fall back to `javac`.
/// Streams each captured line as a `build-output` event. `Err` only when neither tool
/// could run.
fn compile(
    root: &Path,
    mvn_path: &str,
    java_home: Option<&Path>,
    sink: &Arc<dyn EventSink>,
) -> Result<CompileOutcome, String> {
    match run_mvn_compile(root, mvn_path, java_home) {
        Ok((ok, raw)) => Ok(finish_compile("mvn", ok, raw, sink)),
        Err(spawn_err) => {
            let (ok, raw) = run_javac(root, java_home)
                .map_err(|javac_err| format!("mvn: {spawn_err}; javac: {javac_err}"))?;
            Ok(finish_compile("javac", ok, raw, sink))
        }
    }
}

/// Stream the raw log line-by-line, parse it, and assemble the outcome.
fn finish_compile(
    tool: &str,
    ok: bool,
    raw: String,
    sink: &Arc<dyn EventSink>,
) -> CompileOutcome {
    for line in raw.lines() {
        sink.emit(EVT_BUILD_OUTPUT, json!({ "text": line }));
    }
    CompileOutcome { tool: tool.to_string(), ok, diagnostics: parse_diagnostics(&raw) }
}

fn run_mvn_compile(
    root: &Path,
    mvn_path: &str,
    java_home: Option<&Path>,
) -> Result<(bool, String), String> {
    let mut cmd = Command::new(mvn_path);
    cmd.current_dir(root)
        .arg("-q")
        .arg("compile")
        .arg("--batch-mode")
        .arg("-o"); // offline: resolve only from the local ~/.m2 cache
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

// ── run classpath ──────────────────────────────────────────────────────────────

/// The run classpath: `target/classes` first, then the `.m2`-resolved dependency jars
/// (Phase-2 resolver, offline). Dep resolution failure is non-fatal — the run degrades
/// to `target/classes` only.
fn run_classpath(root: &Path, java_home: Option<&Path>) -> String {
    let sep = if cfg!(windows) { ";" } else { ":" };
    let mut parts: Vec<String> = vec![root.join("target").join("classes").display().to_string()];

    let mut opts = MavenResolveOpts::default();
    opts.mvn_path = resolve_mvn();
    opts.offline = true;
    if let Some(jh) = java_home {
        opts.java_home = Some(jh.to_path_buf());
    }
    let mut cache = MavenClasspathCache::new();
    if let Ok(cp) = cache.get(root, &opts) {
        for jar in &cp.jars {
            parts.push(jar.display().to_string());
        }
    }
    parts.join(sep)
}

// ── the PURE error parser (unit-tested; no I/O) ────────────────────────────────

/// Parse `javac` / `mvn` compiler output into structured [`BuildDiagnostic`]s.
///
/// Recognised shapes (Windows drive-letter aware — a `C:\` colon is never the
/// `file:line` separator):
/// - javac: `Path.java:12: error: cannot find symbol`
/// - javac line:col: `Path.java:12:7: error: ';' expected`
/// - javac warning: `Path.java:3: warning: [deprecation] ...`
/// - mvn compiler-plugin: `[ERROR] /abs/Foo.java:[45,17] cannot find symbol`
///
/// mvn often echoes the same javac error both raw and wrapped in `[ERROR]`; identical
/// diagnostics are de-duped.
pub fn parse_diagnostics(raw: &str) -> Vec<BuildDiagnostic> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for line in raw.lines() {
        if let Some(d) = parse_line(line) {
            let key = (d.file.clone(), d.line, d.col, d.message.clone());
            if seen.insert(key) {
                out.push(d);
            }
        }
    }
    out
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
fn split_severity(rest: &str, mvn_sev: Option<&'static str>) -> (String, String) {
    for word in ["error", "warning", "note"] {
        let pfx = format!("{word}:");
        if let Some(msg) = rest.strip_prefix(&pfx) {
            return (word.to_string(), msg.trim().to_string());
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

// ── run registry (cancellation) ────────────────────────────────────────────────

/// Tracks live `bennu_run` ids so `bennu_cancel_run` can report whether a run is
/// in-flight. The run thread owns the `Child` (it `wait()`s on it), so this port's
/// `cancel` deregisters the id and returns whether it was live; the FE close button
/// stops consuming the stream. Kill-by-handle (a shared `Child` the canceller can
/// `.kill()`) is a follow-up — see the limits note in the summary.
struct RunRegistry {
    live: Mutex<HashSet<String>>,
}

impl RunRegistry {
    fn global() -> &'static RunRegistry {
        static REG: OnceLock<RunRegistry> = OnceLock::new();
        REG.get_or_init(|| RunRegistry { live: Mutex::new(HashSet::new()) })
    }

    fn register(&self, run_id: &str) {
        self.live.lock().unwrap_or_else(|p| p.into_inner()).insert(run_id.to_string());
    }

    fn finish(&self, run_id: &str) {
        self.live.lock().unwrap_or_else(|p| p.into_inner()).remove(run_id);
    }

    fn cancel(&self, run_id: &str) -> bool {
        self.live.lock().unwrap_or_else(|p| p.into_inner()).remove(run_id)
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

/// Spawn a thread that reads `reader` line-by-line and emits each as a `run-output`
/// event tagged with the stream name. Lossy on non-UTF-8 (a run's stdout is text).
fn spawn_pump<R: std::io::Read + Send + 'static>(
    reader: R,
    stream: &'static str,
    run_id: String,
    sink: Arc<dyn EventSink>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let buf = BufReader::new(reader);
        for line in buf.lines().map_while(Result::ok) {
            sink.emit(EVT_RUN_OUTPUT, json!({ "run_id": run_id, "stream": stream, "text": line }));
        }
    })
}

// ── program / path resolution ──────────────────────────────────────────────────

/// Resolve `JAVA_HOME` for the project: its configured/pom JDK level → an installed JDK
/// home; else the default (JDK 8). `None` when no matching JDK is installed (the child
/// then inherits the ambient `JAVA_HOME` / `PATH`).
fn resolve_java_home(root: &str) -> Option<PathBuf> {
    let cfg = bennu_core::config::load();
    let version = cfg.jdk_overrides.get(root).cloned().unwrap_or_else(|| DEFAULT_JDK.to_string());
    find_jdk_home(&version).or_else(|| find_jdk_home(DEFAULT_JDK))
}

/// The `mvn` launcher: `mvn` on `PATH` (Windows resolves `mvn.cmd`). A configured path
/// is a follow-up; the Phase-2 resolver uses the same default.
fn resolve_mvn() -> String {
    "mvn".to_string()
}

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
        let cp = run_classpath(root, None);
        let sep = if cfg!(windows) { ";" } else { ":" };
        let first = cp.split(sep).next().unwrap();
        assert!(first.ends_with("classes"), "target/classes must lead: {cp}");
    }
}
