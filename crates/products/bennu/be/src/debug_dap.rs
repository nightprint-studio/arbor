//! A debug session over DAP — the Rust side of what `debug.rs` is for Java.
//!
//! [`bennu_dap`] owns the protocol; this owns what a *session* is: which handles are still valid, how
//! a DAP frame and a DAP variable become Bennu's own, and how the panel's eight operations map onto
//! the adapter's requests. Implements [`crate::debug_backend::DebugBackend`], so the ten
//! `bennu_debug_*` handlers reach it without knowing it exists.
//!
//! ## Handles die on every resume, and that is the whole design
//!
//! DAP hands out a `frameId` per frame and a `variablesReference` per expandable value, and **both are
//! void the moment the debuggee continues** — an adapter is entitled to reuse the numbers for something
//! else. The frontend, meanwhile, addresses a value by a string handle it got earlier and may click a
//! minute later.
//!
//! So the session keeps a **generation**: every handle it hands out is `<gen>:<ref>`, and a handle from
//! an older generation is refused by name instead of being sent to the adapter. Without that, expanding
//! a stale row does not fail — it answers about *whatever now has that number*, which is a debugger
//! quietly showing you the wrong object's fields. That is the same discipline the JDWP session keeps
//! for its frame ids, for the same reason, and it is worth the bookkeeping in both.
//!
//! ## Threads: one, pinned
//!
//! DAP is multi-threaded and `stopped` names a thread. This session works on **the thread that
//! stopped**, which is what the panel can draw today — the same shape the JDWP session has. A thread
//! selector is a feature, not an omission, and it is stated rather than implied.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use bennu_dap::prelude::*;
use bennu_proto::prelude::{
    BreakpointStatus, DebugConfig, DebugPause, DebugStatus, DebugValue, StackFrame as FrameDto,
};
use arbor_ipc::prelude::EventSink;
use serde_json::json;

use crate::debug_backend::{DebugBackend, Step};

/// How many frames of a stopped stack to fetch.
///
/// A runaway recursion has hundreds of thousands, and an adapter that did not claim
/// `supportsDelayedStackTraceLoading` sends every one it is asked for.
const MAX_FRAMES: u32 = 512;

/// How many elements of one container to fetch. A `Vec` with a million entries is a legal thing to
/// have stopped inside, and the row says how many there really are.
const MAX_ELEMENTS: u32 = 500;

/// The events the frontend already listens to, **verbatim** — a DAP session and a JDWP one are the
/// same thing to the panel, and inventing a second set of names here would have been the one way to
/// break that promise while the trait appeared to keep it. The payload shapes are
/// `bennu_proto`'s `DebugPause` / `BreakpointStatus`, for the same reason.
const EVT_DEBUG_STATUS: &str = "arbor://bennu/debug-status";
const EVT_DEBUG_PAUSED: &str = "arbor://bennu/debug-paused";
const EVT_DEBUG_BREAKPOINTS: &str = "arbor://bennu/debug-breakpoints";
/// The Run console's own stream — a program's output belongs there whether or not it is being
/// debugged, and the debugger has no second console.
const EVT_RUN_OUTPUT: &str = "arbor://bennu/run-output";
const EVT_RUN_EXIT: &str = "arbor://bennu/run-exit";

/// What the session knows only while the debuggee is stopped.
#[derive(Default)]
struct Stopped {
    /// The frames as the adapter reported them, innermost first. The index into this is what the
    /// frontend calls a frame number.
    frames: Vec<StackFrame>,
    /// Which thread they belong to.
    thread: i64,
}

