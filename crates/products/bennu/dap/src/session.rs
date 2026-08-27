//! The handshake, and the operations a debugger panel needs.
//!
//! [`crate::client`] moves messages; this decides what they mean. Still no policy about what a
//! breakpoint means to a project — that is the host's — but the protocol's own choreography lives
//! here, because getting it wrong is how a session hangs with no error anywhere.
//!
//! ## The configuration handshake, and the event in the middle of it
//!
//! The order is fixed by the spec and it is not the obvious one:
//!
//! ```text
//! → initialize                 (and wait for the response: it carries the capabilities)
//! → launch  /  attach          (do NOT wait for its response yet — see below)
//! ← initialized                (an EVENT, and the signal that breakpoints may be set)
//! → setBreakpoints × files
//! → setExceptionBreakpoints
//! → configurationDone          (only if the adapter asked for it)
//! ← launch response            (arrives now, or arrived already)
//! ```
//!
//! Two traps in that:
//!
//! 1. **`initialized` is an event, not the response to `initialize`.** It can arrive before the
//!    `launch` response, after it, or between two other messages. So it is waited for on a flag that
//!    is armed *before* `launch` is sent — a client that starts waiting afterwards can miss it and
//!    then waits forever, which presents as a debugger that never binds a breakpoint.
//!
//! 2. **`configurationDone` is conditional.** Sending it to an adapter that did not advertise
//!    `supportsConfigurationDoneRequest` is an error on several; *not* sending it to one that did
//!    leaves the debuggee suspended before `main` with nothing to release it. The second failure looks
//!    exactly like a hang, so the capability is checked rather than assumed either way.
//!
//! ## Handles are only valid while the debuggee is stopped
//!
//! A `frameId` and a `variablesReference` mean nothing after a `continue`, and an adapter is entitled
//! to reuse the numbers. So [`Session`] drops every handle it holds the moment it sees `continued` —
//! the same discipline the JDWP session keeps for its frame ids, and for the same reason: a stale
//! handle does not error, it answers about something else.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use crate::client::{AdapterHandler, DapClient, DapError};
use crate::discovery::AdapterSpec;
use crate::protocol::{Event, Response};
use crate::types::*;

/// How long to wait for a request that only talks to the adapter.
///
/// Generous because `launch` is the outlier: it loads a binary, and a large one with full debug info
/// takes seconds before the adapter answers anything.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// How long to wait for the `initialized` event.
///
/// Bounded so a wedged adapter is reported rather than hung on. An adapter that has not said it is
/// ready in this long is one that never will.
pub const INITIALIZED_TIMEOUT: Duration = Duration::from_secs(20);

/// What the host is told about a live session. Events the host must react to, in its own vocabulary.
///
/// Deliberately narrower than DAP's event set: everything not here is either handled inside the
/// session or is nothing a debugger panel can act on.
pub trait SessionHandler: Send + Sync {
    /// The debuggee stopped, and where.
    fn on_stopped(&self, stopped: StoppedEvent);
    /// The debuggee is running again.
    fn on_continued(&self);
    /// The debuggee wrote something. `category` is `stdout` / `stderr` / `console`.
    fn on_output(&self, category: &str, text: &str);
    /// A breakpoint bound, moved, or was lost, after the fact.
    fn on_breakpoint(&self, breakpoint: Breakpoint);
    /// The session is over. `code` is the debuggee's exit status when the adapter reported one.
    fn on_terminated(&self, code: Option<i64>, reason: &str);
}

/// The state the reader thread writes and the session reads.
///
/// Separate from [`Session`] because the client's handler is built **before** the client exists and
/// therefore before the session does — which is exactly what makes the `initialized` race avoidable:
/// the flag is armed at construction, so it cannot be missed.
struct Shared {
    handler: Arc<dyn SessionHandler>,
    /// Set when `initialized` arrives; the condvar wakes whoever is waiting for it.
    initialized: Mutex<bool>,
    ready: Condvar,
    /// The thread the debuggee is stopped in, or 0. Written on `stopped`, cleared on `continued`.
    stopped_thread: AtomicI64,
    /// Whether the debuggee is stopped at all.
    stopped: AtomicBool,
    /// The exit code, when `exited` arrived before `terminated` — which is the usual order, and the
    /// only place the code is reported.
    exit_code: Mutex<Option<i64>>,
}

