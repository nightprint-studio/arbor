//! `profiler` domain — recording a program and reading the recording back.
//!
//! The flow, for every language, is the same four verbs:
//!
//! 1. `bennu_profile_detect` — what this machine can profile with, and why not when it cannot.
//! 2. `bennu_profile_prepare` — a session: scratch dir + the [`StartPlan`] (JVM flags, env, a
//!    wrapper command). The caller starts the process with the plan (`bennu_run` for Java, which
//!    already assembles the project's classpath; `bennu_profile_run_command` for the rest) so the
//!    run shows up in the Run console like any other.
//! 3. `bennu_profile_finish` — parse on a background thread, persist, emit
//!    `arbor://bennu/profile-done`.
//! 4. Reading: list / get / flame / compare / file metrics.
//!
//! Parsing is streamed and capped — see [`bennu_profile`] — and never runs on the request thread.

mod decls;
mod detect;
mod parse;
mod prefs;
mod samply;
mod store;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use bennu_core::prelude::BennuState;
use bennu_profile::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

use store::{now_ms, ProfileMeta};

pub const EVT_PROGRESS: &str = "arbor://bennu/profile-progress";
pub const EVT_DONE: &str = "arbor://bennu/profile-done";

// ── sessions ──────────────────────────────────────────────────────────────────

struct Session {
    root: String,
    lang: Lang,
    dir: PathBuf,
    recording: PathBuf,
    label: String,
    started_ms: u64,
    how: Option<ParseHow>,
    plan: StartPlan,
    jcmd: Option<PathBuf>,
}

fn sessions() -> &'static Mutex<HashMap<String, Session>> {
    static S: OnceLock<Mutex<HashMap<String, Session>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn session_id(lang: Lang) -> String {
    let l = match lang {
        Lang::Java => "java",
        Lang::Node => "node",
        Lang::Rust => "rust",
    };
    format!("{l}-{}", radix36(now_ms()))
}

fn radix36(mut n: u64) -> String {
    const D: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if n == 0 {
        return "0".into();
    }
    let mut v = Vec::new();
    while n > 0 {
        v.push(D[(n % 36) as usize]);
        n /= 36;
    }
    v.reverse();
    String::from_utf8(v).unwrap_or_default()
}

// ── detect ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct RootArgs {
    pub root: String,
}

#[derive(Serialize)]
pub struct DetectOut {
    pub java: JfrAvailability,
    pub jdk_home: Option<String>,
    pub can_attach: bool,
    pub node: Option<String>,
    pub samply: Option<String>,
}

fn find_tool(name: &str) -> Option<String> {
    arbor_process_ext::prelude::locate_executable(name, None, &[], &[])
}

/// What each language's profiler needs, and whether this machine has it.
#[arbor_rpc::handler]
fn bennu_profile_detect(_ctx: &BennuState, args: RootArgs) -> Result<DetectOut, String> {
    let jdk = detect::probe_jdk(&args.root);
    Ok(DetectOut {
        can_attach: jdk.jcmd.is_some() && jdk.availability.available,
        jdk_home: jdk.home.map(|h| h.display().to_string()),
        java: jdk.availability,
        node: find_tool("node"),
        samply: samply::resolve_configured().ok().flatten().map(|r| r.path.display().to_string()),
    })
}

#[arbor_rpc::handler]
fn bennu_profile_list_jvms(_ctx: &BennuState, args: RootArgs) -> Result<Vec<detect::LocalJvm>, String> {
    detect::list_jvms(&args.root)
}

// ── prepare ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct PrepareArgs {
    pub root: String,
    /// `java` | `node` | `rust`.
    pub lang: String,
    pub label: String,
    /// Attach to this JVM instead of launching (Java only).
    #[serde(default)]
    pub pid: Option<u32>,
    /// Explicit `samply` binary (Rust); otherwise looked up on PATH.
    #[serde(default)]
    pub samply_path: Option<String>,
    #[serde(default)]
    pub rate_hz: Option<u32>,
    /// Java: the execution sampler's interval in milliseconds (default 10). Honoured on JDK 17 and newer.
    #[serde(default)]
    pub sampling_ms: Option<u32>,
}

#[derive(Serialize)]
pub struct PrepareOut {
    pub session_id: String,
    pub plan: StartPlan,
}

fn parse_lang(s: &str) -> Result<Lang, String> {
    match s {
        "java" => Ok(Lang::Java),
        "node" => Ok(Lang::Node),
        "rust" => Ok(Lang::Rust),
        other => Err(format!("unknown language `{other}`")),
    }
}