pub struct DapSession {
    id: String,
    root: String,
    /// Which adapter is driving. Kept as the spec rather than as its label because the label is the
    /// least interesting thing about it: which expression dialects it honours and how it renders Rust
    /// values are both answered from here — see [`crate::debug_expr`].
    spec: &'static AdapterSpec,
    /// How Rust's own types will render on this machine, decided once at launch. `Raw` is what a
    /// `Vec`-as-a-pointer-and-a-length looks like from the inside, and the panel says so rather than
    /// leaving the user to conclude the debugger is broken.
    rendering: RustRendering,
    session: Mutex<Option<Session>>,
    sink: Arc<dyn EventSink>,
    /// Bumped on every resume: see the module docs on why a stale handle must be refused rather than
    /// forwarded.
    generation: AtomicU64,
    stopped: Mutex<Stopped>,
    /// The configured breakpoints, kept so muting can put them back.
    config: Mutex<DebugConfig>,
    /// Whether the breakpoints are currently uninstalled.
    muted: std::sync::atomic::AtomicBool,
    /// The last exit code, for the terminated status.
    exit: AtomicI64,
}

impl DapSession {
    /// The handle the frontend gets for an expandable value, tied to this stop.
    pub(crate) fn handle(&self, reference: i64) -> Option<String> {
        (reference != 0).then(|| format!("{}:{}", self.generation.load(Ordering::SeqCst), reference))
    }

    /// Read a handle back, refusing one from an earlier stop.
    fn parse_handle(&self, handle: &str) -> Result<i64, String> {
        let (gen, reference) = handle
            .split_once(':')
            .ok_or_else(|| "not a value handle".to_string())?;
        let gen: u64 = gen.parse().map_err(|_| "not a value handle".to_string())?;
        let reference: i64 = reference.parse().map_err(|_| "not a value handle".to_string())?;
        if gen != self.generation.load(Ordering::SeqCst) {
            // Said plainly, because the alternative — forwarding it — answers about whatever now holds
            // that number, and a debugger showing the wrong object's fields is worse than one that
            // says the row is stale.
            return Err("that value belongs to an earlier stop — the program has run on since".into());
        }
        Ok(reference)
    }

    /// Which adapter is driving this session.
    pub(crate) fn spec(&self) -> &'static AdapterSpec {
        self.spec
    }

    pub(crate) fn with<T>(&self, f: impl FnOnce(&Session) -> Result<T, String>) -> Result<T, String> {
        let guard = self.session.lock().unwrap_or_else(|p| p.into_inner());
        match guard.as_ref() {
            Some(session) => f(session),
            None => Err("that debug session is no longer live".to_string()),
        }
    }

    /// The frame the frontend means, as the adapter's id.
    pub(crate) fn frame_id(&self, index: usize) -> Result<i64, String> {
        let stopped = self.stopped.lock().unwrap_or_else(|p| p.into_inner());
        stopped
            .frames
            .get(index)
            .map(|f| f.id)
            .ok_or_else(|| "the program is not stopped there".to_string())
    }

    fn emit_status(&self, status: &str, message: &str) {
        let payload = DebugStatus {
            session_id: self.id.clone(),
            status: status.to_string(),
            vm: self.spec.label.to_string(),
            engine: "native".to_string(),
            // The one thing worth saying about this session for as long as it lives, and only when
            // there is something to say. It rides on every status because the panel may be opened
            // after the launch that would have carried it once.
            note: self.rendering.caveat().unwrap_or_default().to_string(),
            message: message.to_string(),
        };
        self.sink.emit(EVT_DEBUG_STATUS, serde_json::to_value(payload).unwrap_or(json!({})));
    }
}

/// A DAP frame as Bennu's own.
///
/// The one place the two protocols genuinely differ. A JDWP frame has a declaring class and a method;
/// a DAP frame has **one name** — `geode::mine::dig`, or
/// `core::ops::function::FnOnce::call_once{{vtable.shim}}` for a synthetic one. Splitting that at the
/// last `::` to invent a class produces nonsense on exactly the frames worth reading, so `name` carries
/// it whole and `class` is left empty. The panel prefers `name` when it is there.
fn frame_dto(index: u32, frame: &StackFrame, root: &str) -> FrameDto {
    let file = frame.source.as_ref().and_then(|s| s.path.clone());
    // Whether it is this project's own code, which is what the panel dims a library frame by. A path
    // prefix, because that is all either protocol can honestly answer.
    let project = file.as_deref().is_some_and(|f| {
        let f = f.replace('\\', "/");
        let root = root.replace('\\', "/");
        f.starts_with(&root) && !f.contains("/.cargo/") && !f.contains("/rustlib/")
    });
    FrameDto {
        index,
        class: String::new(),
        name: frame.name.clone(),
        method: frame.name.clone(),
        line: (frame.line > 0).then_some(frame.line),
        file,
        project,
    }
}