/// The client-side handler: hands each event to the session's worker, in order.
///
/// ## Why a worker, and not just doing the work here
///
/// [`AdapterHandler::on_event`] is called **inline on the reader thread**, and the reader is the only
/// thing that can deliver a response. So a handler that makes a request from here is waiting for
/// something only the thread it is blocking can produce: a deadlock, resolved by the request's timeout
/// into an *empty answer*. Which is worse than a hang, because it looks like data — a `stopped` handler
/// that asks for the stack frames gets none, and the panel says "paused" with nothing to focus.
///
/// Answering a `stopped` event **requires** requests: the thread list, and the stack trace. So the
/// work goes to one worker thread per session, fed by a channel.
///
/// **One** worker, not a thread per event, and that is the load-bearing part: the channel preserves
/// order, so a `continued` cannot overtake the `stopped` before it and leave the panel believing the
/// program is still standing still.
///
/// `initialized` is handled inline, because it needs no request and the startup handshake is waiting
/// on it — putting it behind the channel would add a scheduling hop to every launch.
struct Events {
    shared: Arc<Shared>,
    /// The worker's end. Dropping this ends the worker, which is what a finished session does.
    tx: Mutex<Option<std::sync::mpsc::Sender<Event>>>,
}

impl AdapterHandler for Events {
    fn on_event(&self, event: Event) {
        if event.event == "initialized" {
            let mut guard = self.shared.initialized.lock().unwrap_or_else(|p| p.into_inner());
            *guard = true;
            self.shared.ready.notify_all();
            return;
        }
        // In order, and off this thread. A closed channel means the session is finished and the event
        // has nowhere to go, which is not an error.
        let guard = self.tx.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(tx) = guard.as_ref() {
            let _ = tx.send(event);
        }
    }

    fn on_exit(&self, reason: &str) {
        let code = *self.shared.exit_code.lock().unwrap_or_else(|p| p.into_inner());
        self.shared.stopped.store(false, Ordering::SeqCst);
        self.shared.handler.on_terminated(code, reason);
    }
}

impl Shared {
    /// One event, handled on the worker — where making a request is safe.
    fn handle(&self, event: Event) {
        match event.event.as_str() {
            "stopped" => {
                let Ok(body) = event.parse::<StoppedEvent>() else { return };
                // An adapter is allowed to omit the thread, meaning "all of them". Recording 0 lets
                // the session fall back to the first thread it can find rather than asking for a
                // stack trace of nothing.
                self.stopped_thread.store(body.thread_id.unwrap_or(0), Ordering::SeqCst);
                self.stopped.store(true, Ordering::SeqCst);
                self.handler.on_stopped(body);
            }
            "continued" => {
                self.stopped.store(false, Ordering::SeqCst);
                self.stopped_thread.store(0, Ordering::SeqCst);
                self.handler.on_continued();
            }
            "output" => {
                let Ok(body) = event.parse::<OutputEvent>() else { return };
                let category = body.category.unwrap_or_else(|| "console".to_string());
                // The adapter talking to its vendor, not to the user.
                if category == "telemetry" {
                    return;
                }
                self.handler.on_output(&category, &body.output);
            }
            "breakpoint" => {
                let Ok(body) = event.parse::<BreakpointEvent>() else { return };
                self.handler.on_breakpoint(body.breakpoint);
            }
            "exited" => {
                if let Ok(body) = event.parse::<ExitedEvent>() {
                    *self.exit_code.lock().unwrap_or_else(|p| p.into_inner()) =
                        Some(body.exit_code);
                }
            }
            "terminated" => {
                let code = *self.exit_code.lock().unwrap_or_else(|p| p.into_inner());
                self.stopped.store(false, Ordering::SeqCst);
                self.handler.on_terminated(code, "the program ended");
            }
            // `thread`, `module`, `loadedSource`, `progress*`, `capabilities`, and whatever an adapter
            // invents. Nothing a panel can act on today; dropped rather than guessed at.
            _ => {}
        }
    }
}

/// How the debuggee is started.
#[derive(Debug, Clone)]
pub enum Launch {
    /// Run this binary under the debugger.
    Program {
        /// Absolute path to the executable.
        program: String,
        args: Vec<String>,
        cwd: String,
        /// Extra environment, on top of the adapter's own.
        env: Vec<(String, String)>,
        /// Whether to stop before the first line of `main`. Off by default: the useful stop is the
        /// first breakpoint, and stopping at entry on every launch is a keystroke of ceremony.
        stop_on_entry: bool,
    },
    /// Attach to something already running.
    Pid(u32),
}