#[arbor_rpc::handler]
fn bennu_profile_prepare(_ctx: &BennuState, args: PrepareArgs) -> Result<PrepareOut, String> {
    let lang = parse_lang(&args.lang)?;
    let id = session_id(lang);
    let dir = std::env::temp_dir().join("arbor-bennu-profile").join(&id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let ext = match lang {
        Lang::Java => "jfr",
        Lang::Node => "cpuprofile",
        Lang::Rust => "json.gz",
    };
    let paths = SessionPaths { recording: dir.join(format!("recording.{ext}")), dir: dir.clone() };

    let (plan, how, jcmd) = match lang {
        Lang::Java => {
            let jdk = detect::probe_jdk(&args.root);
            if !jdk.availability.available {
                let _ = std::fs::remove_dir_all(&dir);
                return Err(unavailable_message(&jdk.availability));
            }
            let tuning = JfrTuning { sampling_ms: args.sampling_ms, event_settings: jdk.major >= 17 };
            let plan = match args.pid {
                Some(pid) => JfrAttach { pid, label: args.label.clone(), tuning }.plan(&paths)?,
                None => JfrLaunch { label: args.label.clone(), tuning }.plan(&paths)?,
            };
            (plan, jdk.availability.how_to_parse, jdk.jcmd)
        }
        Lang::Node => {
            if find_tool("node").is_none() {
                let _ = std::fs::remove_dir_all(&dir);
                return Err("Node.js was not found on PATH. Install Node (v12 or newer) to profile JavaScript.".into());
            }
            (NodeCpuProf { label: args.label.clone() }.plan(&paths)?, None, None)
        }
        Lang::Rust => {
            // An explicit path from the caller wins; otherwise: configured path > Arbor's own copy > PATH.
            let explicit = args.samply_path.as_deref().map(str::trim).filter(|p| !p.is_empty());
            let samply = match explicit {
                Some(p) => Some(PathBuf::from(p)).filter(|p| p.is_file()),
                None => match samply::resolve_configured() {
                    Ok(found) => found.map(|r| r.path),
                    Err(e) => {
                        let _ = std::fs::remove_dir_all(&dir);
                        return Err(e);
                    }
                },
            };
            let Some(samply) = samply else {
                let _ = std::fs::remove_dir_all(&dir);
                return Err("samply was not found. Download it from the profiler panel, pick an existing copy in the \
                            profiler settings, or install it (`cargo install samply`) and put it on PATH."
                    .into());
            };
            (SamplyRecord { samply, label: args.label.clone(), rate_hz: args.rate_hz }.plan(&paths)?, None, None)
        }
    };
    lock(sessions()).insert(
        id.clone(),
        Session {
            root: args.root,
            lang,
            dir,
            recording: paths.recording,
            label: args.label,
            started_ms: now_ms(),
            how,
            plan: plan.clone(),
            jcmd,
        },
    );
    Ok(PrepareOut { session_id: id, plan })
}

fn unavailable_message(a: &JfrAvailability) -> String {
    match &a.remedy {
        Some(r) => format!("{} {r}", a.reason),
        None => a.reason.clone(),
    }
}

// ── running a non-Java command under a plan ──────────────────────────────────

#[derive(Deserialize)]
pub struct RunCommandArgs {
    pub root: String,
    pub session_id: String,
    /// The program and its arguments (without the profiler wrapper — that comes from the plan).
    pub argv: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
}

/// Start `argv` under the session's plan (wrapper + environment), in the Run console.
#[arbor_rpc::handler]
fn bennu_profile_run_command(ctx: &BennuState, args: RunCommandArgs) -> Result<bennu_proto::prelude::RunHandle, String> {
    spawn_under_plan(ctx, &args.root, &args.session_id, &args.argv, args.cwd.as_deref(), args.env.as_ref())
}

/// The plan's wrapper, then `argv`, run with the plan's environment and the caller's own on top.
/// Shared by every way of starting a non-JVM recording, so they cannot disagree about it.
fn spawn_under_plan(
    ctx: &BennuState,
    root: &str,
    session_id: &str,
    argv: &[String],
    cwd: Option<&str>,
    env: Option<&HashMap<String, String>>,
) -> Result<bennu_proto::prelude::RunHandle, String> {
    let (plan, label) = {
        let s = lock(sessions());
        let s = s.get(session_id).ok_or("unknown profiling session")?;
        (s.plan.clone(), s.label.clone())
    };
    let mut full: Vec<String> = plan.wrapper.clone();
    full.extend(argv.iter().cloned());
    let (program, rest) = full.split_first().ok_or("nothing to run")?;
    let resolved =
        arbor_process_ext::prelude::locate_executable(program, None, &[], &[]).unwrap_or_else(|| program.clone());
    let mut cmd = std::process::Command::new(&resolved);
    cmd.args(rest);
    let cwd = cwd.filter(|c| !c.trim().is_empty()).unwrap_or(root).to_string();
    cmd.current_dir(&cwd)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    for (k, v) in &plan.env {
        cmd.env(k, v);
    }
    if let Some(env) = env {
        for (k, v) in env {
            cmd.env(k, v);
        }
    }
    #[cfg(windows)]
    {
        use arbor_process_ext::prelude::NoWindowExt;
        cmd.no_window();
    }
    let display = full.join(" ");
    crate::build::spawn_streamed(cmd, format!("profile: {label}"), display, cwd, root, ctx.event_sink(), |_| {})
        .map_err(|e| format!("cannot start `{resolved}`: {e}"))
}

#[derive(Deserialize)]
pub struct RunCargoArgs {
    pub root: String,
    pub session_id: String,
    /// The same invocation the Run button uses, so profiling is the configuration you already have.
    pub invocation: bennu_cargo::prelude::Invocation,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
}

/// Build a Cargo target and run its **binary** under the session's profiler.
///
/// Not `cargo run` under the wrapper: that would profile cargo, which then execs the program, and
/// what the recording should be about is the program. So the target is built and located the way
/// Debug does it, and the executable itself is what the wrapper is given. Blocking, like Debug: the
/// build is the wait, and the caller stays busy until there is a process or a reason there is not.
#[arbor_rpc::handler]
fn bennu_profile_run_cargo(ctx: &BennuState, args: RunCargoArgs) -> Result<bennu_proto::prelude::RunHandle, String> {
    let cwd = args.working_dir.clone().filter(|d| !d.trim().is_empty()).unwrap_or_else(|| args.root.clone());
    let wanted = crate::cargo_debug::Wanted::from_selector(&args.invocation.target);
    let sink = ctx.event_sink();
    let exe = crate::cargo_debug::build_and_locate(&args.root, &cwd, &args.invocation, &wanted, |line| {
        sink.emit("arbor://bennu/build-output", json!({ "text": line }));
    })?;
    let mut argv = vec![exe];
    argv.extend(args.invocation.args.iter().cloned());
    spawn_under_plan(ctx, &args.root, &args.session_id, &argv, Some(cwd.as_str()), args.env.as_ref())
}

// ── attach ────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SessionArgs {
    pub session_id: String,
}

