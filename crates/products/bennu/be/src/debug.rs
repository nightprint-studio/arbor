//! The debugger: a JDWP session over a launched JVM.
//!
//! [`bennu_jdwp`] is the protocol and knows nothing about projects. This module is what makes
//! it a *debugger*: it launches the program with the agent, decides what a breakpoint is,
//! turns the VM's answers back into files and lines this editor can open, and owns the one
//! session at a time.
//!
//! ## Listening first, rather than being announced
//!
//! The agent has two modes. `server=y` makes the **VM** listen and print the port it chose on
//! its own stdout, which means the launcher has to scrape a line of the program's output and
//! race the program's own writes to it. `server=n` is the other way round: the debugger listens
//! first and the VM connects to it. That is what this does — the port is known before the
//! process exists, there is no line to parse, and the console never shows a line the user did
//! not write. The cost is that the socket must be open *before* the spawn, or the VM aborts
//! with "transport failed to initialize"; hence [`prepare`] returning the listener that
//! [`crate::build`] then hands back here.
//!
//! ## Suspending, or not
//!
//! `suspend=n` is the default and the launch is a normal one: the program runs, and a
//! breakpoint it has already passed is simply missed. `suspend=y` holds the VM before `main`
//! until the debugger has installed everything, which is the only way to stop in start-up code
//! — and it is opt-in per run configuration, because every launch then begins frozen.
//!
//! ## One suspend policy, everywhere
//!
//! Breakpoints, steps and exception stops all suspend **every** thread, and the only way to
//! continue is `resume_vm`. Mixing policies is the subtle mistake: an `EventThread` suspend is
//! undone by `resume_thread` and an `All` suspend is not, so a session that used both would
//! have to remember which — and would get it wrong exactly once, leaving the program half
//! frozen with no way back. One rule costs nothing and cannot be got wrong.
//!
//! ## Never call the VM from the event thread
//!
//! Replies are delivered by the crate's reader thread; the event loop here runs on its own
//! thread and is free to make calls, but it must never be the thread reading the socket. That
//! is the same reverse-channel deadlock the Arbor shell hit going out-of-process, and it is
//! why the session's locks are held to *store* results and never across a round trip.

use std::collections::HashMap;
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;
use bennu_core::prelude::BennuState;
// Named rather than glob-imported: the crate's prelude exports its own one-parameter
// `Result<T>`, and a glob would quietly shadow `std`'s in a module whose every handler returns
// `Result<T, String>`.
use bennu_jdwp::prelude::{
    class_name, class_signature, classes_by_signature, clear_event, dispose, fields, frames,
    kind, line_table, location_of_line, methods, object_type, request_class_prepare,
    request_exception, request_step, resume_thread, resume_vm, set_breakpoint, thread_name,
    type_signature, variable_table, version, Client, Composite, Event, Field, Frame, Id,
    LineEntry, Local, Location, Method, StepDepth, SuspendPolicy,
};
use bennu_proto::prelude::{
    Breakpoint, BreakpointStatus, DebugConfig, DebugDump, DebugPause, DebugStatus, DebugValue,
    ExceptionBreakpoint, StackFrame,
};
use serde::Deserialize;
use serde_json::json;

use crate::index_service::IndexService;

// ── event topics (the wire contract for the FE) ────────────────────────────────

/// The session as a whole moved: attaching, running, paused, gone.
const EVT_DEBUG_STATUS: &str = "arbor://bennu/debug-status";
/// The program stopped somewhere, with the stack that got it there.
const EVT_DEBUG_PAUSED: &str = "arbor://bennu/debug-paused";
/// A breakpoint's verification changed — a class loaded, or a line turned out to have no code.
const EVT_DEBUG_BREAKPOINTS: &str = "arbor://bennu/debug-breakpoints";

/// Packages a *step into* passes straight through.
///
/// Not a nicety: an unfiltered step into `list.add(order)` lands in `ArrayList.add`, then
/// `ensureCapacity`, then `Arrays.copyOf` — a stepping session spent somewhere the user has no
/// source for and did not ask about. With the filter the step stops at the first frame that is
/// not one of these, which is nearly always the one that was meant.
const DEFAULT_STEP_EXCLUDES: &[&str] = &[
    // The runtime.
    "java.*",
    "javax.*",
    "jakarta.*",
    "sun.*",
    "com.sun.*",
    "jdk.*",
    // Other languages' runtimes, when they share the VM.
    "kotlin.*",
    "scala.*",
    "groovy.*",
    "org.codehaus.groovy.*",
    // The machinery a call to an injected bean actually goes through. Without these, stepping
    // into `service.place(order)` walks the proxy, then `ReflectiveMethodInvocation.proceed`,
    // then every interceptor in the chain — a dozen stops in code the reader has no source for
    // and did not ask about, before the method they meant.
    "org.springframework.*",
    "org.aopalliance.*",
    "net.sf.cglib.*",
    "javassist.*",
    "org.hibernate.*",
    "reactor.*",
    "org.reactivestreams.*",
    "org.slf4j.*",
    "ch.qos.logback.*",
    "org.apache.commons.logging.*",
];

/// The patterns this session steps through: the configured list, or [`DEFAULT_STEP_EXCLUDES`]
/// when it is empty.
///
/// **Invalid patterns are dropped rather than sent.** JDWP accepts a `*` at one end of a
/// pattern and nowhere else, and one bad entry makes the VM refuse the whole `EventRequest.Set`
/// — which does not surface as an error anywhere the user is looking, it surfaces as stepping
/// having quietly stopped working.
fn step_excludes() -> Vec<String> {
    let configured: Vec<String> = bennu_core::config::load()
        .step_excludes
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty() && is_valid_pattern(p))
        .collect();
    if configured.is_empty() {
        DEFAULT_STEP_EXCLUDES.iter().map(|s| (*s).to_string()).collect()
    } else {
        configured
    }
}

/// Whether `pattern` is one the VM will accept: a star at one end, or none at all.
fn is_valid_pattern(pattern: &str) -> bool {
    let inner = pattern.trim_start_matches('*').trim_end_matches('*');
    !inner.is_empty() && !inner.contains('*')
}

/// How long the VM gets to connect back before the launch is declared un-debuggable. The agent
/// connects during VM initialization, so this is seconds of slack over a cold start, not a
/// budget anything normally spends.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// The deepest stack the frame list goes. A runaway recursion produces tens of thousands of
/// frames and nobody reads past the first screen; the cut is reported rather than silent.
const MAX_FRAMES: usize = 300;

/// How many elements of an array are listed before it is cut short.
pub(crate) const MAX_ELEMENTS: i32 = 100;

/// How many times a step may be silently continued before stopping wherever it is.
///
/// A proxy chain is a handful of frames, so this is generous; it exists so that a shape nobody
/// anticipated cannot turn one key press into a program that steps forever. Reaching it stops
/// somewhere unhelpful, which is at least somewhere.
const MAX_STEP_SKIPS: u32 = 40;