impl Launch {
    /// The request name and its arguments.
    ///
    /// The argument set is the **union** of what the three adapters read, which is safe because each
    /// ignores what it does not know: `codelldb` and `lldb-dap` take `program`/`args`/`cwd`/`env`,
    /// GDB's DAP mode takes the same names. `env` is sent as an object because that is what all three
    /// accept — `codelldb` also accepts a list of `K=V`, and the object form is the portable one.
    ///
    /// On top of that go the arguments that are about the **adapter** rather than about the program —
    /// importing Rust's formatters into a plain LLDB, above all. They come from
    /// [`crate::rendering::launch_extras`], and they are what stands between a `Vec<Order>` shown as
    /// its elements and the same `Vec` shown as a pointer and a length. They are merged for `attach`
    /// too: an attached session renders values through the same formatters.
    fn request(&self, spec: &AdapterSpec) -> (&'static str, serde_json::Value) {
        let (command, mut arguments) = match self {
            Launch::Program { program, args, cwd, env, stop_on_entry } => {
                let env: serde_json::Map<String, serde_json::Value> = env
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                    .collect();
                (
                    "launch",
                    serde_json::json!({
                        "program": program,
                        "args": args,
                        "cwd": cwd,
                        "env": env,
                        "stopOnEntry": stop_on_entry,
                        // Without this a `println!` goes to the adapter's own stdout instead of
                        // arriving as an `output` event, and the Run console stays empty.
                        "terminal": "console",
                    }),
                )
            }
            Launch::Pid(pid) => ("attach", serde_json::json!({ "pid": pid })),
        };
        if let Some(map) = arguments.as_object_mut() {
            for (key, value) in crate::rendering::launch_extras(spec) {
                map.insert(key.to_string(), value);
            }
        }
        (command, arguments)
    }
}

/// One debug session: an adapter, configured and running.
pub struct Session {
    client: Arc<DapClient>,
    shared: Arc<Shared>,
    capabilities: Capabilities,
    /// Whether the adapter's `terminate` may be used — see [`DapClient::shutdown`].
    supports_terminate: bool,
}

impl Session {
    /// Start an adapter, hand it the launch configuration, and run the whole handshake.
    ///
    /// Returns once the debuggee is configured and running (or stopped at entry). Every failure on the
    /// way carries the adapter's stderr where there is any, because that is where an adapter that
    /// would not start says why.
    pub fn start(
        adapter: &crate::discovery::Adapter,
        cwd: &str,
        launch: Launch,
        breakpoints: &[(String, Vec<SourceBreakpoint>)],
        exception_filters: &[String],
        handler: Arc<dyn SessionHandler>,
    ) -> Result<Session, DapError> {
        let shared = Arc::new(Shared {
            handler,
            // Armed BEFORE the client exists, which is what makes the `initialized` event
            // unmissable — see the module docs.
            initialized: Mutex::new(false),
            ready: Condvar::new(),
            stopped_thread: AtomicI64::new(0),
            stopped: AtomicBool::new(false),
            exit_code: Mutex::new(None),
        });

        // The worker that handles every event but `initialized` — see [`Events`] for why it exists at
        // all. One thread, one channel, so the order the adapter sent them in is the order they are
        // handled in.
        let (tx, rx) = std::sync::mpsc::channel::<Event>();
        let worker_shared = Arc::clone(&shared);
        std::thread::Builder::new()
            .name(format!("dap-events-{}", adapter.spec.id))
            .spawn(move || {
                // Ends when the sender drops, which is when the session is dropped.
                for event in rx {
                    worker_shared.handle(event);
                }
            })
            .map_err(|e| DapError::Spawn(e.to_string()))?;

        let events = Arc::new(Events { shared: Arc::clone(&shared), tx: Mutex::new(Some(tx)) });
        let (exe, args) = adapter.command();
        let client = DapClient::spawn(adapter.spec.id, &exe, &args, cwd, events)?;

        // ── initialize ────────────────────────────────────────────────────────
        // The adapter's OWN id, which is what `adapterID` asks for — some adapters key their behaviour
        // on it, and a placeholder is the wrong answer to a question about them.
        let response = client.request(
            "initialize",
            Some(InitializeArguments::for_adapter(adapter.spec.id)),
            REQUEST_TIMEOUT,
        )?;
        if !response.success {
            let tail = client.stderr_tail();
            client.shutdown(false);
            return Err(DapError::Spawn(with_stderr(&response.error_text(), &tail)));
        }
        let capabilities: Capabilities = response.parse().map_err(DapError::Transport)?;

        // ── launch / attach, then wait for `initialized` ───────────────────────
        //
        // The launch response is deliberately not awaited here: several adapters hold it until
        // `configurationDone`, so waiting for it before sending that is a deadlock the spec warns
        // about. It is sent, and its answer is collected after the configuration sequence.
        let (command, arguments) = launch.request(adapter.spec);
        let launch_seq = client.request_async(command, Some(arguments))?;

        Self::await_initialized(&client, &shared)?;

        // ── the configuration sequence ────────────────────────────────────────
        for (file, breakpoints) in breakpoints {
            let args = SetBreakpointsArguments {
                source: Source { path: Some(file.clone()), ..Source::default() },
                breakpoints: allowed(breakpoints.clone(), &capabilities),
                source_modified: false,
            };
            // A file the adapter refuses is one file's breakpoints lost, not a failed session: a
            // stale path in the persisted set must not stop the program from running.
            let _ = client.request("setBreakpoints", Some(args), REQUEST_TIMEOUT);
        }
        if !exception_filters.is_empty() {
            let _ = client.request(
                "setExceptionBreakpoints",
                Some(serde_json::json!({ "filters": exception_filters })),
                REQUEST_TIMEOUT,
            );
        }
        if capabilities.supports_configuration_done_request {
            let _ = client.request("configurationDone", None::<()>, REQUEST_TIMEOUT);
        }

        // Now the launch may be collected. A failure here is the real one — "no such file", "not an
        // executable", "the binary has no debug info" — and it is what the user needs to see.
        let launched = client.await_response(launch_seq, REQUEST_TIMEOUT)?;
        if !launched.success {
            let tail = client.stderr_tail();
            client.shutdown(false);
            return Err(DapError::Spawn(with_stderr(&launched.error_text(), &tail)));
        }

        Ok(Session {
            client,
            shared,
            supports_terminate: capabilities.supports_terminate_request,
            capabilities,
        })
    }