fn run_jcmd_steps(session_id: &str, stop: bool) -> Result<(), String> {
    let (jcmd, steps) = {
        let s = lock(sessions());
        let s = s.get(session_id).ok_or("unknown profiling session")?;
        let steps = if stop { s.plan.attach_stop.clone() } else { s.plan.attach_start.clone() };
        (s.jcmd.clone(), steps)
    };
    let jcmd = jcmd.ok_or("jcmd was not found in the project's JDK (a JRE has none).")?;
    for step in steps {
        detect::run_jcmd(&jcmd, &step_args(&step))?;
    }
    Ok(())
}

/// `[pid, "JFR.start", opts…]` is already in `jcmd <pid> <command> <opts…>` order.
fn step_args(step: &[String]) -> Vec<String> {
    step.to_vec()
}

/// Start the recording inside the already-running JVM.
#[arbor_rpc::handler]
fn bennu_profile_attach_start(_ctx: &BennuState, args: SessionArgs) -> Result<(), String> {
    run_jcmd_steps(&args.session_id, false)
}

/// Stop the recording (writing the file) and parse it.
#[arbor_rpc::handler]
fn bennu_profile_attach_stop(ctx: &BennuState, args: SessionArgs) -> Result<(), String> {
    run_jcmd_steps(&args.session_id, true)?;
    finish_session(ctx, &args.session_id)
}

#[derive(Deserialize)]
pub struct DumpArgs {
    pub session_id: String,
    /// The main class the profiled run was started with — how its JVM is found among the local ones.
    pub main_class: String,
}

/// "Stop recording" for a JVM we launched: dump the recording from the live process (the file
/// would otherwise only be written when the JVM exits by itself), then parse it. The caller stops
/// the run afterwards.
#[arbor_rpc::handler]
fn bennu_profile_dump_running(ctx: &BennuState, args: DumpArgs) -> Result<(), String> {
    let (jcmd, recording) = {
        let s = lock(sessions());
        let s = s.get(&args.session_id).ok_or("unknown profiling session")?;
        (s.jcmd.clone(), s.recording.clone())
    };
    let jcmd = jcmd.ok_or("jcmd was not found in the project's JDK (a JRE has none).")?;
    let jvms = detect::parse_jcmd_list(&detect::run_jcmd(&jcmd, &["-l".into()])?);
    let target = jvms
        .iter()
        .find(|j| j.name.contains(&args.main_class))
        .ok_or("The profiled JVM is no longer running; its recording is read from the file it wrote on exit.")?;
    detect::run_jcmd(
        &jcmd,
        &[
            target.pid.to_string(),
            "JFR.dump".into(),
            "name=arbor-profile".into(),
            format!("filename={}", jfr_value(&recording.display().to_string())),
        ],
    )?;
    finish_session(ctx, &args.session_id)
}