// ── launching ──────────────────────────────────────────────────────────────────

/// A bound port waiting for the VM to call back. Returned by [`prepare`] and consumed by
/// [`start`], with the launch in between — the socket has to exist before the process does.
pub(crate) struct DebugLaunch {
    listener: TcpListener,
    /// The port the JVM must be told to connect to.
    pub(crate) port: u16,
}

/// Bind a loopback port for the VM to connect back on. `None` when the port cannot be bound,
/// which degrades the launch to an ordinary run rather than failing it — a program that runs
/// without the debugger attached is better than one that does not start.
pub(crate) fn prepare() -> Option<DebugLaunch> {
    let listener = TcpListener::bind("127.0.0.1:0").ok()?;
    let port = listener.local_addr().ok()?.port();
    listener.set_nonblocking(true).ok()?;
    Some(DebugLaunch { listener, port })
}

/// The `-agentlib` argument for a launch that connects back to `port`.
pub(crate) fn agent_arg(port: u16, suspend: bool) -> String {
    let suspend = if suspend { "y" } else { "n" };
    format!("-agentlib:jdwp=transport=dt_socket,server=n,suspend={suspend},address=127.0.0.1:{port}")
}

/// Take over the launch: wait for the VM to connect, install what the project has configured,
/// and run the session until the program ends. Returns immediately; everything happens on a
/// thread of its own.
pub(crate) fn start(
    session_id: String,
    root: String,
    launch: DebugLaunch,
    sink: Arc<dyn EventSink>,
) {
    emit_status(&sink, &session_id, "starting", "", "");
    let _ = std::thread::Builder::new()
        .name(format!("bennu-debug-{session_id}"))
        .spawn(move || match accept(launch) {
            Ok(stream) => match Client::from_stream(stream) {
                Ok((client, events)) => serve(session_id, root, client, events, sink),
                Err(e) => emit_status(&sink, &session_id, "terminated", "", &e.to_string()),
            },
            Err(message) => emit_status(&sink, &session_id, "terminated", "", &message),
        });
}

/// Poll for the VM's connection until [`CONNECT_TIMEOUT`].
///
/// Polled rather than blocking on `accept()`: a program that dies before its agent connects —
/// a bad classpath, a `-Xmx` the machine cannot honour — would otherwise park this thread for
/// the life of the process, waiting for a caller that no longer exists.
fn accept(launch: DebugLaunch) -> Result<TcpStream, String> {
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    loop {
        match launch.listener.accept() {
            Ok((stream, _)) => {
                let _ = stream.set_nonblocking(false);
                return Ok(stream);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err("the program never connected to the debugger".to_string());
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(e) => return Err(format!("waiting for the program to connect: {e}")),
        }
    }
}

/// The session, start to finish: register it, install the project's breakpoints, then drain
/// events until the VM is gone.
fn serve(
    session_id: String,
    root: String,
    client: Client,
    events: Receiver<Composite>,
    sink: Arc<dyn EventSink>,
) {
    let vm = version(&client).unwrap_or_default();
    let config: DebugConfig = crate::repo_config::load(&root, "debug");
    let session = Arc::new(Session::new(session_id.clone(), root, client, sink.clone(), config));
    registry().lock().unwrap_or_else(|p| p.into_inner()).insert(session_id.clone(), session.clone());
    // …and in the shared dispatch table the handlers read, which is what makes them protocol-blind.
    // Two maps, one truth each: this module's answers "which JDWP sessions exist" (its own event loop
    // and `debug_value` need the concrete type), the shared one answers "who has this id".
    crate::debug_backend::insert(
        &session_id,
        session.clone() as Arc<dyn crate::debug_backend::DebugBackend>,
    );

    session.install_all();
    session.install_exceptions();
    emit_status(&sink, &session_id, "running", &vm, "");

    for composite in events {
        let policy = composite.policy;
        for event in composite.events {
            session.on_event(event, policy);
        }
    }

    // The channel closed, which means the reader thread ended, which means the socket did.
    registry().lock().unwrap_or_else(|p| p.into_inner()).remove(&session_id);
    crate::debug_backend::remove(&session_id);
    emit_status(&sink, &session_id, "terminated", &vm, "");
}

/// The JDWP session's side of the seam every `bennu_debug_*` handler is written against.
///
/// A thin adapter and nothing more: each method is one of the inherent ones this module already had,
/// which is the sign the seam was real before it was named. See [`crate::debug_backend`].
impl crate::debug_backend::DebugBackend for Session {
    fn kind(&self) -> &'static str {
        "jdwp"
    }

    fn describe(&self) -> String {
        version(&self.client).unwrap_or_default()
    }

    fn root(&self) -> String {
        self.root.clone()
    }

    fn resume(&self) -> Result<(), String> {
        Session::resume(self)
    }

    fn step(&self, step: crate::debug_backend::Step) -> Result<(), String> {
        let depth = match step {
            crate::debug_backend::Step::Into => StepDepth::Into,
            crate::debug_backend::Step::Out => StepDepth::Out,
            crate::debug_backend::Step::Over => StepDepth::Over,
        };
        Session::step(self, depth)
    }

    fn set_muted(&self, muted: bool) -> Result<(), String> {
        Session::set_muted(self, muted);
        Ok(())
    }

    fn detach(&self) -> Result<(), String> {
        // Best-effort: a VM that has stopped answering still has to let go of this end.
        if dispose(&self.client).is_err() {
            self.client.close();
        }
        Ok(())
    }

    fn variables(&self, frame: usize) -> Result<Vec<DebugValue>, String> {
        crate::debug_value::variables(self, frame)
    }

    fn expand(&self, handle: &str) -> Result<Vec<DebugValue>, String> {
        let object: Id = handle.parse().map_err(|_| "not an object handle".to_string())?;
        crate::debug_value::expand(self, object)
    }

    fn watch(&self, frame: usize, expression: &str) -> Result<DebugValue, String> {
        crate::debug_value::watch(self, frame, expression)
    }

    fn reinstall(&self, config: &DebugConfig) -> Result<(), String> {
        self.set_breakpoints(config.breakpoints.clone());
        self.set_exceptions(config.exceptions.clone());
        Ok(())
    }
}

fn emit_status(sink: &Arc<dyn EventSink>, id: &str, status: &str, vm: &str, message: &str) {
    let payload = DebugStatus {
        session_id: id.to_string(),
        status: status.to_string(),
        vm: vm.to_string(),
        message: message.to_string(),
        engine: "jvm".to_string(),
        // A JVM session has no equivalent of the missing-formatters caveat: the VM renders its own
        // values and there is nothing to install.
        note: String::new(),
    };
    sink.emit(EVT_DEBUG_STATUS, serde_json::to_value(payload).unwrap_or(json!({})));
}

// ── the session ────────────────────────────────────────────────────────────────