    /// Block until the adapter says it is ready to be configured.
    fn await_initialized(client: &DapClient, shared: &Shared) -> Result<(), DapError> {
        let deadline = Instant::now() + INITIALIZED_TIMEOUT;
        let mut guard = shared.initialized.lock().unwrap_or_else(|p| p.into_inner());
        while !*guard {
            // The adapter dying while we wait is the common failure — a bad `program` path takes this
            // path — and without the check it would be a full timeout instead of an immediate answer.
            if !client.is_alive() {
                let tail = client.stderr_tail();
                return Err(DapError::Spawn(with_stderr(
                    "the debug adapter exited during startup",
                    &tail,
                )));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                let tail = client.stderr_tail();
                return Err(DapError::Spawn(with_stderr(
                    "the debug adapter never reported itself ready",
                    &tail,
                )));
            }
            // Woken periodically rather than only on the notify, so the liveness check above runs.
            let (next, _) = shared
                .ready
                .wait_timeout(guard, remaining.min(Duration::from_millis(250)))
                .unwrap_or_else(|p| p.into_inner());
            guard = next;
        }
        Ok(())
    }

    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }

    pub fn is_alive(&self) -> bool {
        self.client.is_alive()
    }

    pub fn is_stopped(&self) -> bool {
        self.shared.stopped.load(Ordering::SeqCst)
    }

    /// The thread the debuggee is stopped in.
    ///
    /// Falls back to the first thread the adapter reports when the `stopped` event named none, which
    /// is legal and which several adapters do when everything stopped at once.
    pub fn stopped_thread(&self) -> Option<i64> {
        match self.shared.stopped_thread.load(Ordering::SeqCst) {
            0 => self.threads().ok()?.first().map(|t| t.id),
            id => Some(id),
        }
    }

    // ── the operations ────────────────────────────────────────────────────────

    pub fn threads(&self) -> Result<Vec<Thread>, String> {
        let body: ThreadsBody = self.ask("threads", None::<()>)?;
        Ok(body.threads)
    }

    /// The stopped thread's stack, innermost frame first.
    ///
    /// `limit` caps it, because a runaway recursion has a stack of hundreds of thousands of frames and
    /// an adapter that did not claim `supportsDelayedStackTraceLoading` will send every one of them.
    pub fn stack(&self, thread_id: i64, limit: u32) -> Result<Vec<StackFrame>, String> {
        let args = StackTraceArguments { thread_id, start_frame: 0, levels: limit };
        let body: StackTraceBody = self.ask("stackTrace", Some(args))?;
        Ok(body.stack_frames)
    }

    pub fn scopes(&self, frame_id: i64) -> Result<Vec<Scope>, String> {
        let body: ScopesBody =
            self.ask("scopes", Some(serde_json::json!({ "frameId": frame_id })))?;
        Ok(body.scopes)
    }

    /// What is inside a handle. `limit` caps an indexed container.
    pub fn variables(&self, reference: i64, limit: Option<u32>) -> Result<Vec<Variable>, String> {
        let args = VariablesArguments {
            variables_reference: reference,
            count: limit,
            // `start` is only meaningful with a count, and sending one without the other makes two
            // adapters return nothing at all.
            start: limit.map(|_| 0),
            filter: None,
        };
        let body: VariablesBody = self.ask("variables", Some(args))?;
        Ok(body.variables)
    }

    /// One element of an indexed container, fetched **without** fetching the ones before it.
    ///
    /// The paging half of the `variables` request, and the only way to answer `v[400000]` on a `Vec`
    /// of a million: the plain call is capped, and the cap is not a bug — an adapter asked for a
    /// million children will send a million children. `filter: "indexed"` is required with
    /// `start`/`count`, because otherwise the offset counts a struct's named fields too and lands on
    /// the wrong element.
    ///
    /// `None` means the container has nothing at that index.
    pub fn indexed_variable(&self, reference: i64, at: u32) -> Result<Option<Variable>, String> {
        let args = VariablesArguments {
            variables_reference: reference,
            filter: Some("indexed".to_string()),
            start: Some(at),
            count: Some(1),
        };
        let body: VariablesBody = self.ask("variables", Some(args))?;
        Ok(body.variables.into_iter().next())
    }

    /// Evaluate an expression in a frame. `context` is `watch` or `hover` — see [`EvaluateArguments`].
    pub fn evaluate(
        &self,
        expression: &str,
        frame_id: Option<i64>,
        context: &str,
    ) -> Result<EvaluateBody, String> {
        let args = EvaluateArguments {
            expression: expression.to_string(),
            frame_id,
            context: context.to_string(),
        };
        self.ask("evaluate", Some(args))
    }

    /// Replace one file's breakpoints; the answer says what the adapter made of each, in order.
    pub fn set_breakpoints(
        &self,
        file: &str,
        breakpoints: &[SourceBreakpoint],
    ) -> Result<Vec<Breakpoint>, String> {
        let args = SetBreakpointsArguments {
            source: Source { path: Some(file.to_string()), ..Source::default() },
            breakpoints: allowed(breakpoints.to_vec(), &self.capabilities),
            source_modified: false,
        };
        let body: SetBreakpointsBody = self.ask("setBreakpoints", Some(args))?;
        Ok(body.breakpoints)
    }

    pub fn set_exception_filters(&self, filters: &[String]) -> Result<(), String> {
        self.ask_unit("setExceptionBreakpoints", Some(serde_json::json!({ "filters": filters })))
    }

    pub fn resume(&self, thread_id: i64) -> Result<(), String> {
        self.ask_unit("continue", Some(serde_json::json!({ "threadId": thread_id })))
    }

    pub fn pause(&self, thread_id: i64) -> Result<(), String> {
        self.ask_unit("pause", Some(serde_json::json!({ "threadId": thread_id })))
    }

    /// One step. `over` / `into` / `out` — the three a panel offers.
    pub fn step(&self, thread_id: i64, depth: StepDepth) -> Result<(), String> {
        let command = match depth {
            StepDepth::Over => "next",
            StepDepth::Into => "stepIn",
            StepDepth::Out => "stepOut",
        };
        self.ask_unit(command, Some(serde_json::json!({ "threadId": thread_id })))
    }

    /// End the session. The debuggee goes with it.
    pub fn stop(&self) {
        self.client.shutdown(self.supports_terminate);
    }

    /// The tail of the adapter's stderr — what a failure report needs.
    pub fn stderr_tail(&self) -> Vec<String> {
        self.client.stderr_tail()
    }

    // ── the two shapes every operation takes ──────────────────────────────────

    /// Ask, and parse the body. A refusal becomes the adapter's own words.
    fn ask<A: serde::Serialize, T: serde::de::DeserializeOwned + Default>(
        &self,
        command: &str,
        arguments: Option<A>,
    ) -> Result<T, String> {
        let response = self.request(command, arguments)?;
        response.parse()
    }

    /// Ask, and care only whether it worked.
    fn ask_unit<A: serde::Serialize>(
        &self,
        command: &str,
        arguments: Option<A>,
    ) -> Result<(), String> {
        self.request(command, arguments).map(|_| ())
    }

    fn request<A: serde::Serialize>(
        &self,
        command: &str,
        arguments: Option<A>,
    ) -> Result<Response, String> {
        let response = self
            .client
            .request(command, arguments, REQUEST_TIMEOUT)
            .map_err(|e| e.to_string())?;
        if !response.success {
            return Err(response.error_text());
        }
        Ok(response)
    }
}