// ── finish / import ───────────────────────────────────────────────────────────

/// The run ended: parse what it left behind (asynchronously; completion arrives as an event).
#[arbor_rpc::handler]
fn bennu_profile_finish(ctx: &BennuState, args: SessionArgs) -> Result<(), String> {
    finish_session(ctx, &args.session_id)
}

fn finish_session(ctx: &BennuState, session_id: &str) -> Result<(), String> {
    let s = lock(sessions()).remove(session_id).ok_or("unknown profiling session")?;
    let sink = ctx.event_sink();
    let id = session_id.to_string();
    std::thread::Builder::new()
        .name(format!("bennu-profile-{id}"))
        .spawn(move || {
            let outcome = parse_session(&s, &id, &*sink);
            let _ = std::fs::remove_dir_all(&s.dir);
            report(&*sink, &id, &s.root, outcome);
        })
        .map(|_| ())
        .map_err(|e| format!("cannot start the parser thread: {e}"))
}

fn report(sink: &dyn arbor_ipc::prelude::EventSink, session_id: &str, root: &str, outcome: Result<Profile, String>) {
    match outcome.and_then(|p| store::save(root, &p).map(|_| p)) {
        Ok(p) => {
            lock(outcomes()).insert(session_id.to_string(), Ok(()));
            sink.emit(EVT_DONE, json!({ "session_id": session_id, "ok": true, "meta": store::meta_of(&p) }));
        }
        Err(e) => {
            lock(outcomes()).insert(session_id.to_string(), Err(e.clone()));
            sink.emit(EVT_DONE, json!({ "session_id": session_id, "ok": false, "error": e }));
        }
    }
}

/// How each recording's parse ended, for a caller that waits for it instead of listening for the event (the AI
/// assistant). Small: a session id and a line.
fn outcomes() -> &'static Mutex<HashMap<String, Result<(), String>>> {
    static O: OnceLock<Mutex<HashMap<String, Result<(), String>>>> = OnceLock::new();
    O.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Whether the recording `session_id` has been parsed and saved: `None` while it is still being read.
pub(crate) fn outcome_of(session_id: &str) -> Option<Result<(), String>> {
    lock(outcomes()).get(session_id).cloned()
}

// ── for the assistant's own tools ─────────────────────────────────────────────

/// The JVMs on this machine a recorder can attach to.
pub(crate) fn running_jvms(root: &str) -> Result<Vec<(u32, String)>, String> {
    Ok(detect::list_jvms(root)?.into_iter().map(|j| (j.pid, j.name)).collect())
}

/// Start recording the JVM `pid`: a session and the recorder started inside it. Returns the session id.
pub(crate) fn start_recording(ctx: &BennuState, root: &str, pid: u32, label: &str) -> Result<String, String> {
    let prepared = bennu_profile_prepare(ctx, PrepareArgs { root: root.to_string(), lang: "java".into(), label: label.to_string(), pid: Some(pid), samply_path: None, rate_hz: None, sampling_ms: None })?;
    if let Err(e) = run_jcmd_steps(&prepared.session_id, false) {
        // Nothing is recording: the session and its scratch folder go.
        if let Some(s) = lock(sessions()).remove(&prepared.session_id) {
            let _ = std::fs::remove_dir_all(&s.dir);
        }
        return Err(e);
    }
    Ok(prepared.session_id)
}

/// Stop the recording and parse it in the background; `outcome_of` says when it is saved.
pub(crate) fn stop_recording(ctx: &BennuState, session_id: &str) -> Result<(), String> {
    bennu_profile_attach_stop(ctx, SessionArgs { session_id: session_id.to_string() })
}

/// The summary of a saved recording, as `bennu_profile_summary` gives it.
pub(crate) fn recording_summary(root: &str, id: &str, limit: usize) -> Result<serde_json::Value, String> {
    summary_of(root, Some(id), limit.clamp(1, 50))
}