/// The live sessions, by id. The id is the **run id**, so the Run console tab and the debug
/// session are the same thing to everything that has to correlate them.
fn registry() -> &'static Mutex<HashMap<String, Arc<Session>>> {
    static REG: OnceLock<Mutex<HashMap<String, Arc<Session>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

fn session_of(id: &str) -> Option<Arc<Session>> {
    registry().lock().unwrap_or_else(|p| p.into_inner()).get(id).cloned()
}

/// A breakpoint the way the session holds it: what the user set, plus what the VM made of it.
struct Bp {
    at: Breakpoint,
    /// The parsed condition, when there is one. Parsed **once**, when the set is installed, rather
    /// than at each hit: a condition on a hot line is asked thousands of times a second, and a
    /// parse error has to be reported when the breakpoint is set rather than the first time the
    /// line happens to run.
    condition: Option<crate::debug_cond::Cond>,
    /// Why the condition could not be used — a parse error, or the last evaluation failure. Shown
    /// on the breakpoint itself, because that is the thing that is broken.
    condition_error: String,
    /// How many times it has stopped the program this session — what a pass count counts.
    hits: u32,
    /// Event requests to clear when it is removed or the set is replaced, each with the kind it
    /// was set with. The kind is carried rather than assumed because a single breakpoint holds
    /// both `BREAKPOINT` requests (where it is installed) and `CLASS_PREPARE` ones (what it is
    /// still waiting for), and `EventRequest.Clear` refuses a kind that does not match — so a
    /// list that assumed one kind would leak the other on every edit.
    requests: Vec<(u8, i32)>,
    verified: bool,
    message: String,
}

/// A configured breakpoint as the session holds it, with its condition parsed.
///
/// A condition that does not parse leaves the breakpoint **unconditional** and says so, rather than
/// disabling it: a typo must not silently remove a breakpoint you are standing at, and a stop you
/// did not want is recoverable in one keystroke while a stop that never happens is not.
fn new_bp(at: Breakpoint) -> Bp {
    let (condition, condition_error) = match crate::debug_cond::parse(&at.condition) {
        Ok(parsed) => (parsed, String::new()),
        Err(why) => (None, format!("condition ignored — {why}")),
    };
    Bp {
        at,
        condition,
        condition_error,
        hits: 0,
        requests: Vec::new(),
        verified: false,
        message: String::new(),
    }
}

/// Where a suspended thread is. Frame ids are valid **only** while it stays suspended —
/// resuming invalidates every one the VM handed out, which is why they are dropped wholesale
/// rather than refreshed.
struct Paused {
    thread: Id,
    frames: Vec<Frame>,
}

#[derive(Default)]
struct Mutable {
    paused: Option<Paused>,
    breakpoints: Vec<Bp>,
    exceptions: Vec<(ExceptionBreakpoint, Vec<i32>)>,
    /// The in-flight step request, cleared when its event lands — a step request left set makes
    /// the next resume step again.
    step: Option<i32>,
    /// How many times running a step has been silently continued because it landed somewhere
    /// there is nothing to show. Bounded by [`MAX_STEP_SKIPS`] and reset on a real stop.
    skips: u32,
    /// Breakpoints are **muted**: still set, still listed, but not installed in the VM.
    ///
    /// The point is to run a program to its end without deleting the twelve breakpoints you
    /// will want back in a minute — which is why this clears the VM's requests and leaves the
    /// model exactly as it is.
    muted: bool,
}

/// What the VM already told us, so it is not asked twice. A stack of forty frames in a
/// recursive method is one class and one method asked about forty times.
#[derive(Default)]
pub(crate) struct Cache {
    signatures: HashMap<Id, String>,
    methods: HashMap<Id, Vec<Method>>,
    lines: HashMap<(Id, Id), Vec<LineEntry>>,
    variables: HashMap<(Id, Id), Vec<Local>>,
    fields: HashMap<Id, Vec<Field>>,
}

pub(crate) struct Session {
    pub(crate) id: String,
    pub(crate) root: String,
    pub(crate) client: Client,
    sink: Arc<dyn EventSink>,
    /// Fully-qualified class name → the file declaring it, for this project.
    classes: HashMap<String, String>,
    /// The reverse: a normalized file path → every type declared in it. What turns "line 118 of
    /// Order.java" into classes the VM can be asked about.
    by_file: HashMap<String, Vec<String>>,
    /// What a step passes through. Read once, at attach: a step re-issues itself while it is
    /// crossing generated code, and re-reading a file forty times to answer the same question
    /// would be forty file reads for a value that cannot have changed. An edit applies to the
    /// next launch.
    excludes: Vec<String>,
    state: Mutex<Mutable>,
    pub(crate) cache: Mutex<Cache>,
}

impl Session {
    fn new(
        id: String,
        root: String,
        client: Client,
        sink: Arc<dyn EventSink>,
        config: DebugConfig,
    ) -> Session {
        let mut classes = HashMap::new();
        let mut by_file: HashMap<String, Vec<String>> = HashMap::new();
        if let Some(entries) = IndexService::global().class_index(&root) {
            for entry in entries {
                by_file.entry(normalize(&entry.file)).or_default().push(entry.fqcn.clone());
                classes.entry(entry.fqcn).or_insert(entry.file);
            }
        }
        let state = Mutable {
            breakpoints: config
                .breakpoints
                .into_iter()
                .map(|at| Bp { at, requests: Vec::new(), verified: false, message: String::new() })
                .collect(),
            exceptions: config.exceptions.into_iter().map(|e| (e, Vec::new())).collect(),
            ..Mutable::default()
        };
        Session {
            id,
            root,
            client,
            sink,
            classes,
            by_file,
            excludes: step_excludes(),
            state: Mutex::new(state),
            cache: Mutex::new(Cache::default()),
        }
    }

    /// The exclusion patterns as the protocol wants them.
    fn exclude_refs(&self) -> Vec<&str> {
        self.excludes.iter().map(String::as_str).collect()
    }

    // ── events ─────────────────────────────────────────────────────────────────

    fn on_event(&self, event: Event, policy: SuspendPolicy) {
        match event {
            // With `suspend=y` the VM is frozen here and everything is already installed, so
            // this is where the program is let go. With `suspend=n` nothing is suspended and
            // the policy says so.
            Event::VmStart { .. } => {
                if policy != SuspendPolicy::None {
                    let _ = resume_vm(&self.client);
                }
            }
            Event::VmDeath { .. } => {
                // The socket closes right after; `serve` emits the terminated status when its
                // event channel ends, so there is one place that says the session is over.
            }
            Event::ClassPrepare { thread, signature, .. } => {
                self.on_class_prepare(&signature);
                // A class-prepare suspends only its own thread (that is what makes installing
                // in time possible). Forgetting this resume is how a debugged program appears
                // to hang the moment a watched class loads.
                let _ = resume_thread(&self.client, thread);
            }
            Event::Breakpoint { request, thread, .. } => {
                // A condition and a pass count are both checked HERE, after the VM has already
                // stopped, because that is the only place the frame they talk about exists. When
                // they do not hold the program is let go without anyone ever being told it
                // stopped — which is also why a condition on a hot line costs what it costs.
                if !self.should_stop(request, thread) {
                    let _ = resume_vm(&self.client);
                    return;
                }
                self.on_stop(thread, "breakpoint", None);
            }
            Event::Step { thread, location, .. } => {
                self.clear_step();
                // A step that landed in generated code has arrived nowhere: keep going rather
                // than stopping the reader in front of a proxy they cannot see.
                if self.is_opaque(location) && self.step_again(thread) {
                    return;
                }
                self.on_stop(thread, "step", None);
            }
            Event::Exception { thread, exception, .. } => {
                let name = self.type_name_of(exception.1);
                self.on_stop(thread, "exception", name);
            }
            Event::ThreadStart { .. } | Event::ThreadDeath { .. } | Event::Other { .. } => {}
        }
    }