/// Which way a step goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepDepth {
    Over,
    Into,
    Out,
}

impl StepDepth {
    /// Parse the wire word Bennu's own debug contract uses, so the DAP and JDWP sessions take the
    /// same argument from the same handler.
    pub fn parse(s: &str) -> Option<StepDepth> {
        match s {
            "over" | "line" => Some(StepDepth::Over),
            "into" | "in" => Some(StepDepth::Into),
            "out" => Some(StepDepth::Out),
            _ => None,
        }
    }
}

/// A message with the adapter's own log after it, when there is any.
///
/// The reason an adapter would not start is almost never in its protocol answer — `gdb` older than 14
/// prints a usage error and exits, `codelldb` writes a missing-library message — so a failure report
/// without the log is a failure report with the cause left out.
fn with_stderr(message: &str, tail: &[String]) -> String {
    let log: Vec<&String> = tail.iter().rev().take(8).collect();
    if log.is_empty() {
        return message.to_string();
    }
    let log: Vec<&str> = log.into_iter().rev().map(String::as_str).collect();
    format!("{message}\n{}", log.join("\n"))
}

/// Strip the breakpoint fields this adapter did not claim to support.
///
/// The spec says a client sends `condition` only when `supportsConditionalBreakpoints` is set, and
/// the reason is not pedantry: `setBreakpoints` replaces a **whole file's** set in one request, so an
/// adapter that errors on a field it does not know does not lose one breakpoint — it loses every
/// breakpoint in that file, and the program then runs straight past all of them.
///
/// Dropping the field rather than the breakpoint is the right half to give up. A breakpoint that
/// stops too often is one keystroke from being fixed; one that never binds is invisible.
fn allowed(mut breakpoints: Vec<SourceBreakpoint>, capabilities: &Capabilities) -> Vec<SourceBreakpoint> {
    if capabilities.supports_conditional_breakpoints
        && capabilities.supports_hit_conditional_breakpoints
    {
        return breakpoints;
    }
    for bp in &mut breakpoints {
        if !capabilities.supports_conditional_breakpoints {
            bp.condition = None;
        }
        if !capabilities.supports_hit_conditional_breakpoints {
            bp.hit_condition = None;
        }
    }
    breakpoints
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A breakpoint carrying both extras, for the capability filter below.
    fn restricted() -> SourceBreakpoint {
        SourceBreakpoint {
            line: 10,
            condition: Some("i > 5".into()),
            hit_condition: Some("%3".into()),
            ..SourceBreakpoint::default()
        }
    }

    #[test]
    fn an_adapter_that_supports_both_gets_both_unchanged() {
        let caps = Capabilities {
            supports_conditional_breakpoints: true,
            supports_hit_conditional_breakpoints: true,
            ..Capabilities::default()
        };
        let out = allowed(vec![restricted()], &caps);
        assert_eq!(out[0].condition.as_deref(), Some("i > 5"));
        assert_eq!(out[0].hit_condition.as_deref(), Some("%3"));
    }

    /// The failure this guards is not "the condition is ignored" — `setBreakpoints` replaces a whole
    /// file's set, so an adapter that errors on an unknown field loses EVERY breakpoint in that file.
    #[test]
    fn a_field_the_adapter_never_claimed_is_dropped_and_the_breakpoint_survives() {
        let out = allowed(vec![restricted()], &Capabilities::default());
        assert_eq!(out.len(), 1, "the breakpoint stays — only the field goes");
        assert_eq!(out[0].line, 10);
        assert_eq!(out[0].condition, None);
        assert_eq!(out[0].hit_condition, None);
    }

    #[test]
    fn the_two_capabilities_are_answered_separately() {
        let caps = Capabilities {
            supports_conditional_breakpoints: true,
            ..Capabilities::default()
        };
        let out = allowed(vec![restricted()], &caps);
        assert_eq!(out[0].condition.as_deref(), Some("i > 5"));
        assert_eq!(out[0].hit_condition, None, "a hit condition is a separate claim");
    }

    /// The bug this seam exists for, pinned.
    ///
    /// Answering a `stopped` event needs the thread list and the stack trace — both **requests**. Made
    /// from `on_event`, which the client calls inline on its reader thread, they block waiting for a
    /// response only that thread can deliver: a deadlock that resolves into an *empty* answer when the
    /// request times out. The panel then says "paused" with no frames and nothing to focus, which is
    /// worse than a hang because it looks like data.
    ///
    /// So this asserts what makes it safe: an event handler that blocks does **not** block the caller
    /// of `on_event`, and events are still handled in the order they arrived.
    #[test]
    fn an_event_handler_that_blocks_does_not_block_the_reader_and_order_is_kept() {
        use std::sync::mpsc;
        use std::time::Instant;

        struct Slow {
            seen: Mutex<Vec<String>>,
            done: mpsc::Sender<()>,
        }
        impl SessionHandler for Slow {
            fn on_stopped(&self, stopped: StoppedEvent) {
                // Stands in for the two requests the real handler makes.
                std::thread::sleep(Duration::from_millis(120));
                self.seen.lock().unwrap().push(format!("stopped:{}", stopped.reason));
            }
            fn on_continued(&self) {
                self.seen.lock().unwrap().push("continued".into());
                let _ = self.done.send(());
            }
            fn on_output(&self, _: &str, _: &str) {}
            fn on_breakpoint(&self, _: Breakpoint) {}
            fn on_terminated(&self, _: Option<i64>, _: &str) {}
        }

        let (done_tx, done_rx) = mpsc::channel();
        let handler = Arc::new(Slow { seen: Mutex::new(Vec::new()), done: done_tx });
        let shared = Arc::new(Shared {
            handler: Arc::clone(&handler) as Arc<dyn SessionHandler>,
            initialized: Mutex::new(false),
            ready: Condvar::new(),
            stopped_thread: AtomicI64::new(0),
            stopped: AtomicBool::new(false),
            exit_code: Mutex::new(None),
        });

        let (tx, rx) = mpsc::channel::<Event>();
        let worker = Arc::clone(&shared);
        std::thread::spawn(move || {
            for event in rx {
                worker.handle(event);
            }
        });
        let events = Events { shared: Arc::clone(&shared), tx: Mutex::new(Some(tx)) };

        let stopped = Event {
            event: "stopped".into(),
            body: Some(serde_json::json!({ "reason": "breakpoint", "threadId": 1 })),
        };
        let continued = Event {
            event: "continued".into(),
            body: Some(serde_json::json!({ "threadId": 1 })),
        };

        // Both handed over from "the reader thread". Neither call may wait for the slow handler.
        let before = Instant::now();
        events.on_event(stopped);
        events.on_event(continued);
        let handing_over = before.elapsed();
        assert!(
            handing_over < Duration::from_millis(60),
            "the reader was blocked for {handing_over:?} — this is the deadlock the worker prevents",
        );

        done_rx.recv_timeout(Duration::from_secs(5)).expect("the worker ran both");
        // In order: a `continued` that overtook its `stopped` would leave the panel believing the
        // program is still standing still.
        assert_eq!(handler.seen.lock().unwrap().as_slice(), &["stopped:breakpoint", "continued"]);
    }

    /// `initialized` stays inline, because the startup handshake is waiting on it and it needs no
    /// request to answer — going through the channel would add a scheduling hop to every launch.
    #[test]
    fn initialized_is_recorded_without_the_worker_running_at_all() {
        struct Nothing;
        impl SessionHandler for Nothing {
            fn on_stopped(&self, _: StoppedEvent) {}
            fn on_continued(&self) {}
            fn on_output(&self, _: &str, _: &str) {}
            fn on_breakpoint(&self, _: Breakpoint) {}
            fn on_terminated(&self, _: Option<i64>, _: &str) {}
        }
        let shared = Arc::new(Shared {
            handler: Arc::new(Nothing),
            initialized: Mutex::new(false),
            ready: Condvar::new(),
            stopped_thread: AtomicI64::new(0),
            stopped: AtomicBool::new(false),
            exit_code: Mutex::new(None),
        });
        // No worker, and no channel: the flag must still be set.
        let events = Events { shared: Arc::clone(&shared), tx: Mutex::new(None) };
        events.on_event(Event { event: "initialized".into(), body: None });
        assert!(*shared.initialized.lock().unwrap(), "the handshake would wait forever");
    }

    #[test]
    fn a_step_depth_parses_the_words_bennu_s_contract_uses() {
        // `line` is JDWP's word for the same thing, and both sessions are driven by one handler.
        assert_eq!(StepDepth::parse("over"), Some(StepDepth::Over));
        assert_eq!(StepDepth::parse("line"), Some(StepDepth::Over));
        assert_eq!(StepDepth::parse("into"), Some(StepDepth::Into));
        assert_eq!(StepDepth::parse("in"), Some(StepDepth::Into));
        assert_eq!(StepDepth::parse("out"), Some(StepDepth::Out));
        assert_eq!(StepDepth::parse("sideways"), None);
    }

    #[test]
    fn a_program_launch_carries_what_all_three_adapters_read() {
        let launch = Launch::Program {
            program: "/p/target/debug/app".into(),
            args: vec!["--flag".into()],
            cwd: "/p".into(),
            env: vec![("RUST_BACKTRACE".into(), "1".into())],
            stop_on_entry: false,
        };
        let spec = crate::discovery::spec_by_id("codelldb").unwrap();
        let (command, args) = launch.request(spec);
        assert_eq!(command, "launch");
        assert_eq!(args["program"], "/p/target/debug/app");
        assert_eq!(args["args"][0], "--flag");
        assert_eq!(args["cwd"], "/p");
        // An object, which is the form all three accept.
        assert_eq!(args["env"]["RUST_BACKTRACE"], "1");
        assert_eq!(args["stopOnEntry"], false);
        // Without this a `println!` never becomes an `output` event and the console stays empty.
        assert_eq!(args["terminal"], "console");
    }

    /// The adapter's own arguments ride on the same request — and they are not cosmetic: they are
    /// what decides whether a `Vec` shows its elements.
    #[test]
    fn a_launch_carries_the_adapters_rendering_arguments_too() {
        let launch = Launch::Program {
            program: "/p/app".into(),
            args: Vec::new(),
            cwd: "/p".into(),
            env: Vec::new(),
            stop_on_entry: false,
        };
        let codelldb = crate::discovery::spec_by_id("codelldb").unwrap();
        let (_, args) = launch.request(codelldb);
        assert_eq!(args["sourceLanguages"][0], "rust");
        assert_eq!(args["expressions"], "simple");

        // A plain LLDB gets the toolchain's formatters imported instead — when there is a toolchain.
        let lldb = crate::discovery::spec_by_id("lldb-dap").unwrap();
        let (_, args) = launch.request(lldb);
        assert_eq!(args["enableAutoVariableSummaries"], true);
        if crate::rendering::toolchain_etc().is_some() {
            let commands = args["initCommands"].as_array().expect("the import commands");
            assert_eq!(commands.len(), 2);
            assert!(commands[0].as_str().unwrap().contains("lldb_lookup.py"));
        }
    }

    #[test]
    fn attaching_is_a_different_request_entirely() {
        let spec = crate::discovery::spec_by_id("gdb").unwrap();
        let (command, args) = Launch::Pid(4321).request(spec);
        assert_eq!(command, "attach");
        assert_eq!(args["pid"], 4321);
    }

    #[test]
    fn a_failure_report_carries_the_adapters_own_log() {
        let tail: Vec<String> =
            (0..20).map(|i| format!("line {i}")).collect();
        let text = with_stderr("the debug adapter would not start", &tail);
        assert!(text.starts_with("the debug adapter would not start"));
        // The tail, in order, because the cause is at the end of a log.
        assert!(text.contains("line 19"), "{text}");
        assert!(text.contains("line 12"), "{text}");
        assert!(!text.contains("line 5"), "bounded: {text}");
        let last = text.lines().last().unwrap();
        assert_eq!(last, "line 19", "the newest line is the last one, not the first");
    }

    #[test]
    fn a_failure_report_with_no_log_is_just_the_message() {
        assert_eq!(with_stderr("nope", &[]), "nope");
    }
}