fn parse_session(s: &Session, id: &str, sink: &dyn arbor_ipc::prelude::EventSink) -> Result<Profile, String> {
    let progress = |events: u64| sink.emit(EVT_PROGRESS, json!({ "session_id": id, "events": events }));
    sink.emit(EVT_PROGRESS, json!({ "session_id": id, "events": 0 }));
    let mut p = match s.lang {
        Lang::Java => {
            if !s.recording.is_file() {
                return Err(format!(
                    "The JVM left no recording behind. It probably exited abnormally (killed, System.halt) before the \
                     recorder could write {}.",
                    s.recording.display()
                ));
            }
            let how = s.how.as_ref().ok_or("no way to read the recording was resolved")?;
            parse::parse_jfr(how, &s.recording, &s.dir.join("reader"), id, &s.label, &progress)?
        }
        Lang::Node => {
            let files = files_with_ext(&s.dir, "cpuprofile");
            if files.is_empty() {
                return Err("Node wrote no .cpuprofile. The process must EXIT normally (not be killed) for V8 to write it; \
                            stop a dev server with Ctrl+C / its own shutdown rather than the Stop button, or import the file by hand."
                    .into());
            }
            parse::parse_cpuprofiles(&files, id, &s.label)?
        }
        Lang::Rust => {
            if !s.recording.is_file() {
                return Err("samply wrote no profile. Check the Run console output of the profiled command.".into());
            }
            parse::parse_samply(&s.recording, id, &s.label)?
        }
    };
    p.duration_ms = p.duration_ms.or(Some(now_ms().saturating_sub(s.started_ms) as f64));
    Ok(p)
}

#[derive(Deserialize)]
pub struct ImportArgs {
    pub root: String,
    pub path: String,
    /// `java` (.jfr) | `node` (.cpuprofile) | `rust` (samply JSON). Guessed from the extension if absent.
    #[serde(default)]
    pub lang: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
}

/// Import a recording made elsewhere (`.jfr`, `.cpuprofile`, a samply profile).
#[arbor_rpc::handler]
fn bennu_profile_import(ctx: &BennuState, args: ImportArgs) -> Result<String, String> {
    let path = PathBuf::from(&args.path);
    if !path.is_file() {
        return Err(format!("{} is not a file", args.path));
    }
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let lang = match args.lang.as_deref() {
        Some(l) => parse_lang(l)?,
        None => match ext.as_str() {
            "jfr" => Lang::Java,
            "cpuprofile" => Lang::Node,
            "json" | "gz" => Lang::Rust,
            _ => return Err("cannot tell what this file is; pick the language".into()),
        },
    };
    let id = session_id(lang);
    let label = args.label.unwrap_or_else(|| {
        path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "imported".into())
    });
    let dir = std::env::temp_dir().join("arbor-bennu-profile").join(&id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let (how, jcmd) = if lang == Lang::Java {
        let jdk = detect::probe_jdk(&args.root);
        if !jdk.availability.available {
            let _ = std::fs::remove_dir_all(&dir);
            return Err(unavailable_message(&jdk.availability));
        }
        (jdk.availability.how_to_parse, jdk.jcmd)
    } else {
        (None, None)
    };
    // Node imports go through the same code path as a session: a directory holding the file.
    let (scratch_dir, recording) = if lang == Lang::Node {
        let copy = dir.join(path.file_name().unwrap_or_default());
        std::fs::copy(&path, &copy).map_err(|e| e.to_string())?;
        (dir.clone(), copy)
    } else {
        (dir.clone(), path.clone())
    };
    let session = Session {
        root: args.root,
        lang,
        dir: scratch_dir,
        recording,
        label,
        started_ms: now_ms(),
        how,
        plan: StartPlan::default(),
        jcmd,
    };
    lock(sessions()).insert(id.clone(), session);
    finish_session(ctx, &id)?;
    Ok(id)
}

// ── reading ───────────────────────────────────────────────────────────────────

#[arbor_rpc::handler]
fn bennu_profile_list(_ctx: &BennuState, args: RootArgs) -> Result<Vec<ProfileMeta>, String> {
    Ok(store::list(&args.root))
}

#[derive(Deserialize)]
pub struct IdArgs {
    pub root: String,
    pub id: String,
}

#[arbor_rpc::handler]
fn bennu_profile_delete(_ctx: &BennuState, args: IdArgs) -> Result<(), String> {
    store::delete(&args.root, &args.id)
}

/// The tables of one recording (no call tree — that is `bennu_profile_flame`).
#[derive(Serialize)]
pub struct ProfileView {
    pub meta: ProfileMeta,
    pub methods: Vec<MethodStat>,
    pub alloc_sites: Vec<AllocStat>,
    pub alloc_types: Vec<AllocStat>,
    pub locks: Vec<LockStat>,
    pub gc: Vec<GcStat>,
    pub threads: Vec<ThreadStat>,
    /// Where threads waited off the CPU — sockets, files, parks — longest first.
    pub waits: Vec<WaitStat>,
    /// Whether the recording kept allocation stacks, so "who allocates" can be answered. A recording made before
    /// they were kept has only the allocating site.
    pub has_alloc_stacks: bool,
    pub has_waits: bool,
    pub notes: Vec<String>,
}

const VIEW_METHODS: usize = 400;
const VIEW_ROWS: usize = 200;