    /// Whether the breakpoint behind `request` says to stop this time — condition first, then the
    /// pass count.
    ///
    /// **In that order**, because the other one does not compose: "the third time `i > 5`" is a
    /// question anybody might ask, while "`i > 5` on the third hit, whatever `i` was on the first
    /// two" is not one anyone means. So a hit the condition rejected is not counted at all.
    ///
    /// A plain breakpoint still passes through here rather than short-circuiting, because the hit
    /// count is worth having on every one of them — and this only runs when the VM has already
    /// stopped, which costs milliseconds of round trips either way. A lock is not what to save here.
    ///
    /// `true` as well when the condition **could not be answered** — a null halfway down the path,
    /// a field that is not there on this subclass. That is deliberate and is the whole safety
    /// property of the feature: a condition that errors is a bug in the condition, the only way to
    /// see it is to be standing there, and silently continuing would turn a typo into a breakpoint
    /// that never fires and never explains itself. The reason is recorded on the breakpoint, so the
    /// gutter and the Breakpoints window say why the stop happened.
    fn should_stop(&self, request: i32, thread: Id) -> bool {
        let Some((index, cond, every)) = ({
            let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state
                .breakpoints
                .iter()
                .position(|b| {
                    b.requests.iter().any(|(k, r)| *k == kind::BREAKPOINT && *r == request)
                })
                .map(|i| {
                    let bp = &state.breakpoints[i];
                    (i, bp.condition.clone(), bp.at.hit_count)
                })
        }) else {
            // A request nothing claims — the set was edited between the hit and this lock. Stop:
            // the alternative silently swallows a breakpoint over a race.
            return true;
        };
        if let Some(cond) = cond {
            if !self.condition_holds(index, thread, &cond) {
                return false;
            }
        }
        self.count_hit(index, every)
    }

    /// Evaluate one breakpoint's condition in frame 0 of the stopped thread — the frame its line is
    /// in. Read directly rather than through `Paused`, which is only set once a stop has been
    /// announced: publishing a pause and taking it back is a state the panel would briefly render.
    fn condition_holds(&self, index: usize, thread: Id, cond: &crate::debug_cond::Cond) -> bool {
        let Some(frame) = frames(&self.client, thread).ok().and_then(|f| f.into_iter().next())
        else {
            self.note_condition(index, "condition skipped — the stopped thread has no frame");
            return true;
        };
        match crate::debug_cond::holds(self, thread, &frame, cond) {
            Ok(hit) => {
                self.note_condition(index, "");
                hit
            }
            Err(why) => {
                self.note_condition(index, &format!("stopped anyway — {why}"));
                true
            }
        }
    }