/// A DAP variable as Bennu's own.
///
/// The adapter's rendered `value` is used verbatim where there is one: it knows how to print a
/// `Vec<T>` — given the formatters, see [`bennu_dap::prelude::rendering`] — and a second opinion here
/// would be a worse one. What is added is the **handle**, tied to this stop.
///
/// The one substitution is for a value the adapter rendered as *nothing at all*, which LLDB does for
/// any struct it has no summary for. A blank cell beside a variable name reads as "empty", so the row
/// says how many things are inside instead — the honest version of the same non-answer, and an
/// invitation to expand it.
pub(crate) fn value_dto(session: &DapSession, variable: &Variable, kind: &str) -> DebugValue {
    let mut value = variable.value.clone();
    if value.trim().is_empty() {
        let inside =
            variable.named_variables.unwrap_or(0) + variable.indexed_variables.unwrap_or(0);
        value = match (inside, variable.variables_reference) {
            (0, 0) => String::new(),
            (0, _) => "{…}".to_string(),
            (1, _) => "{1 field}".to_string(),
            (n, _) => format!("{{{n} fields}}"),
        };
    }
    DebugValue {
        name: variable.name.clone(),
        kind: kind.to_string(),
        type_name: variable.type_name.clone().unwrap_or_default(),
        value,
        object: session.handle(variable.variables_reference),
    }
}

/// The scope's `presentationHint`, or its name, as the row kind the panel colours by.
fn scope_kind(scope: &Scope) -> &'static str {
    let hint = scope.presentation_hint.as_deref().unwrap_or("");
    let name = scope.name.to_ascii_lowercase();
    if hint == "arguments" || name.contains("argument") || name.contains("param") {
        return "argument";
    }
    if hint == "registers" || name.contains("register") {
        return "static";
    }
    "local"
}