#[arbor_rpc::handler]
fn bennu_profile_get(_ctx: &BennuState, args: IdArgs) -> Result<ProfileView, String> {
    let p = store::load(&args.root, &args.id)?;
    let take = |v: &[AllocStat]| v.iter().take(VIEW_ROWS).cloned().collect::<Vec<_>>();
    Ok(ProfileView {
        meta: store::meta_of(&p),
        methods: p.methods.iter().take(VIEW_METHODS).cloned().collect(),
        alloc_sites: take(&p.alloc_sites),
        alloc_types: take(&p.alloc_types),
        locks: p.locks.iter().take(VIEW_ROWS).cloned().collect(),
        gc: p.gc.clone(),
        threads: p.threads.iter().take(VIEW_ROWS).cloned().collect(),
        waits: p.waits.iter().take(VIEW_ROWS).cloned().collect(),
        has_alloc_stacks: p.alloc_tree.total > 0,
        has_waits: p.wait_tree.total > 0,
        notes: p.notes.clone(),
    })
}

#[derive(Deserialize)]
pub struct FlameArgs {
    pub root: String,
    pub id: String,
    /// Restrict to one thread; absent = every thread merged.
    #[serde(default)]
    pub thread: Option<String>,
    /// What the boxes weigh: `cpu` (the default), `wait` (milliseconds blocked off the CPU) or `alloc` (bytes).
    #[serde(default)]
    pub kind: Option<String>,
}

#[arbor_rpc::handler]
fn bennu_profile_flame(_ctx: &BennuState, args: FlameArgs) -> Result<FlameNode, String> {
    let p = store::load(&args.root, &args.id)?;
    let tree = match args.kind.as_deref() {
        Some("wait") => &p.wait_tree,
        Some("alloc") => &p.alloc_tree,
        _ => &p.tree,
    };
    Ok(tree.flame(args.thread.as_deref(), FlameOptions::default()))
}

#[derive(Deserialize)]
pub struct CompareArgs {
    pub root: String,
    pub old: String,
    pub new: String,
}

#[arbor_rpc::handler]
fn bennu_profile_compare(_ctx: &BennuState, args: CompareArgs) -> Result<Vec<MethodDelta>, String> {
    let old = store::load(&args.root, &args.old)?;
    let new = store::load(&args.root, &args.new)?;
    Ok(compare(&old, &new, 300))
}

// ── source location ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct LocateArgs {
    pub root: String,
    /// JFR binary class name (`a.b.Outer$Inner`).
    pub class: String,
    pub method: String,
    #[serde(default)]
    pub arity: Option<u32>,
}

#[derive(Serialize)]
pub struct Located {
    pub file: String,
    /// Signature line of the method; `None` when the class was found but the method was not
    /// (a lambda, a synthetic accessor) — the caller then opens the file at the top.
    pub line: Option<u32>,
}

/// Where a profiled Java method is declared, from the project's class index. `None` for a class
/// outside the project (JDK, dependency) — the caller falls back to the library source view.
#[arbor_rpc::handler]
fn bennu_profile_locate(_ctx: &BennuState, args: LocateArgs) -> Result<Option<Located>, String> {
    let dotted = dotted_class(&args.class);
    // A nested class lives in its outermost class's file.
    let outer = dotted_class(args.class.split('$').next().unwrap_or(&args.class));
    let entries = crate::index_service::IndexService::global().class_index(&args.root).unwrap_or_default();
    let found = entries
        .iter()
        .find(|e| e.fqcn == dotted)
        .or_else(|| entries.iter().find(|e| e.fqcn == outer));
    let Some(entry) = found else { return Ok(None) };
    let file = entry.file.clone();
    let line = std::fs::read(&file).ok().and_then(|b| {
        let text = String::from_utf8_lossy(&b).into_owned();
        let name = match unlambda_name(&args.method) {
            Some(n) => n,
            None => args.method.clone(),
        };
        let decls = decls::declared_methods(&text);
        let same_class: Vec<_> = decls.iter().filter(|d| d.class == dotted && (d.name == name || (name == "<init>" && d.is_constructor))).collect();
        same_class
            .iter()
            .find(|d| args.arity.map_or(true, |a| d.arity == a))
            .or_else(|| same_class.first())
            .map(|d| d.line)
    });
    Ok(Some(Located { file: file.to_string(), line }))
}

fn unlambda_name(method: &str) -> Option<String> {
    method.strip_prefix("lambda$")?.rsplit_once('$').map(|(n, _)| n.to_string())
}

// ── editor metrics ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct FileMetricsArgs {
    pub root: String,
    pub file: String,
    /// The editor's current text (Java: declarations are read from it, so edits move the hints).
    #[serde(default)]
    pub source: Option<String>,
    /// A specific recording; absent = the newest one of the file's language.
    #[serde(default)]
    pub id: Option<String>,
}