    /// Count a hit the condition accepted, and say whether the pass count lets it through.
    ///
    /// `every <= 1` is "every hit", so the counter still advances — the number is worth having on
    /// its own ("is this line even running" is otherwise only answerable by adding a log line and
    /// rebuilding), and it is what the Breakpoints window shows.
    fn count_hit(&self, index: usize, every: u32) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let Some(bp) = state.breakpoints.get_mut(index) else { return true };
        bp.hits = bp.hits.saturating_add(1);
        every <= 1 || bp.hits % every == 0
    }

    /// Record (and publish) what happened to a breakpoint's condition. Publishes only on a change,
    /// so a condition that is fine does not emit an event on every hit of a hot line.
    fn note_condition(&self, index: usize, why: &str) {
        {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            let Some(bp) = state.breakpoints.get_mut(index) else { return };
            if bp.condition_error == why {
                return;
            }
            bp.condition_error = why.to_string();
        }
        self.emit_breakpoints();
    }

    /// Whether there is nothing to show at `at` — so a step that landed there has not arrived.
    ///
    /// Two ways to be nowhere. **Generated code**: a Spring CGLIB proxy, a JDK proxy, a
    /// Javassist or Hibernate stand-in — its methods exist only at runtime, and stopping in one
    /// puts the reader in a class whose source has never been written. **No line at all**: a
    /// method compiled without debug information has no line table, so the stop has no place in
    /// any file to point at.
    ///
    /// The class-exclude filters on the step handle the *frameworks* between a caller and its
    /// target; they cannot handle the proxy, because a proxy of `com.acme.OrderService` is
    /// itself named `com.acme.…` and no pattern excluding the framework can reach it.
    fn is_opaque(&self, at: Location) -> bool {
        let class = self.class_name_of(at.class);
        arbor_logscan::prelude::is_synthetic(&class) || self.line_of(at).is_none()
    }

    /// Continue a step that landed nowhere. `false` when the budget is spent — the caller then
    /// stops where it is, because a debugger that never comes back is worse than one that stops
    /// somewhere unhelpful.
    ///
    /// Always **into**: the point is to get out of generated code and into the method that was
    /// meant, and the frames it is passing through are exactly the ones a step-over would have
    /// returned from empty-handed.
    fn step_again(&self, thread: Id) -> bool {
        {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            if state.skips >= MAX_STEP_SKIPS {
                state.skips = 0;
                return false;
            }
            state.skips += 1;
        }
        let Ok(request) =
            request_step(&self.client, thread, StepDepth::Into, &self.exclude_refs(), SuspendPolicy::All)
        else {
            return false;
        };
        self.state.lock().unwrap_or_else(|p| p.into_inner()).step = Some(request);
        resume_vm(&self.client).is_ok()
    }

    /// The program stopped: read the stack while it is still suspended, then say so.
    fn on_stop(&self, thread: Id, reason: &str, exception: Option<String>) {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).skips = 0;
        let stack = frames(&self.client, thread).unwrap_or_default();
        let cut: Vec<Frame> = stack.into_iter().take(MAX_FRAMES).collect();
        let described: Vec<StackFrame> = cut
            .iter()
            .enumerate()
            .map(|(i, f)| self.describe(i as u32, f))
            .collect();
        let thread_name = thread_name(&self.client, thread).unwrap_or_default();

        // Stored last, and briefly: the round trips above must not happen under the lock a
        // handler needs to answer "what are the variables here".
        self.state.lock().unwrap_or_else(|p| p.into_inner()).paused =
            Some(Paused { thread, frames: cut });

        let payload = DebugPause {
            session_id: self.id.clone(),
            thread: thread.to_string(),
            thread_name,
            reason: reason.to_string(),
            exception,
            frames: described,
        };
        self.sink.emit(EVT_DEBUG_PAUSED, serde_json::to_value(payload).unwrap_or(json!({})));
        emit_status(&self.sink, &self.id, "paused", "", "");
        // The hit counts moved, and a stop is the one moment they are worth publishing: they are
        // read while the program is standing still, and emitting on every rejected hit of a hot
        // line would be the slowest thing in the debugger.
        self.emit_breakpoints();
    }

    /// A class this project declares just loaded — install whatever was waiting for it.
    fn on_class_prepare(&self, signature: &str) {
        let fqcn = class_name(signature);
        let Some(file) = self.file_of(&fqcn) else { return };
        let indices: Vec<usize> = {
            let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state
                .breakpoints
                .iter()
                .enumerate()
                .filter(|(_, b)| b.at.enabled && !b.verified && same_file(&b.at.file, &file))
                .map(|(i, _)| i)
                .collect()
        };
        if indices.is_empty() {
            return;
        }
        for i in indices {
            self.install_one(i);
        }
        self.emit_breakpoints();
    }

    // ── describing a frame ─────────────────────────────────────────────────────

    /// A JDWP location as something the editor can open.
    fn describe(&self, index: u32, frame: &Frame) -> StackFrame {
        let class = self.class_name_of(frame.location.class);
        let method = self
            .methods_of(frame.location.class)
            .iter()
            .find(|m| m.id == frame.location.method)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        let line = self.line_of(frame.location);
        let file = self.file_of(&class);
        // `name` stays empty: a JVM frame HAS a class and a method, so inventing a combined string
        // would be a second rendering of what the panel already composes from the two.
        StackFrame { index, class, method, name: String::new(), line, project: file.is_some(), file }
    }

    /// The source line a bytecode index falls on: the **last** table entry at or before it,
    /// because a line's entry marks where it *starts* and execution is somewhere inside it.
    fn line_of(&self, at: Location) -> Option<u32> {
        let table = self.line_table_of(at.class, at.method);
        table
            .iter()
            .filter(|e| e.index <= at.index)
            .max_by_key(|e| e.index)
            .map(|e| e.line.max(0) as u32)
    }

    /// The file this project declares `class` in. A nested class is looked up through its outer
    /// name, which is the one with a file of its own.
    ///
    /// A **generated** class has none, and must not borrow one. `OrderService$$SpringCGLIB$$0`
    /// looks like a nested class of `OrderService` to any name-based lookup, so resolving it
    /// through the outer name opens `OrderService.java` and lands on whatever line the proxy's
    /// synthetic method claims — the right file, at a meaningless point in it. That is worse
    /// than not opening anything, because it looks like it worked.
    fn file_of(&self, class: &str) -> Option<String> {
        if arbor_logscan::prelude::is_synthetic(class) {
            return None;
        }
        let outer = arbor_logscan::prelude::outer_class(class);
        self.classes.get(outer).or_else(|| self.classes.get(class)).cloned()
    }

    // ── the caches ─────────────────────────────────────────────────────────────

    pub(crate) fn class_name_of(&self, class: Id) -> String {
        class_name(&self.signature_of(class))
    }

    pub(crate) fn signature_of(&self, class: Id) -> String {
        if let Some(s) = self.cache.lock().unwrap_or_else(|p| p.into_inner()).signatures.get(&class)
        {
            return s.clone();
        }
        let signature = type_signature(&self.client, class).unwrap_or_default();
        self.cache
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .signatures
            .insert(class, signature.clone());
        signature
    }

    fn methods_of(&self, class: Id) -> Vec<Method> {
        if let Some(m) = self.cache.lock().unwrap_or_else(|p| p.into_inner()).methods.get(&class) {
            return m.clone();
        }
        let list = methods(&self.client, class).unwrap_or_default();
        self.cache.lock().unwrap_or_else(|p| p.into_inner()).methods.insert(class, list.clone());
        list
    }

    fn line_table_of(&self, class: Id, method: Id) -> Vec<LineEntry> {
        let key = (class, method);
        if let Some(t) = self.cache.lock().unwrap_or_else(|p| p.into_inner()).lines.get(&key) {
            return t.clone();
        }
        let table = line_table(&self.client, class, method).unwrap_or_default();
        self.cache.lock().unwrap_or_else(|p| p.into_inner()).lines.insert(key, table.clone());
        table
    }

    pub(crate) fn variables_of(&self, class: Id, method: Id) -> Vec<Local> {
        let key = (class, method);
        if let Some(t) = self.cache.lock().unwrap_or_else(|p| p.into_inner()).variables.get(&key) {
            return t.clone();
        }
        // Absent information (a class compiled without `-g:vars`) is an empty table, not an
        // error: the panel then shows `this` and says why, which is the honest answer.
        let table = variable_table(&self.client, class, method).unwrap_or_default();
        self.cache.lock().unwrap_or_else(|p| p.into_inner()).variables.insert(key, table.clone());
        table
    }

    pub(crate) fn fields_of(&self, class: Id) -> Vec<Field> {
        if let Some(f) = self.cache.lock().unwrap_or_else(|p| p.into_inner()).fields.get(&class) {
            return f.clone();
        }
        let list = fields(&self.client, class).unwrap_or_default();
        self.cache.lock().unwrap_or_else(|p| p.into_inner()).fields.insert(class, list.clone());
        list
    }

    /// The runtime type of an object handle, as a reader sees it (`Order`, `int[]`). `None` for
    /// null. Goes through the descriptor rather than the class name because an array's "name"
    /// *is* a descriptor — `[I`, which means nothing to anyone reading a variables panel.
    pub(crate) fn type_name_of(&self, object: Id) -> Option<String> {
        if object == 0 {
            return None;
        }
        let class = object_type(&self.client, object).ok()?;
        Some(crate::debug_value::type_display(&self.signature_of(class.id)))
    }

    // ── breakpoints ────────────────────────────────────────────────────────────

    /// Replace the whole set. The FE owns the model and pushes it entire, which is what makes
    /// "what the gutter shows" and "what the VM has" one thing rather than two that drift.
    fn set_breakpoints(&self, wanted: Vec<Breakpoint>) {
        let old: Vec<Bp> = {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            std::mem::replace(
                &mut state.breakpoints,
                wanted.into_iter().map(new_bp).collect(),
            )
        };
        for bp in &old {
            for (kind, request) in &bp.requests {
                let _ = clear_event(&self.client, *kind, *request);
            }
        }
        self.install_all();
        self.emit_breakpoints();
    }

    fn install_all(&self) {
        let count = self.state.lock().unwrap_or_else(|p| p.into_inner()).breakpoints.len();
        for i in 0..count {
            self.install_one(i);
        }
        self.emit_breakpoints();
    }

    /// Mute or unmute: drop every request the VM holds, or put them all back.
    ///
    /// Not a filter applied when an event arrives — the requests themselves go, so a muted
    /// program runs at full speed instead of stopping thousands of times to be told to carry
    /// on. The breakpoints are untouched, which is the whole point of muting rather than
    /// deleting.
    fn set_muted(&self, muted: bool) {
        let old: Vec<Vec<(u8, i32)>> = {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            if state.muted == muted {
                return;
            }
            state.muted = muted;
            state
                .breakpoints
                .iter_mut()
                .map(|b| {
                    b.verified = false;
                    b.message = if muted { "muted".to_string() } else { String::new() };
                    std::mem::take(&mut b.requests)
                })
                .collect()
        };
        for requests in &old {
            for (kind, request) in requests {
                let _ = clear_event(&self.client, *kind, *request);
            }
        }
        if muted {
            self.emit_breakpoints();
        } else {
            self.install_all();
        }
    }

    /// Install breakpoint `i` against every class of its file the VM has already loaded, and ask
    /// to be told about the ones it has not.
    ///
    /// Two requests per top-level type — the type itself and `Type$*` — because a breakpoint
    /// inside an anonymous class body belongs to `Order$1`, which no source scan knows the name
    /// of. A lambda body needs neither: it compiles to a synthetic method *of the enclosing
    /// class*, so the enclosing class's line table already has it.
    fn install_one(&self, i: usize) {
        let Some((at, already, muted)) = ({
            let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state.breakpoints.get(i).map(|b| (b.at.clone(), b.verified, state.muted))
        }) else {
            return;
        };
        if !at.enabled || already || muted {
            return;
        }

        let declared = self.by_file.get(&normalize(&at.file)).cloned().unwrap_or_default();
        if declared.is_empty() {
            self.mark(i, false, "no class of this project is declared in that file", Vec::new());
            return;
        }

        let mut requests = Vec::new();
        // Where it really landed. A comment or a blank line compiles to nothing, so the VM binds
        // the statement under it — true to what the click meant, and worth saying out loud.
        let mut bound: Option<u32> = None;
        for fqcn in &declared {
            for class in
                classes_by_signature(&self.client, &class_signature(fqcn)).unwrap_or_default()
            {
                let methods = self.methods_of(class.id);
                let Ok(Some(location)) =
                    location_of_line(&self.client, class.id, &methods, at.line as i32)
                else {
                    continue;
                };
                if let Ok(request) = set_breakpoint(&self.client, location, SuspendPolicy::All) {
                    requests.push((kind::BREAKPOINT, request));
                    let line = self.line_of(location).unwrap_or(at.line);
                    bound = Some(bound.map_or(line, |b| b.min(line)));
                }
            }
        }

        if let Some(line) = bound {
            let message = if line == at.line {
                String::new()
            } else {
                format!("line {} has no code — stopping at line {line}", at.line)
            };
            self.mark(i, true, &message, requests);
            return;
        }

        // Not loaded yet — the normal case for everything but the class you launched from, and
        // it resolves itself the moment the program touches it.
        //
        // Two patterns per top-level type: the type, and `Type$*`. The second is not
        // redundant — a breakpoint inside an anonymous class body belongs to `Order$1`, and no
        // scan of the source knows that name.
        let mut waiting = requests;
        for fqcn in declared.iter().filter(|f| !f.contains('$')) {
            for pattern in [fqcn.to_string(), format!("{fqcn}$*")] {
                if let Ok(r) = request_class_prepare(&self.client, &pattern) {
                    waiting.push((kind::CLASS_PREPARE, r));
                }
            }
        }
        self.mark(i, false, "waiting for the class to load", waiting);
    }

    fn mark(&self, i: usize, verified: bool, message: &str, requests: Vec<(u8, i32)>) {
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(bp) = state.breakpoints.get_mut(i) {
            bp.verified = verified;
            bp.message = message.to_string();
            bp.requests.extend(requests);
        }
    }

    fn install_exceptions(&self) {
        let wanted: Vec<ExceptionBreakpoint> = {
            let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state.exceptions.iter().map(|(e, _)| e.clone()).collect()
        };
        let mut installed: Vec<Vec<i32>> = Vec::with_capacity(wanted.len());
        for e in &wanted {
            let mut requests = Vec::new();
            if e.enabled && (e.caught || e.uncaught) {
                // A named throwable that is not loaded yet cannot be filtered on, so the request
                // is simply not made — narrowing to a class the VM has never heard of would stop
                // on everything instead of on nothing.
                let class = match e.class.trim() {
                    "" => Some(None),
                    name => classes_by_signature(&self.client, &class_signature(name))
                        .ok()
                        .and_then(|c| c.first().map(|c| Some(c.id))),
                };
                if let Some(class) = class {
                    if let Ok(r) = request_exception(
                        &self.client,
                        class,
                        e.caught,
                        e.uncaught,
                        SuspendPolicy::All,
                    ) {
                        requests.push(r);
                    }
                }
            }
            installed.push(requests);
        }
        let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        for (slot, requests) in state.exceptions.iter_mut().zip(installed) {
            slot.1 = requests;
        }
    }

    fn set_exceptions(&self, wanted: Vec<ExceptionBreakpoint>) {
        let old: Vec<(ExceptionBreakpoint, Vec<i32>)> = {
            let mut state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            std::mem::replace(
                &mut state.exceptions,
                wanted.into_iter().map(|e| (e, Vec::new())).collect(),
            )
        };
        for (_, requests) in &old {
            for r in requests {
                let _ = clear_event(&self.client, kind::EXCEPTION, *r);
            }
        }
        self.install_exceptions();
    }

    fn emit_breakpoints(&self) {
        let states: Vec<BreakpointStatus> = {
            let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state
                .breakpoints
                .iter()
                .map(|b| BreakpointStatus {
                    file: b.at.file.clone(),
                    line: b.at.line,
                    verified: b.verified,
                    message: b.message.clone(),
                    condition_error: b.condition_error.clone(),
                    hits: b.hits,
                })
                .collect()
        };
        self.sink.emit(
            EVT_DEBUG_BREAKPOINTS,
            json!({ "session_id": self.id, "root": self.root, "breakpoints": states }),
        );
    }

    // ── running on ─────────────────────────────────────────────────────────────

    /// Let the program go. Every stop suspends the whole VM (see the module doc), so this is
    /// always `resume_vm` — and never sent when nothing is suspended, which would drive some
    /// thread's suspend count below zero.
    fn resume(&self) -> Result<(), String> {
        let was = self.state.lock().unwrap_or_else(|p| p.into_inner()).paused.take();
        if was.is_none() {
            return Ok(());
        }
        resume_vm(&self.client).map_err(|e| e.to_string())?;
        emit_status(&self.sink, &self.id, "running", "", "");
        Ok(())
    }

    fn step(&self, depth: StepDepth) -> Result<(), String> {
        let thread = {
            let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
            state.paused.as_ref().map(|p| p.thread)
        };
        let Some(thread) = thread else { return Err("the program is not stopped".to_string()) };
        self.clear_step();
        let request = request_step(&self.client, thread, depth, &self.exclude_refs(), SuspendPolicy::All)
            .map_err(|e| e.to_string())?;
        self.state.lock().unwrap_or_else(|p| p.into_inner()).step = Some(request);
        self.resume()
    }

    /// Drop the in-flight step request. A step request that outlives its event makes the *next*
    /// resume step as well, which reads as the debugger stopping for no reason.
    fn clear_step(&self) {
        let request = self.state.lock().unwrap_or_else(|p| p.into_inner()).step.take();
        if let Some(r) = request {
            let _ = clear_event(&self.client, kind::SINGLE_STEP, r);
        }
    }

    /// The suspended thread and the frame at `index`, if the program is stopped there.
    pub(crate) fn frame_at(&self, index: usize) -> Option<(Id, Frame)> {
        let state = self.state.lock().unwrap_or_else(|p| p.into_inner());
        let paused = state.paused.as_ref()?;
        Some((paused.thread, *paused.frames.get(index)?))
    }
}