impl DebugBackend for DapSession {
    fn kind(&self) -> &'static str {
        "dap"
    }

    fn describe(&self) -> String {
        self.spec.label.to_string()
    }

    fn root(&self) -> String {
        self.root.clone()
    }

    fn resume(&self) -> Result<(), String> {
        let thread = self.stopped.lock().unwrap_or_else(|p| p.into_inner()).thread;
        // Bumped BEFORE the request: the adapter may report the next stop before this returns, and a
        // handle minted for that stop must not carry the old generation.
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.stopped.lock().unwrap_or_else(|p| p.into_inner()).frames.clear();
        self.with(|s| s.resume(thread))?;
        self.emit_status("running", "");
        Ok(())
    }

    fn step(&self, step: Step) -> Result<(), String> {
        let thread = self.stopped.lock().unwrap_or_else(|p| p.into_inner()).thread;
        if thread == 0 {
            return Err("the program is not stopped".to_string());
        }
        let depth = match step {
            Step::Into => StepDepth::Into,
            Step::Out => StepDepth::Out,
            Step::Over => StepDepth::Over,
        };
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.with(|s| s.step(thread, depth))
    }

    /// Muting, DAP's way: uninstall every file's breakpoints and remember them.
    ///
    /// The adapter has no notion of a muted breakpoint, so the effect is reproduced — an empty list per
    /// file removes them, and unmuting sends the configured set back. The *model* is untouched either
    /// way, which is what makes this the same feature the JDWP session offers rather than a
    /// near-miss: the twelve breakpoints you will want back in a minute are still listed.
    fn set_muted(&self, muted: bool) -> Result<(), String> {
        let config = self.config.lock().unwrap_or_else(|p| p.into_inner()).clone();
        self.with(|session| {
            for (file, breakpoints) in by_file(&config) {
                let wanted = if muted { Vec::new() } else { breakpoints };
                // Per file, best-effort: one file the adapter refuses must not stop the rest from
                // being muted, or the gesture half-applies.
                let _ = session.set_breakpoints(&file, &wanted);
            }
            Ok(())
        })?;
        self.muted.store(muted, Ordering::SeqCst);
        Ok(())
    }

    fn detach(&self) -> Result<(), String> {
        // `stop()` disconnects, and the client's shutdown asks for the debuggee to be left alone where
        // the adapter honours it. Stopping the program is the Run console's Stop.
        if let Some(session) = self.session.lock().unwrap_or_else(|p| p.into_inner()).take() {
            session.stop();
        }
        self.emit_status("terminated", "detached");
        Ok(())
    }

    fn variables(&self, frame: usize) -> Result<Vec<DebugValue>, String> {
        let frame_id = self.frame_id(frame)?;
        let scopes = self.with(|s| s.scopes(frame_id))?;
        let mut out = Vec::new();
        for scope in &scopes {
            // A scope the adapter marked expensive — registers, usually — is listed as a row rather
            // than opened: fetching it costs a round trip nobody asked for.
            if scope.expensive {
                out.push(DebugValue {
                    name: scope.name.clone(),
                    kind: "static".to_string(),
                    type_name: String::new(),
                    value: String::new(),
                    object: self.handle(scope.variables_reference),
                });
                continue;
            }
            let kind = scope_kind(scope);
            let variables =
                self.with(|s| s.variables(scope.variables_reference, Some(MAX_ELEMENTS)))?;
            out.extend(variables.iter().map(|v| value_dto(self, v, kind)));
        }
        Ok(out)
    }

    fn expand(&self, handle: &str) -> Result<Vec<DebugValue>, String> {
        let reference = self.parse_handle(handle)?;
        let variables = self.with(|s| s.variables(reference, Some(MAX_ELEMENTS)))?;
        // A child's kind is not a scope's: it is a field of something, or an element of something.
        // Which of the two the adapter says by naming an index rather than an identifier.
        Ok(variables
            .iter()
            .map(|v| {
                let kind = if v.name.starts_with('[') { "element" } else { "field" };
                value_dto(self, v, kind)
            })
            .collect())
    }

    /// A watch. Which of three things happens to it is [`crate::debug_expr`]'s decision — a path is
    /// walked here over the same variables tree the panel shows, anything else goes to the adapter's
    /// evaluator, and what Rust cannot evaluate at all is said rather than forwarded.
    fn watch(&self, frame: usize, expression: &str) -> Result<DebugValue, String> {
        crate::debug_expr::watch(self, frame, expression)
    }

    fn reinstall(&self, config: &DebugConfig) -> Result<(), String> {
        *self.config.lock().unwrap_or_else(|p| p.into_inner()) = config.clone();
        if self.muted.load(Ordering::SeqCst) {
            // Muted: the model changed, what is installed does not. Unmuting will send the new set.
            return Ok(());
        }
        self.with(|session| {
            for (file, breakpoints) in by_file(config) {
                let _ = session.set_breakpoints(&file, &breakpoints);
            }
            Ok(())
        })
    }
}

/// The configured breakpoints, grouped by file, in the shape `setBreakpoints` takes.
///
/// Grouped because DAP replaces **a file's whole set** in one request: sending them one at a time would
/// have each request delete the one before it, which is a debugger where only the last breakpoint in
/// each file works.
fn by_file(config: &DebugConfig) -> Vec<(String, Vec<SourceBreakpoint>)> {
    let mut grouped: std::collections::BTreeMap<String, Vec<SourceBreakpoint>> =
        std::collections::BTreeMap::new();
    for bp in &config.breakpoints {
        if !bp.enabled {
            continue;
        }
        grouped.entry(bp.file.replace('\\', "/")).or_default().push(SourceBreakpoint {
            line: bp.line,
            // Sent as the adapter's OWN expression, not translated. Bennu's condition grammar
            // exists because JDWP has no evaluator and one had to be written; an adapter already
            // has a real one, its docs describe it, and reimplementing a subset here would be a
            // strictly worse language that also disagreed with everything the user has read.
            condition: (!bp.condition.trim().is_empty()).then(|| bp.condition.clone()),
            // `%N` is CodeLLDB's "every Nth hit". DAP does not define the syntax — the spec says
            // the adapter interprets `hitCondition` as it sees fit — so this is a best effort that
            // an adapter reading it differently will get differently, and the docs say so rather
            // than promising the two engines behave alike.
            //
            // `0` and `1` are every hit and are sent as nothing rather than as `%1`: expressing "no
            // restriction" must never be the thing that makes a breakpoint fail to install.
            hit_condition: (bp.hit_count > 1).then(|| format!("%{}", bp.hit_count)),
            ..SourceBreakpoint::default()
        });
    }
    grouped.into_iter().collect()
}