#[derive(Serialize)]
pub struct FileMetricsOut {
    pub recording_id: Option<String>,
    pub label: String,
    pub created_ms: u64,
    /// The file was modified after the recording was made: the numbers describe older code.
    pub stale: bool,
    pub metrics: FileMetrics,
}

fn lang_of_file(file: &str) -> Option<Lang> {
    let ext = std::path::Path::new(file).extension()?.to_string_lossy().to_ascii_lowercase();
    match ext.as_str() {
        "java" => Some(Lang::Java),
        "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "vue" | "svelte" => Some(Lang::Node),
        "rs" => Some(Lang::Rust),
        _ => None,
    }
}

fn mtime_ms(file: &str) -> Option<u64> {
    std::fs::metadata(file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
}

/// Whether a class (a binary name) is the project's own: one of the classes its index declares when it has them,
/// otherwise anything that is not the JDK — less exact, and better than crediting nothing while the index builds.
fn project_predicate(root: &str) -> Box<dyn Fn(&str) -> bool> {
    match crate::index_service::IndexService::global().class_index(root) {
        Some(entries) => {
            let own: std::collections::HashSet<String> = entries.into_iter().map(|c| c.fqcn).collect();
            Box::new(move |class: &str| own.contains(class.split('$').next().unwrap_or(class)))
        }
        None => Box::new(|class: &str| !["java.", "javax.", "jdk.", "sun.", "com.sun."].iter().any(|p| class.starts_with(p))),
    }
}

/// Who allocates: the recording's allocated bytes credited to the project's own methods (see `bennu_profile::alloc_code`).
#[derive(Serialize)]
pub struct AllocByCode {
    /// False for a recording made before allocation stacks were kept: there is nothing to walk up.
    pub available: bool,
    pub total_bytes: u64,
    pub rows: Vec<CodeAlloc>,
}

#[arbor_rpc::handler]
fn bennu_profile_alloc_by_code(_ctx: &BennuState, args: IdArgs) -> Result<AllocByCode, String> {
    let p = store::load(&args.root, &args.id)?;
    if p.alloc_tree.total == 0 {
        return Ok(AllocByCode { available: false, total_bytes: 0, rows: Vec::new() });
    }
    let is_project = project_predicate(&args.root);
    Ok(AllocByCode { available: true, total_bytes: p.alloc_tree.total, rows: allocations_by_code(&p.alloc_tree, &*is_project, 300) })
}

/// Numbers for the methods and lines of one file, for the editor's inline hints and gutter.
#[arbor_rpc::handler]
fn bennu_profile_file_metrics(_ctx: &BennuState, args: FileMetricsArgs) -> Result<FileMetricsOut, String> {
    let none = || FileMetricsOut {
        recording_id: None,
        label: String::new(),
        created_ms: 0,
        stale: false,
        metrics: FileMetrics::default(),
    };
    let Some(lang) = lang_of_file(&args.file) else { return Ok(none()) };
    let meta = match &args.id {
        Some(id) => store::list(&args.root).into_iter().find(|m| &m.id == id),
        None => store::latest(&args.root, Some(lang)),
    };
    let Some(meta) = meta else { return Ok(none()) };
    if meta.lang != lang {
        return Ok(none());
    }
    let p = store::load(&args.root, &meta.id)?;
    let metrics = match lang {
        Lang::Java => {
            let source = match args.source {
                Some(s) => s,
                None => std::fs::read_to_string(&args.file).map_err(|e| e.to_string())?,
            };
            java_file_metrics(&p, &decls::declared_methods(&source), 0.5, &*project_predicate(&args.root))
        }
        _ => path_file_metrics(&p, &args.file, 0.5),
    };
    Ok(FileMetricsOut {
        recording_id: Some(meta.id),
        label: meta.label,
        created_ms: meta.created_ms,
        stale: mtime_ms(&args.file).is_some_and(|m| m > meta.created_ms),
        metrics,
    })
}

// ── for the assistant ─────────────────────────────────────────────────────────

/// Args for [`bennu_profile_summary`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ProfileSummaryArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// How many rows each list keeps (default 15, at most 50).
    #[serde(default)]
    pub limit: Option<usize>,
    /// The recording to summarise, by id (see `bennu_profile_recordings`). Default: the newest.
    #[serde(default)]
    pub id: Option<String>,
}

/// A recording of the project's profiler, reduced to what explains where time and memory
/// went: the hottest methods (self and total share of the samples, self milliseconds, the hottest
/// source lines of each), the biggest allocation sites and allocated types, lock contention and
/// garbage-collection pauses. The newest one unless `id` names another. Use it to find the method worth
/// optimising and to propose a change; it measures a run the user already made and starts nothing.
/// Errors when no recording exists.
#[arbor_rpc::handler(mcp(
    title = "Profiler recording summary",
    safety = read,
))]
fn bennu_profile_summary(_ctx: &BennuState, args: ProfileSummaryArgs) -> Result<serde_json::Value, String> {
    summary_of(&args.root, args.id.as_deref(), args.limit.unwrap_or(15).clamp(1, 50))
}