/// A path as a comparison key: forward slashes, and case-folded where the filesystem is.
fn normalize(path: &str) -> String {
    let slashed = path.replace('\\', "/");
    if cfg!(windows) {
        slashed.to_lowercase()
    } else {
        slashed
    }
}

fn same_file(a: &str, b: &str) -> bool {
    normalize(a) == normalize(b)
}

/// `com.acme.Order$Line` → `Order$Line`. What a variables panel shows: the package is the same
/// for every row and buys nothing.
pub(crate) fn simple_name(fqcn: &str) -> String {
    fqcn.rsplit('.').next().unwrap_or(fqcn).to_string()
}

// ── handlers ───────────────────────────────────────────────────────────────────

/// A handler that takes nothing. The seam still passes an `args` object, so it needs a shape.
#[derive(Deserialize, Default)]
#[serde(default)]
pub struct NoArgs {}

/// The class-name patterns a step currently passes through — the configured list, or the
/// defaults when there is none.
///
/// Asked rather than mirrored: the default list is a judgement this side makes and revises, and
/// a copy of it in the settings panel would be a second answer that drifts. The panel shows
/// what is actually in force, and editing any row is what turns the defaults into a list of
/// your own.
#[arbor_rpc::handler]
fn bennu_step_excludes(_ctx: &BennuState, _args: NoArgs) -> Result<Vec<String>, String> {
    Ok(step_excludes())
}