/// The session's side of the adapter's events: turn each into what the frontend already listens for.
struct Events {
    id: String,
    root: String,
    sink: Arc<dyn EventSink>,
    /// Set once the session exists — the handler is built first, so this is filled in after.
    session: Mutex<std::sync::Weak<DapSession>>,
}

impl SessionHandler for Events {
    fn on_stopped(&self, stopped: StoppedEvent) {
        let Some(session) = self.session.lock().unwrap_or_else(|p| p.into_inner()).upgrade() else {
            return;
        };
        // The thread the adapter named, or the first one it knows — `threadId` is optional in the spec.
        let thread = stopped.thread_id.or_else(|| session.with(|s| s.threads()).ok()?.first().map(|t| t.id));
        let Some(thread) = thread else { return };

        let frames = session.with(|s| s.stack(thread, MAX_FRAMES)).unwrap_or_default();
        {
            let mut guard = session.stopped.lock().unwrap_or_else(|p| p.into_inner());
            guard.thread = thread;
            guard.frames = frames.clone();
        }

        let dto: Vec<FrameDto> =
            frames.iter().enumerate().map(|(i, f)| frame_dto(i as u32, f, &self.root)).collect();

        // The panel's `reason` is one of three words it branches on, so the adapter's own vocabulary is
        // mapped onto it rather than passed through — `rust_panic` and `exception` and `signal` are all
        // "something was thrown", and an unknown reason reads as a step, which is the harmless one.
        let reason = match stopped.reason.as_str() {
            "breakpoint" | "function breakpoint" | "data breakpoint" | "instruction breakpoint" => {
                "breakpoint"
            }
            "exception" | "signal" | "panic" | "rust_panic" | "assert" => "exception",
            _ => "step",
        };
        // The prose the adapter gave, which for a panic is the panic message — the most useful line on
        // the screen when there is one. It rides in `exception`, which is where the panel shows it.
        let detail = stopped
            .description
            .clone()
            .or_else(|| stopped.text.clone())
            .filter(|d| !d.trim().is_empty());

        let thread_name = session
            .with(|s| s.threads())
            .ok()
            .and_then(|threads| threads.into_iter().find(|t| t.id == thread).map(|t| t.name))
            .unwrap_or_else(|| format!("thread {thread}"));

        let payload = DebugPause {
            session_id: self.id.clone(),
            thread: thread.to_string(),
            thread_name,
            reason: reason.to_string(),
            exception: detail.clone(),
            frames: dto,
        };
        self.sink.emit(EVT_DEBUG_PAUSED, serde_json::to_value(payload).unwrap_or(json!({})));
        session.emit_status("paused", &detail.unwrap_or_default());
    }

    fn on_continued(&self) {
        if let Some(session) = self.session.lock().unwrap_or_else(|p| p.into_inner()).upgrade() {
            session.generation.fetch_add(1, Ordering::SeqCst);
            session.stopped.lock().unwrap_or_else(|p| p.into_inner()).frames.clear();
            session.emit_status("running", "");
        }
    }