/// Args for [`bennu_profile_recordings`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct RecordingsArgs {
    /// Absolute path to the project root.
    pub root: String,
}

/// The profiler recordings the project has, newest first: id, language, label, when it was made, how long it ran
/// and how many samples it holds. The ids are what `bennu_profile_summary` and `bennu_profile_diff` take. Read-only.
#[arbor_rpc::handler(mcp(
    title = "Profiler recordings",
    safety = read,
))]
fn bennu_profile_recordings(_ctx: &BennuState, args: RecordingsArgs) -> Result<serde_json::Value, String> {
    let list = store::list(&args.root);
    if list.is_empty() {
        return Err("There is no profiler recording for this project yet. Ask the user to record one (Profiler tool window).".into());
    }
    serde_json::to_value(list).map_err(|e| e.to_string())
}

/// Args for [`bennu_profile_diff`].
#[derive(Deserialize, schemars::JsonSchema)]
pub struct ProfileDiffArgs {
    /// Absolute path to the project root.
    pub root: String,
    /// The earlier recording's id (see `bennu_profile_recordings`).
    pub old: String,
    /// The later recording's id.
    pub new: String,
    /// How many methods to return (default 15, at most 50).
    #[serde(default)]
    pub limit: Option<usize>,
}

/// What changed between two recordings, method by method: the share of the run each took before and after, the
/// change in percentage points and the self milliseconds, the biggest changes first. Use it to check that a change
/// made the code faster (or to find what made it slower) after the user recorded again. Read-only.
#[arbor_rpc::handler(mcp(
    title = "Profiler recordings compared",
    safety = read,
))]
fn bennu_profile_diff(_ctx: &BennuState, args: ProfileDiffArgs) -> Result<serde_json::Value, String> {
    let (old, new) = (store::load(&args.root, &args.old)?, store::load(&args.root, &args.new)?);
    let changes = compare(&old, &new, args.limit.unwrap_or(15).clamp(1, 50));
    Ok(json!({
        "old": store::meta_of(&old),
        "new": store::meta_of(&new),
        "changes": changes,
    }))
}

/// A compact JSON summary of a recording (the newest when `id` is `None`): what the model needs to propose a fix.
/// `limit` caps every list.
fn summary_of(root: &str, id: Option<&str>, limit: usize) -> Result<serde_json::Value, String> {
    let meta = match id {
        Some(id) => store::list(root).into_iter().find(|m| m.id == id).ok_or_else(|| format!("There is no recording `{id}`: bennu_profile_recordings lists them."))?,
        None => store::latest(root, None)
            .ok_or("There is no profiler recording for this project yet. Ask the user to record one (Profiler tool window).")?,
    };
    let p = store::load(root, &meta.id)?;
    let total = p.tree.total.max(1) as f64;
    let methods: Vec<_> = p
        .methods
        .iter()
        .take(limit)
        .map(|m| {
            json!({
                "method": format!("{}.{}", m.class, m.method),
                "file": m.file,
                "self_pct": (m.self_w as f64 * 1000.0 / total).round() / 10.0,
                "total_pct": (m.total_w as f64 * 1000.0 / total).round() / 10.0,
                "self_ms": p.ms(m.self_w).round(),
                "hot_lines": m.hot_lines.iter().take(3).collect::<Vec<_>>(),
            })
        })
        .collect();
    Ok(json!({
        "recording": { "id": meta.id, "language": meta.lang, "label": meta.label, "created_ms": meta.created_ms,
                       "duration_ms": meta.duration_ms, "samples": meta.samples, "ms_per_sample": meta.ms_per_unit },
        "hot_methods": methods,
        "allocation_sites": p.alloc_sites.iter().take(limit).collect::<Vec<_>>(),
        "allocated_types": p.alloc_types.iter().take(limit).collect::<Vec<_>>(),
        "lock_contention": p.locks.iter().take(limit).collect::<Vec<_>>(),
        "gc": p.gc,
        "notes": p.notes,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_ids_are_path_safe_and_language_prefixed() {
        let id = session_id(Lang::Node);
        assert!(id.starts_with("node-"));
        assert!(id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'));
    }

    #[test]
    fn files_map_to_languages() {
        assert_eq!(lang_of_file("/a/B.java"), Some(Lang::Java));
        assert_eq!(lang_of_file("C:\\a\\x.TS"), Some(Lang::Node));
        assert_eq!(lang_of_file("/a/lib.rs"), Some(Lang::Rust));
        assert_eq!(lang_of_file("/a/README.md"), None);
    }
}