/// Args for [`bennu_debug_check_condition`].
#[derive(Deserialize)]
pub struct CheckConditionArgs {
    /// The file the breakpoint is in — which engine will have to answer it.
    pub file: String,
    pub condition: String,
}

/// What is wrong with a breakpoint condition, or `""` when there is nothing wrong with it.
///
/// Asked while the user types, because a condition is the one setting in the debugger whose
/// mistakes are **invisible at the time you make them**: a bad watch shows an error next to the
/// watch, a bad condition just means the program never stops, ten minutes from now, in a place you
/// are not looking. The parser is the same one the session uses, so what the box accepts and what
/// the debugger accepts cannot drift.
///
/// A native condition is the adapter's own expression language — Bennu has no parser for it and
/// says nothing rather than guessing.
#[arbor_rpc::handler]
fn bennu_debug_check_condition(
    _ctx: &BennuState,
    args: CheckConditionArgs,
) -> Result<String, String> {
    if !crate::intel::is_java_file(&args.file) {
        return Ok(String::new());
    }
    Ok(match crate::debug_cond::parse(&args.condition) {
        Ok(_) => String::new(),
        Err(why) => why,
    })
}

/// Args naming a project root.
#[derive(Deserialize)]
pub struct RootArgs {
    /// Absolute path to the project root — where `[debug]` is persisted.
    pub root: String,
}

/// Args for [`bennu_set_debug_config`].
#[derive(Deserialize)]
pub struct SetDebugConfigArgs {
    pub root: String,
    /// The whole section, replacing what is there.
    pub config: DebugConfig,
}

/// The project's persisted `[debug]` — what the gutter draws when a file opens, and the
/// watches the panel starts with.
#[arbor_rpc::handler]
fn bennu_get_debug_config(_ctx: &BennuState, args: RootArgs) -> Result<DebugConfig, String> {
    Ok(crate::repo_config::load(&args.root, "debug"))
}

/// Persist the debug section **and** push its live half to any running session, in that order.
///
/// One call rather than two: a breakpoint you just added is a breakpoint the running program
/// should respect *and* one that is still there tomorrow, and splitting those into separate
/// verbs is how the two answers start disagreeing. Watches are persisted but not pushed —
/// nothing is installed for a watch, it is evaluated when a frame is selected.
#[arbor_rpc::handler]
fn bennu_set_debug_config(_ctx: &BennuState, args: SetDebugConfigArgs) -> Result<(), String> {
    crate::repo_config::save(&args.root, "debug", &args.config)?;

    // Every live session of that root, whichever protocol it speaks: a push that silently reached the
    // Java one and not the Rust one would be worse than one that reached neither.
    for session in live_sessions_of(&args.root) {
        let _ = session.reinstall(&args.config);
    }
    Ok(())
}

/// Every live session debugging `root`. A list rather than an option because nothing here
/// forbids two, and a breakpoint push that silently reached only one of them would be worse
/// than one that reaches none.
fn live_sessions_of(root: &str) -> Vec<Arc<dyn crate::debug_backend::DebugBackend>> {
    crate::debug_backend::registry()
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .values()
        .filter(|s| same_file(&s.root(), root))
        .cloned()
        .collect()
}

/// Args naming a session.
#[derive(Deserialize)]
pub struct SessionArgs {
    pub session_id: String,
}

/// Args naming a session and a frame of its stopped thread.
#[derive(Deserialize)]
pub struct FrameArgs {
    pub session_id: String,
    /// 0 = the innermost frame.
    #[serde(default)]
    pub frame: usize,
}

/// Args for a step.
#[derive(Deserialize)]
pub struct StepArgs {
    pub session_id: String,
    /// `into` · `over` · `out`.
    pub depth: String,
}

/// Let the program run on.
#[arbor_rpc::handler]
fn bennu_debug_resume(_ctx: &BennuState, args: SessionArgs) -> Result<(), String> {
    crate::debug_backend::get(&args.session_id)?.resume()
}

/// One step, at line granularity, skipping the JDK and other people's frameworks
/// ([`DEFAULT_STEP_EXCLUDES`], or the configured list).
#[arbor_rpc::handler]
fn bennu_debug_step(_ctx: &BennuState, args: StepArgs) -> Result<(), String> {
    let step = crate::debug_backend::Step::parse(&args.depth);
    crate::debug_backend::get(&args.session_id)?.step(step)
}