    fn on_output(&self, category: &str, text: &str) {
        // The Run console's own stream, which is where a program's output belongs — the debugger has no
        // second console, and a `println!` under the debugger must land in the same tab it lands in
        // without one. `stderr` is marked so the console can colour it, exactly as a plain run's is.
        // The field names are the console's contract, not ours: `text`, and a `stream` of `stderr` is
        // what it colours by. Getting either wrong shows a tab of blank lines.
        self.sink.emit(
            EVT_RUN_OUTPUT,
            json!({
                "run_id": self.id,
                "stream": if category == "stderr" { "stderr" } else { "stdout" },
                "text": text.trim_end_matches(['\n', '\r']),
            }),
        );
    }

    fn on_breakpoint(&self, breakpoint: Breakpoint) {
        // The gutter's answer to "did it bind" — the same broadcast the JVM session sends, so the dot
        // goes solid or stays hollow by the same path. An adapter may MOVE a breakpoint to the next
        // line that has code, and reporting the line it really bound at is what stops the gutter from
        // drawing it where it was clicked instead.
        let Some(session) = self.session.lock().unwrap_or_else(|p| p.into_inner()).upgrade() else {
            return;
        };
        let Some(file) = breakpoint.source.and_then(|s| s.path) else { return };
        let Some(line) = breakpoint.line else { return };
        let status = BreakpointStatus {
            file,
            line,
            verified: breakpoint.verified,
            message: breakpoint.message.unwrap_or_default(),
            // A native condition is the adapter's own expression, evaluated inside the adapter —
            // so a bad one comes back as a refusal to verify, in `message`, and there is no
            // separate answer for Bennu to report here.
            condition_error: String::new(),
        };
        self.sink.emit(
            EVT_DEBUG_BREAKPOINTS,
            json!({ "session_id": self.id, "root": session.root, "breakpoints": [status] }),
        );
    }

    fn on_terminated(&self, code: Option<i64>, reason: &str) {
        crate::debug_backend::remove(&self.id);
        // The console tab is keyed by the same id, and without this it stays "live" forever with a
        // spinner on a program that has already exited.
        self.sink.emit(EVT_RUN_EXIT, json!({ "run_id": self.id, "code": code }));
        if let Some(session) = self.session.lock().unwrap_or_else(|p| p.into_inner()).upgrade() {
            if let Some(code) = code {
                session.exit.store(code, Ordering::SeqCst);
            }
            let message = match code {
                Some(0) => reason.to_string(),
                Some(code) => format!("{reason} (exit code {code})"),
                None => reason.to_string(),
            };
            session.emit_status("terminated", &message);
        }
    }
}

/// Start a DAP session on `program`, and register it.
///
/// Returns the failure as a string the user reads — an adapter that would not start carries its own
/// stderr in it, because that is the only place the reason is written down.
#[allow(clippy::too_many_arguments)]
pub fn start(
    id: String,
    root: String,
    program: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    stop_on_entry: bool,
    pinned_adapter: Option<&str>,
    adapter_path: Option<&str>,
    sink: Arc<dyn EventSink>,
) -> Result<Arc<DapSession>, String> {
    let adapter = bennu_dap::prelude::resolve(pinned_adapter, adapter_path).ok_or_else(|| {
        match pinned_adapter {
            Some(id) => format!(
                "the `{id}` debug adapter is not installed. Bennu can also use CodeLLDB, lldb-dap or GDB 14+ — install one, or point Bennu at it in Settings."
            ),
            None => "no debug adapter found. Install CodeLLDB (the VS Code Rust extension's, and the one that renders Rust's own types), lldb-dap, or GDB 14 or newer.".to_string(),
        }
    })?;

    let config: DebugConfig = crate::repo_config::load(&root, "debug");
    let handler = Arc::new(Events {
        id: id.clone(),
        root: root.clone(),
        sink: Arc::clone(&sink),
        session: Mutex::new(std::sync::Weak::new()),
    });

    let launch = Launch::Program {
        program,
        args,
        cwd: root.clone(),
        env,
        stop_on_entry,
    };
    let breakpoints = by_file(&config);
    let filters = exception_filters(&config);

    let session = Session::start(
        &adapter,
        &root,
        launch,
        &breakpoints,
        &filters,
        Arc::clone(&handler) as Arc<dyn SessionHandler>,
    )
    .map_err(|e| e.to_string())?;

    let rendering = adapter.rendering();
    let dap = Arc::new(DapSession {
        id: id.clone(),
        root,
        spec: adapter.spec,
        rendering,
        session: Mutex::new(Some(session)),
        sink,
        generation: AtomicU64::new(1),
        stopped: Mutex::new(Stopped::default()),
        config: Mutex::new(config),
        muted: std::sync::atomic::AtomicBool::new(false),
        exit: AtomicI64::new(0),
    });
    // Closes the loop: the handler was built before the session existed, so it holds a weak reference
    // filled in now. Weak, so a finished session is dropped even though the adapter's reader thread
    // still holds the handler.
    *handler.session.lock().unwrap_or_else(|p| p.into_inner()) = Arc::downgrade(&dap);

    crate::debug_backend::insert(&id, Arc::clone(&dap) as Arc<dyn DebugBackend>);
    dap.emit_status("running", "");
    Ok(dap)
}

/// Which exception categories to ask the adapter to stop on.
///
/// Rust's are not Java's: there is no caught/uncaught throwable, there is `rust_panic`. So a configured
/// exception breakpoint with an empty class — "any throw" — becomes the adapter's panic filter, and a
/// named Java class means nothing here and is dropped rather than sent as a filter no adapter has.
fn exception_filters(config: &DebugConfig) -> Vec<String> {
    let wants_any = config
        .exceptions
        .iter()
        .any(|e| e.enabled && e.class.trim().is_empty() && (e.uncaught || e.caught));
    if wants_any {
        vec!["rust_panic".to_string()]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bennu_proto::prelude::{Breakpoint as BpConfig, ExceptionBreakpoint};

    fn config(breakpoints: Vec<BpConfig>) -> DebugConfig {
        DebugConfig { breakpoints, exceptions: Vec::new(), watches: Vec::new() }
    }

    fn bp(file: &str, line: u32, enabled: bool) -> BpConfig {
        BpConfig { file: file.to_string(), line, enabled, ..BpConfig::default() }
    }

    /// DAP replaces a file's WHOLE set per request, so they have to be grouped: one request per
    /// breakpoint would have each delete the one before it, leaving only the last in each file.
    #[test]
    fn breakpoints_are_grouped_by_file() {
        let grouped = by_file(&config(vec![
            bp("/p/src/main.rs", 10, true),
            bp("/p/src/lib.rs", 4, true),
            bp("/p/src/main.rs", 22, true),
        ]));
        assert_eq!(grouped.len(), 2, "two files, not three requests");
        let main = grouped.iter().find(|(f, _)| f.ends_with("main.rs")).unwrap();
        assert_eq!(main.1.len(), 2);
        assert_eq!(main.1.iter().map(|b| b.line).collect::<Vec<_>>(), vec![10, 22]);
    }

    #[test]
    fn a_disabled_breakpoint_is_remembered_but_not_installed() {
        let grouped = by_file(&config(vec![bp("/p/src/main.rs", 10, false)]));
        assert!(grouped.is_empty(), "disabled means not sent to the adapter");
    }

    #[test]
    fn windows_separators_are_normalised_before_they_reach_the_adapter() {
        let grouped = by_file(&config(vec![bp(r"C:\p\src\main.rs", 3, true)]));
        assert_eq!(grouped[0].0, "C:/p/src/main.rs");
    }

    /// A condition goes to the adapter verbatim — it is the adapter's language, not Bennu's — and
    /// an empty one is **omitted** rather than sent as `""`, which some adapters read as an
    /// expression that never holds.
    #[test]
    fn a_condition_reaches_the_adapter_unchanged_and_an_empty_one_is_not_sent() {
        let mut with = bp("/p/src/main.rs", 10, true);
        with.condition = "i > 5".to_string();
        let grouped = by_file(&config(vec![with, bp("/p/src/main.rs", 20, true)]));
        let lines = &grouped[0].1;
        assert_eq!(lines[0].condition.as_deref(), Some("i > 5"));
        assert_eq!(lines[1].condition, None);
    }

    /// Rust has no caught/uncaught throwable. "Any throw" becomes the adapter's panic filter; a named
    /// Java class means nothing here and is dropped rather than sent as a filter no adapter has.
    #[test]
    fn any_throw_becomes_the_panic_filter_and_a_java_class_is_dropped() {
        let mut cfg = config(Vec::new());
        cfg.exceptions.push(ExceptionBreakpoint {
            class: String::new(),
            caught: false,
            uncaught: true,
            enabled: true,
        });
        assert_eq!(exception_filters(&cfg), vec!["rust_panic".to_string()]);

        let mut named = config(Vec::new());
        named.exceptions.push(ExceptionBreakpoint {
            class: "java.lang.IllegalStateException".into(),
            caught: true,
            uncaught: true,
            enabled: true,
        });
        assert!(exception_filters(&named).is_empty());

        let mut disabled = config(Vec::new());
        disabled.exceptions.push(ExceptionBreakpoint {
            class: String::new(),
            caught: true,
            uncaught: true,
            enabled: false,
        });
        assert!(exception_filters(&disabled).is_empty());
    }

    /// The frame name is carried whole. Splitting it at the last `::` to invent a class produces
    /// nonsense on exactly the frames worth reading.
    #[test]
    fn a_frame_keeps_its_whole_name_and_invents_no_class() {
        let frame = StackFrame {
            id: 7,
            name: "core::ops::function::FnOnce::call_once{{vtable.shim}}".into(),
            source: None,
            line: 0,
            column: 0,
            presentation_hint: None,
        };
        let dto = frame_dto(3, &frame, "/p");
        assert_eq!(dto.index, 3);
        assert_eq!(dto.name, "core::ops::function::FnOnce::call_once{{vtable.shim}}");
        assert!(dto.class.is_empty(), "there is no class to be had, so none is invented");
        assert_eq!(dto.line, None, "line 0 means the adapter could not place it");
        assert!(!dto.project, "a frame with no source is not this project's");
    }

    #[test]
    fn a_frame_in_the_project_is_marked_and_a_dependency_is_not() {
        let mine = StackFrame {
            id: 1,
            name: "geode::mine::dig".into(),
            source: Some(Source { path: Some("/p/src/mine.rs".into()), ..Source::default() }),
            line: 42,
            column: 5,
            presentation_hint: None,
        };
        assert!(frame_dto(0, &mine, "/p").project);
        assert_eq!(frame_dto(0, &mine, "/p").line, Some(42));

        // A dependency's source sits under the cargo registry, inside no project.
        let theirs = StackFrame {
            source: Some(Source {
                path: Some("/home/u/.cargo/registry/src/x/lib.rs".into()),
                ..Source::default()
            }),
            ..mine.clone()
        };
        assert!(!frame_dto(0, &theirs, "/p").project);

        // …and so does the standard library, which is under a rustlib path.
        let std = StackFrame {
            source: Some(Source {
                path: Some("/p/toolchains/x/lib/rustlib/src/rust/library/core/src/option.rs".into()),
                ..Source::default()
            }),
            ..mine.clone()
        };
        assert!(!frame_dto(0, &std, "/p").project, "under the project root, but not its code");
    }

    #[test]
    fn a_scope_becomes_the_row_kind_the_panel_colours_by() {
        let scope = |name: &str, hint: Option<&str>| Scope {
            name: name.to_string(),
            variables_reference: 1,
            presentation_hint: hint.map(str::to_string),
            expensive: false,
        };
        assert_eq!(scope_kind(&scope("Locals", None)), "local");
        assert_eq!(scope_kind(&scope("Arguments", None)), "argument");
        assert_eq!(scope_kind(&scope("whatever", Some("arguments"))), "argument");
        assert_eq!(scope_kind(&scope("Registers", None)), "static");
        // An adapter's own scope name falls back to a local rather than to nothing.
        assert_eq!(scope_kind(&scope("Statics", None)), "local");
    }
}