/// Args for [`bennu_debug_mute`].
#[derive(Deserialize)]
pub struct MuteArgs {
    pub session_id: String,
    pub muted: bool,
}

/// Mute or unmute this session's breakpoints — set and listed, but not installed.
///
/// For running a program to its end without deleting the twelve breakpoints you will want back
/// in a minute. Exception breakpoints are left alone: they exist for the throw you did *not*
/// predict, and muting the ones you placed on purpose is a different intent.
#[arbor_rpc::handler]
fn bennu_debug_mute(_ctx: &BennuState, args: MuteArgs) -> Result<(), String> {
    crate::debug_backend::get(&args.session_id)?.set_muted(args.muted)
}

/// Detach: the program keeps running, unsuspended, with no debugger attached.
///
/// Deliberately not a kill. Stopping the *program* is the Run console's Stop, which kills the
/// process tree; a server you attached to in order to look at one request should not die
/// because you finished looking.
#[arbor_rpc::handler]
fn bennu_debug_detach(_ctx: &BennuState, args: SessionArgs) -> Result<(), String> {
    crate::debug_backend::get(&args.session_id)?.detach()
}

/// The variables in scope at a frame of the stopped thread.
#[arbor_rpc::handler]
fn bennu_debug_variables(_ctx: &BennuState, args: FrameArgs) -> Result<Vec<DebugValue>, String> {
    crate::debug_backend::get(&args.session_id)?.variables(args.frame)
}

/// Args for expanding one object.
#[derive(Deserialize)]
pub struct ExpandArgs {
    pub session_id: String,
    /// The handle from a [`DebugValue::object`].
    pub object: String,
}

/// What is inside an object: its fields (its own and its superclasses'), or an array's
/// elements.
#[arbor_rpc::handler]
fn bennu_debug_expand(_ctx: &BennuState, args: ExpandArgs) -> Result<Vec<DebugValue>, String> {
    crate::debug_backend::get(&args.session_id)?.expand(&args.object)
}

/// Args for a dump: the row that was clicked, whole.
///
/// The row rather than just its handle, because a handle alone cannot be described — neither protocol
/// has a "what is this reference" request, and the name and declared type are what the header line is
/// made of. The frontend has the row already, so sending it back costs nothing.
#[derive(Deserialize)]
pub struct DumpArgs {
    pub session_id: String,
    pub value: DebugValue,
}

/// One value and everything under it, as RON-shaped text — see [`crate::debug_dump`].
///
/// Protocol-blind: it walks `expand`, so the same modal answers on a Java object graph and a Rust one.
#[arbor_rpc::handler]
fn bennu_debug_dump(_ctx: &BennuState, args: DumpArgs) -> Result<DebugDump, String> {
    let backend = crate::debug_backend::get(&args.session_id)?;
    Ok(crate::debug_dump::dump(backend.as_ref(), &args.value))
}

/// Args for a watch.
#[derive(Deserialize)]
pub struct WatchArgs {
    pub session_id: String,
    #[serde(default)]
    pub frame: usize,
    /// The path to follow — `order`, `order.customer.name`, `items[2]`.
    pub expression: String,
}

/// Evaluate a watch against a frame. See [`crate::debug_value::watch`] for what an expression
/// is allowed to be, and why it is a path rather than Java.
#[arbor_rpc::handler]
fn bennu_debug_watch(_ctx: &BennuState, args: WatchArgs) -> Result<DebugValue, String> {
    crate::debug_backend::get(&args.session_id)?.watch(args.frame, &args.expression)
}

fn session(id: &str) -> Result<Arc<Session>, String> {
    session_of(id).ok_or_else(|| "that debug session is no longer live".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The agent argument is the whole of how a launch becomes debuggable, and every piece of
    /// it matters: `server=n` because we listened first, the loopback address because a debug
    /// port open to the network is a remote-code-execution hole, and the suspend flag because
    /// it is the one thing the run configuration chooses.
    #[test]
    fn the_agent_argument_says_connect_back_to_loopback() {
        let arg = agent_arg(54321, false);
        assert!(arg.starts_with("-agentlib:jdwp="));
        assert!(arg.contains("server=n"), "the debugger listens, not the VM");
        assert!(arg.contains("suspend=n"));
        assert!(arg.contains("address=127.0.0.1:54321"), "loopback only");

        assert!(agent_arg(1, true).contains("suspend=y"));
    }

    /// Path comparison has to survive the two spellings the same file arrives in: the editor's
    /// (whatever the OS handed it) and the index's (forward slashes).
    #[test]
    fn a_file_matches_itself_however_it_was_spelled() {
        assert!(same_file("C:/p/src/Order.java", "C:/p/src/Order.java"));
        assert!(same_file("C:\\p\\src\\Order.java", "C:/p/src/Order.java"));
        assert!(!same_file("C:/p/src/Order.java", "C:/p/src/Invoice.java"));
    }

    #[test]
    fn a_variables_row_is_named_without_its_package() {
        assert_eq!(simple_name("com.acme.Order"), "Order");
        assert_eq!(simple_name("com.acme.Order$Line"), "Order$Line");
        assert_eq!(simple_name("int"), "int");
    }

    /// The exclusion list is what makes *step into* usable. If it ever stops covering the JDK,
    /// stepping quietly becomes a tour of `ArrayList`; if it stops covering Spring's AOP, a
    /// step into an injected bean becomes a tour of `ReflectiveMethodInvocation`.
    #[test]
    fn stepping_skips_the_jdk_and_the_machinery_in_the_way() {
        for pattern in ["java.*", "jdk.*", "sun.*", "org.springframework.*", "org.aopalliance.*"] {
            assert!(DEFAULT_STEP_EXCLUDES.contains(&pattern), "missing {pattern}");
        }
    }

    /// The exclusions are class-name patterns, and JDWP allows a `*` only at one end. A proxy
    /// of `com.acme.OrderService` is itself called `com.acme.…`, so **no** exclusion can reach
    /// it — which is why landing in one is handled after the step rather than before it.
    #[test]
    fn every_exclusion_is_a_pattern_the_vm_accepts() {
        for pattern in DEFAULT_STEP_EXCLUDES {
            assert!(is_valid_pattern(pattern), "`{pattern}` has a star the VM will not accept");
        }
    }

    /// One bad pattern makes the VM refuse the whole request, and stepping then appears to have
    /// stopped working with nothing said anywhere — so a hand-edited config is filtered, not
    /// forwarded.
    #[test]
    fn a_star_in_the_middle_is_refused_before_the_vm_sees_it() {
        assert!(is_valid_pattern("java.*"));
        assert!(is_valid_pattern("*.internal"));
        assert!(is_valid_pattern("com.acme.Order"));
        assert!(!is_valid_pattern("org.*.internal.*"));
        assert!(!is_valid_pattern("com.*.Foo"));
        assert!(!is_valid_pattern("*"));
        assert!(!is_valid_pattern(""));
    }
}
