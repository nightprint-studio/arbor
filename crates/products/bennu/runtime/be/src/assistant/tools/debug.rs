//! The debugger as tools: breakpoints, launching or attaching, waiting for a stop, reading the
//! stack and values, stepping, detaching — the model's way of *watching the program go wrong*
//! instead of guessing from the source.
//!
//! Nothing here is a second debugger. The session is the one the Debug panel shows (the same
//! `crate::debug_backend` registry, the same frames, the same events), so the user sees the program
//! stop where the assistant stopped it and can take over with the buttons they already have. This
//! file is the model-facing layer: specs, approvals, ownership, and the text the model reads (see
//! `debug_text` for the caps and the masking).
//!
//! ## What asks the user
//!
//! * `debug_launch` — always: it builds and runs project code. `debug_attach` — always: it reads another
//!   program's memory.
//! * Breakpoints, waiting, stack, variables, stepping, resuming, detaching: no. Setting one writes a
//!   marked line in the project's breakpoints, which the user sees and can delete; the rest act only on
//!   the session this conversation opened.
//! * `debug_evaluate` never asks, because it cannot run anything: it reads a path (`order.customer.name`)
//!   out of fields and array slots. Method calls, constructors, assignments and increments are refused
//!   before they reach the debugger, and no approval changes that.
//!
//! ## Who owns what, and how it ends
//!
//! One session per conversation, and it belongs to the **conversation**, not to the run that opened it: when a
//! run ends — done, cancelled or failed — the session is parked (see `debug_keep`), still attached, and the
//! next run of the conversation takes it back; a program stopped at a breakpoint stays suspended so the user
//! can inspect it in the Debug panel. It ends with `debug_detach` (a launched program is stopped, an attached
//! one is resumed and keeps running), when the program exits, after the idle time (the lease), or when the
//! conversation is deleted, the project closes or Arbor exits. The breakpoints the assistant set stay until
//! then.

use std::any::Any;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use arbor_ipc::prelude::EventSink;
use arbor_rpc::registry_for;
use bennu_assistant::prelude::{Lease, Op, ToolSpec};
use bennu_core::prelude::BennuState;
use bennu_proto::prelude::{DebugConfig, DebugPause, RunConfigSet};
use serde_json::{json, Value};

use super::debug_bp::{self, Removal, Set, MAX_LOG_MESSAGE_CHARS};
use super::debug_capture::{Capture, TeeSink};
use super::debug_consoles::{self, Console};
use super::debug_launch;
use super::debug_text::{self as text, MAX_STACK_FRAMES};
use super::{spec, Session, ToolHost};
use crate::debug::{apply_config, EVT_DEBUG_CONFIG};
use crate::debug_backend::{self, DebugBackend, Step};

// Navigation (run to a line, break on exception) and the changes to the program, in a file of their own.
#[path = "debug_more.rs"]
pub(super) mod more;

const NAMES: [&str; 13] = [
    "debug_set_breakpoint",
    "debug_list_breakpoints",
    "debug_remove_breakpoint",
    "debug_launch",
    "debug_attach",
    "debug_wait_stop",
    "debug_stack",
    "debug_variables",
    "debug_evaluate",
    "debug_step",
    "debug_resume",
    "debug_detach",
    "debug_output",
];

/// Whether `name` is one of this family's tools.
pub(super) fn is_debug_tool(name: &str) -> bool {
    NAMES.contains(&name) || more::is_extra_tool(name)
}

/// How often a wait looks at the session. A stop is noticed within this.
const POLL: Duration = Duration::from_millis(100);
/// How long a launch waits for the program to connect back to the debugger.
const CONNECT_WAIT: Duration = Duration::from_secs(45);
/// How long an attach waits for the session to register once the socket is open.
const ATTACH_WAIT: Duration = Duration::from_secs(10);
/// How long a step waits for the program to stop again before handing control back.
const STEP_WAIT: Duration = Duration::from_secs(20);
/// The most times one `debug_step` repeats.
const MAX_STEP_COUNT: u64 = 20;
/// `debug_wait_stop`'s default and ceiling, in seconds.
const DEFAULT_WAIT_S: u64 = 60;
const MAX_WAIT_S: u64 = 300;
/// Lines of program output volunteered alongside a stop or an end.
const FRESH_OUTPUT_LINES: usize = 12;

pub(super) fn specs() -> Vec<ToolSpec> {
    let mut all = base_specs();
    all.extend(more::specs());
    all
}

fn base_specs() -> Vec<ToolSpec> {
    vec![
        spec(
            "debug_set_breakpoint",
            "Set a breakpoint at a line of a Java file, saved in the project's breakpoints: the user sees it in the editor gutter marked as set by the assistant and in the Breakpoints list, and can disable or delete it. Setting the same line again updates it. It never replaces a breakpoint the user set. `condition` is a watch path compared with a literal (`total > 100`, `name == \"bob\"`). `log_message` makes it a logpoint: it prints the message, with `{path}` placeholders such as `{order.id}`, to the run console and does NOT stop the program. Takes effect at once in a running debug session, otherwise at the next debug launch.",
            json!({"type":"object","properties":{
                "file":{"type":"string","description":"The Java file."},
                "line":{"type":"integer","description":"1-based line. One with no code (a comment, a brace, a declaration) is moved to the next that has, and the answer says which line it ended on."},
                "condition":{"type":"string"},
                "log_message":{"type":"string","description":"Print this instead of stopping."},
                "suspend":{"type":"string","enum":["all","thread"],"description":"What a stop suspends: every thread (default) or only the thread that hit it, which lets the others run and be stopped too. Use `thread` for a request handler that must not freeze the rest."},
                "capture":{"type":"array","items":{"type":"string"},"description":"Read-only paths (`order.id`, `items[0].price`, at most 10) that Bennu reads at the moment of each stop and hands you with it — even if you are not waiting. Same rules as debug_evaluate; secrets are masked."}
            },"required":["file","line"]}),
        ),
        spec(
            "debug_list_breakpoints",
            "The project's breakpoints: who set each (yours or the user's), whether it is on, its condition, and what the running program made of it (installed, waiting for its class, hit counts).",
            json!({"type":"object","properties":{}}),
        ),
        spec(
            "debug_remove_breakpoint",
            "Remove a breakpoint you set, by file and line, or all of yours with `all`. The user's own breakpoints are never removed.",
            json!({"type":"object","properties":{
                "file":{"type":"string"},
                "line":{"type":"integer"},
                "all":{"type":"boolean","description":"Remove every breakpoint you set."}
            }}),
        ),
        spec(
            "debug_launch",
            "Build the project and start a Java program under the debugger, held at startup until your breakpoints are installed. The user approves every launch. One debug session per conversation; the program is yours and stays alive between messages until you detach, it exits, or it sits unused for too long. Give a run configuration's name in `config`, or a fully qualified `main_class`. Set breakpoints first, then call debug_wait_stop.",
            json!({"type":"object","properties":{
                "config":{"type":"string","description":"A run configuration of the project (application or Spring Boot)."},
                "main_class":{"type":"string","description":"Fully qualified, e.g. com.acme.App."},
                "module":{"type":"string","description":"Maven module, relative to the project."},
                "args":{"type":"array","items":{"type":"string"},"description":"Program arguments."},
                "vm_args":{"type":"array","items":{"type":"string"},"description":"JVM arguments (-D...). The debugger's agent is added for you."},
                "suspend":{"type":"boolean","description":"Hold the VM before main until breakpoints are installed. Default true."}
            }}),
        ),
        spec(
            "debug_attach",
            "Attach the debugger to a JVM that is already running with a JDWP agent listening on this machine (`-agentlib:jdwp=transport=dt_socket,server=y,suspend=n,address=*:PORT`). The user approves every attach. Detaching resumes it and leaves it running.",
            json!({"type":"object","properties":{"port":{"type":"integer"}},"required":["port"]}),
        ),
        spec(
            "debug_wait_stop",
            "Wait until the program stops at a breakpoint (or an exception breakpoint), then return where: the reason and the stack. Returns early if the program ends. If nothing stops within `timeout_s` (default 60, at most 300) it says the program is still running — it is not an error: wait again, or change the breakpoints.",
            json!({"type":"object","properties":{"timeout_s":{"type":"integer"}}}),
        ),
        spec(
            "debug_stack",
            "The stack of the stopped program (innermost first, library frames marked), capped. Says so if the program is running.",
            json!({"type":"object","properties":{"max_frames":{"type":"integer","description":"Default 15."}}}),
        ),
        spec(
            "debug_variables",
            "The variables in scope at a frame of the stopped program (`this`, arguments, locals), capped, with values shortened. Pass `path` (`order`, `order.items`) to list the fields or elements inside one object instead. Values of variables named like secrets (password, token, secret, apiKey...) are masked.",
            json!({"type":"object","properties":{
                "frame":{"type":"integer","description":"0 = innermost. Default 0."},
                "path":{"type":"string","description":"A variable or field path whose contents to list."}
            }}),
        ),
        spec(
            "debug_evaluate",
            "Read one value in a frame of the stopped program. The expression is a PATH — a name, `.field` and `[0]`, e.g. `order.customer.name` or `items[2].price` — read from fields and array slots. Methods, constructors, assignments, `++` and arithmetic are refused: nothing is run in the program and nothing in it can be changed.",
            json!({"type":"object","properties":{
                "expression":{"type":"string"},
                "frame":{"type":"integer","description":"0 = innermost. Default 0."}
            },"required":["expression"]}),
        ),
        spec(
            "debug_step",
            "Step the stopped program one line — `over` the call on it, `into` it (library and framework code is skipped), or `out` of the method — then wait for it to stop again and return where it is. `count` (default 1, at most 20) repeats the step and returns where it ends up; it stops early if something else stops the program (a breakpoint, an exception) or it ends.",
            json!({"type":"object","properties":{
                "kind":{"type":"string","enum":["over","into","out"]},
                "count":{"type":"integer"},
                "return_value":{"type":"boolean","description":"Also report what the method the step leaves returned (the last method that returned during the step). Slows a step over heavy code, so ask only when you need the value."},
                "thread":{"type":"string","description":"Step this stopped thread (id or name) instead of the one you are looking at."}
            },"required":["kind"]}),
        ),
        spec(
            "debug_resume",
            "Let the stopped program run on, and return at once. Use debug_wait_stop for the next stop. If other threads are stopped too, resuming moves on to the next of them (it becomes the stop you look at) — the answer says so.",
            json!({"type":"object","properties":{"thread":{"type":"string","description":"Resume this stopped thread (id or name) instead of the one you are looking at."}}}),
        ),
        spec(
            "debug_detach",
            "End the debug session. A program you launched is stopped; one you attached to is resumed and keeps running. Do this as soon as you have what you need: when the run ends it happens anyway, but a program left suspended in the meantime blocks the user.",
            json!({"type":"object","properties":{}}),
        ),
        spec(
            "debug_output",
            "What a program launched under the debugger has printed (stdout and stderr, `[err]` marks stderr): the newest `tail` lines (default 40, at most 200), or only those after `since` (the `next_since` of a previous call). Also says whether it has exited. Works after the session has closed. Without `session` it reads the console of the program YOU last launched, never another conversation's; give the `session` id that debug_launch printed to read a specific one.",
            json!({"type":"object","properties":{
                "session":{"type":"string","description":"The console id debug_launch gave you. Default: the program you launched last."},
                "tail":{"type":"integer"},
                "since":{"type":"integer"}
            }}),
        ),
    ]
}

// ── State of one run ─────────────────────────────────────────────────────────────────────────────

/// The session this run opened.
pub(super) struct Hold {
    /// The debug session id; for a launch it is also the run id of the program.
    pub(super) session_id: String,
    /// The program was started by this conversation (and is stopped with the session).
    pub(super) launched: bool,
    pub(super) capture: Option<Arc<Capture>>,
    pub(super) label: String,
    /// Renewed by every debug tool of the conversation; when it runs out the session is released.
    pub(super) lease: Lease,
}

/// Where a run debugged: kept after the session ends, so the breakpoints it set can be taken down.
struct Past {
    root: String,
    sink: Arc<dyn EventSink>,
    session: Arc<Session>,
}

/// A run's debugger state. Dropped with the tool host, which is when the run is over however it ended: an open
/// session is then *parked* (see `debug_keep`) for the conversation's next run, not released.
#[derive(Default)]
pub(super) struct DebugRun {
    hold: Mutex<Option<Hold>>,
    past: Mutex<Option<Past>>,
    /// The conversation (id, title) this state belongs to, set by [`Self::adopt`].
    owner: Mutex<Option<(String, String)>>,
    /// Said once, on the first debug tool of a run that took a session back: what state it is in.
    adopted_note: Mutex<Option<String>>,
    /// The last output line already shown, so volunteered output is only what is new.
    cursor: AtomicU64,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl DebugRun {
    /// Start of a run: take back the session this conversation left parked, if it is still alive.
    pub(super) fn adopt(&self, session: &Arc<Session>) {
        *lock(&self.owner) = Some((session.conversation_id.clone(), session.title.clone()));
        let Some(mut k) = super::debug_keep::take(&session.conversation_id) else { return };
        if debug_backend::get(&k.hold.session_id).is_err() {
            // The program went while nobody was looking.
            let _ = release_session(&k.hold, &k.sink);
            remove_assistant_breakpoints(&k.root, &k.sink);
            return;
        }
        let state = match debug_backend::get(&k.hold.session_id).ok().and_then(|b| b.paused()) {
            Some(p) => format!("SUSPENDED — {}", text::stack_text(&p, 1).lines().next().unwrap_or("stopped")),
            None => "RUNNING".to_string(),
        };
        *lock(&self.adopted_note) = Some(format!("(Continuing the debug session of an earlier message: `{}`, {state}. Your breakpoints are still set.)\n", k.hold.label));
        k.hold.lease.set_idle_minutes(session.debug_idle_minutes);
        k.hold.lease.touch(super::debug_keep::now_ms());
        *lock(&self.hold) = Some(k.hold);
        *lock(&self.past) = Some(Past { root: k.root, sink: k.sink, session: Arc::clone(session) });
    }

    /// Any debug tool was used: the idle clock starts over.
    fn touch(&self) {
        if let Some(h) = lock(&self.hold).as_mut() {
            h.lease.touch(super::debug_keep::now_ms());
        }
    }
}

impl Drop for DebugRun {
    fn drop(&mut self) {
        let mut hold = self.hold.get_mut().unwrap_or_else(|p| p.into_inner()).take();
        let past = self.past.get_mut().unwrap_or_else(|p| p.into_inner()).take();
        let owner = self.owner.get_mut().unwrap_or_else(|p| p.into_inner()).take();
        let Some(past) = past else { return };
        // A live session outlives the run: the program stays up (suspended, if it was stopped) for the next
        // message, until the agent detaches or the idle lease runs out.
        let alive = hold.as_ref().is_some_and(|h| debug_backend::get(&h.session_id).is_ok());
        if let (true, Some((conversation, title))) = (alive, owner.as_ref()) {
            if let Some(h) = hold.take() {
                super::debug_keep::park(conversation, super::debug_keep::Kept { hold: h, root: past.root, sink: past.sink, title: title.clone() });
                return;
            }
        }
        let mut done = Vec::new();
        if let Some(hold) = hold {
            done.extend(release_session(&hold, &past.sink));
        }
        done.extend(remove_assistant_breakpoints(&past.root, &past.sink));
        if !done.is_empty() {
            past.session.view().push(json!({ "type": "notice", "level": "info", "text": format!("The run ended, so the debugger was released: {}.", done.join(", ")) }));
            past.session.push_message();
        }
    }
}

/// Call a handler of the catalogue with a state of our own (the launch needs its own event sink).
fn call_with(state: &BennuState, method: &str, params: Value) -> Result<Value, String> {
    let ctx = state as &dyn Any;
    if let Some(call) = registry_for("").get(method) {
        return call(ctx, params);
    }
    Err(format!("`{method}` is not available"))
}

/// End a session: detach the debugger (which resumes a suspended program) and stop the program when this
/// conversation started it. What was done, for the notice.
pub(super) fn release_session(hold: &Hold, sink: &Arc<dyn EventSink>) -> Vec<String> {
    crate::assistant::stop_watch::unwatch(&hold.session_id);
    let mut done = Vec::new();
    if let Ok(backend) = debug_backend::get(&hold.session_id) {
        let _ = backend.detach();
        done.push(if hold.launched { "detached the debugger".to_string() } else { "detached the debugger and resumed the program".to_string() });
    }
    if hold.launched {
        let state = BennuState::new(Arc::clone(sink));
        if call_with(&state, "bennu_cancel_run", json!({ "args": { "run_id": hold.session_id } })).is_ok() {
            done.push(format!("stopped `{}`", hold.label));
        }
    }
    done
}

/// Take down the breakpoints the assistant set, and tell the panel to reload them.
pub(super) fn remove_assistant_breakpoints(root: &str, sink: &Arc<dyn EventSink>) -> Option<String> {
    let mut cfg: DebugConfig = crate::repo_config::load(root, "debug");
    let removed = debug_bp::remove_all_assistant(&mut cfg);
    if removed == 0 || apply_config(root, &cfg).is_err() {
        return None;
    }
    sink.emit(EVT_DEBUG_CONFIG, json!({ "root": root }));
    Some(format!("removed its {removed} breakpoint(s)"))
}

fn str_arg<'a>(input: &'a Value, key: &str) -> Result<&'a str, String> {
    input.get(key).and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| format!("`{key}` is required"))
}

/// `capture`: read-only paths (the grammar of a watch) to read at hit time. Invalid ones are refused up front.
fn capture_arg(input: &Value) -> Result<Vec<String>, String> {
    let Some(list) = input.get("capture").and_then(Value::as_array) else { return Ok(Vec::new()) };
    if list.len() > debug_bp::MAX_CAPTURE_PATHS {
        return Err(format!("`capture` takes at most {} paths", debug_bp::MAX_CAPTURE_PATHS));
    }
    let mut out = Vec::new();
    for item in list {
        let path = item.as_str().map(str::trim).filter(|p| !p.is_empty()).ok_or("`capture` is a list of paths such as `order.id`")?;
        text::parse_expression(path)?;
        out.push(path.to_string());
    }
    Ok(out)
}

fn line_arg(input: &Value) -> Result<u32, String> {
    let line = input.get("line").and_then(Value::as_u64).ok_or("`line` is required")?;
    u32::try_from(line).ok().filter(|l| (1..=1_000_000).contains(l)).ok_or_else(|| "`line` must be a positive line number".to_string())
}

// ── The tools ────────────────────────────────────────────────────────────────────────────────────

impl ToolHost {
    /// The paths a debug call touches, for the access policy.
    pub(super) fn debug_touched(&self, tool: &str, input: &Value) -> Vec<(PathBuf, Op)> {
        match tool {
            "debug_set_breakpoint" | "debug_remove_breakpoint" | "debug_run_to" => {
                input.get("file").and_then(Value::as_str).map(|f| vec![(self.resolve(f), Op::Read)]).unwrap_or_default()
            }
            "debug_launch" | "debug_attach" => vec![(self.project().to_path_buf(), Op::List)],
            _ => Vec::new(),
        }
    }

    /// The questions a debug call raises before it runs. `Err` carries the reason it was refused.
    pub(super) fn debug_gate(&self, tool: &str, input: &Value) -> Result<(), String> {
        match tool {
            "debug_launch" => {
                self.ensure_no_session()?;
                // Checked before asking: a call that cannot run is not worth the user's attention.
                let configs: RunConfigSet = crate::repo_config::load(&self.session.root, "run");
                let plan = debug_launch::plan(input, &configs)?;
                let args = if plan.args.is_empty() { String::new() } else { format!("\nargs: {}", plan.args.join(" ")) };
                self.confirm(
                    "Run under the debugger?",
                    &format!("{}\nIt builds the project, then runs this program on your machine. You will see it stop in the editor.{args}", plan.main_class),
                    true,
                )
            }
            "debug_attach" => {
                self.ensure_no_session()?;
                let port = port_arg(input)?;
                self.confirm("Attach the debugger?", &format!("To the JVM listening on 127.0.0.1:{port}. It can read that program's variables; it changes nothing."), false)
            }
            t if more::is_mutate_tool(t) => self.mutate_gate(t, input),
            _ => Ok(()),
        }
    }

    pub(super) fn run_debug(&self, tool: &str, input: &Value) -> Result<String, String> {
        self.debug.touch();
        let note = lock(&self.debug.adopted_note).take().unwrap_or_default();
        let result = self.run_debug_tool(tool, input);
        match result {
            Ok(out) => Ok(format!("{note}{out}")),
            Err(e) => Err(format!("{note}{e}")),
        }
    }

    fn run_debug_tool(&self, tool: &str, input: &Value) -> Result<String, String> {
        match tool {
            "debug_set_breakpoint" => self.debug_set(input),
            "debug_list_breakpoints" => self.debug_list(),
            "debug_remove_breakpoint" => self.debug_remove(input),
            "debug_launch" => self.debug_launch(input),
            "debug_attach" => self.debug_attach(input),
            "debug_wait_stop" => self.debug_wait(input),
            "debug_stack" => self.debug_stack(input),
            "debug_variables" => self.debug_variables(input),
            "debug_evaluate" => self.debug_evaluate(input),
            "debug_step" => self.debug_step(input),
            "debug_resume" => self.debug_resume(input),
            "debug_detach" => self.debug_detach(),
            "debug_output" => self.debug_output(input),
            other if more::is_extra_tool(other) => self.run_debug_more(other, input),
            other => Err(format!("`{other}` is not a debug tool")),
        }
    }

    fn ensure_no_session(&self) -> Result<(), String> {
        match lock(&self.debug.hold).as_ref() {
            Some(h) => Err(format!("a debug session is already open (`{}`): there is one per conversation. End it with debug_detach first.", h.label)),
            None => Ok(()),
        }
    }

    /// The open session and its debugger. When the program has gone, the session is closed here and the
    /// error says how it ended, so the next call is not a mystery.
    fn held(&self) -> Result<(Arc<dyn DebugBackend>, Option<Arc<Capture>>), String> {
        let (id, capture) = {
            let hold = lock(&self.debug.hold);
            let Some(h) = hold.as_ref() else {
                return Err("no debug session is open: start one with debug_launch (or debug_attach)".into());
            };
            (h.session_id.clone(), h.capture.clone())
        };
        match debug_backend::get(&id) {
            Ok(backend) => Ok((backend, capture)),
            Err(_) => Err(self.session_ended(capture.as_deref())),
        }
    }

    /// Close the books on a session whose program is gone, and describe how it ended.
    fn session_ended(&self, capture: Option<&Capture>) -> String {
        let hold = lock(&self.debug.hold).take();
        let console = hold.as_ref().filter(|h| h.capture.is_some()).map(|h| h.session_id.clone());
        if let (Some(hold), Some(past)) = (hold, lock(&self.debug.past).as_ref()) {
            // Nothing to detach from; this only makes sure a program that is somehow still alive is stopped.
            release_session(&hold, &past.sink);
        }
        let mut out = format!("The program has ended{}. The debug session is closed.", text::exit_text(capture.and_then(Capture::exit)));
        if let Some(c) = capture {
            out.push_str(&self.fresh_output(c, FRESH_OUTPUT_LINES));
        }
        if let Some(id) = console {
            out.push_str(&format!("\nThe whole output stays readable: debug_output (console `{id}`)."));
        }
        out.push_str("\nYour breakpoints are still set: launch again with debug_launch, or remove them.");
        out
    }

    /// The program's output since the last time any tool showed it, as a block to append.
    fn fresh_output(&self, capture: &Capture, max: usize) -> String {
        let since = self.debug.cursor.load(Ordering::SeqCst);
        let (lines, last) = capture.read(since, max);
        self.debug.cursor.store(last, Ordering::SeqCst);
        let skipped = capture.count_after(since).saturating_sub(lines.len() as u64);
        if lines.is_empty() {
            return String::new();
        }
        let more = if skipped > 0 { format!(" ({skipped} older lines not shown; debug_output has them)") } else { String::new() };
        format!("\nProgram output since the last check{more}:\n{}", lines.iter().map(|(_, l)| l.as_str()).collect::<Vec<_>>().join("\n"))
    }

    fn open_session(&self, hold: Hold, root: String) {
        // From now on a stop of this session is noticed by Bennu and told to this conversation.
        crate::assistant::stop_watch::watch(&hold.session_id, &self.session.conversation_id, &self.session.title, Arc::clone(&self.session.sink));
        *lock(&self.debug.hold) = Some(hold);
        *lock(&self.debug.past) = Some(Past { root, sink: Arc::clone(&self.session.sink), session: Arc::clone(&self.session) });
        self.debug.cursor.store(0, Ordering::SeqCst);
    }

    fn new_lease(&self) -> Lease {
        Lease::new(super::debug_keep::now_ms(), self.session.debug_idle_minutes)
    }

    fn notify_breakpoints_changed(&self, root: &str) {
        self.session.sink.emit(EVT_DEBUG_CONFIG, json!({ "root": root }));
    }

    // ── Breakpoints ──────────────────────────────────────────────────────────────────────────────

    fn debug_set(&self, input: &Value) -> Result<String, String> {
        let path = self.resolve(str_arg(input, "file")?);
        let line = line_arg(input)?;
        if !path.is_file() {
            return Err(format!("`{}` is not a file", path.display()));
        }
        let shown = path.to_string_lossy().to_string();
        if !crate::intel::is_java_file(&shown) {
            return Err("breakpoints go in Java files".into());
        }
        let source = std::fs::read_to_string(&path).map_err(|e| format!("could not read the file: {e}"))?;
        let at = text::source_line(&source, line)?;
        // The line asked for may hold no code; what is set, shown and reported from here on is the one that does.
        let line = at.line;
        let condition = input.get("condition").and_then(Value::as_str).unwrap_or("").trim().to_string();
        if let Err(why) = crate::debug_cond::parse(&condition) {
            return Err(format!("the condition is not valid, so nothing was set: {why}"));
        }
        let log_message = input.get("log_message").and_then(Value::as_str).unwrap_or("").trim().to_string();
        if log_message.chars().count() > MAX_LOG_MESSAGE_CHARS {
            return Err(format!("`log_message` is at most {MAX_LOG_MESSAGE_CHARS} characters"));
        }
        let capture = capture_arg(input)?;
        let thread_only = match input.get("suspend").and_then(Value::as_str) {
            None | Some("all") => false,
            Some("thread") => true,
            Some(other) => return Err(format!("`suspend` is `all` or `thread`, not `{other}`")),
        };

        let root = self.root_of(&path);
        let mut cfg: DebugConfig = crate::repo_config::load(&root, "debug");
        let outcome = debug_bp::set_with(&mut cfg, &shown, line, &condition, &log_message, &capture, thread_only)?;
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if outcome == Set::UserOwns {
            return Ok(format!("{name}:{line} already has a breakpoint set by the user; it was left exactly as it is, and it will stop the program there."));
        }
        apply_config(&root, &cfg)?;
        self.notify_breakpoints_changed(&root);

        let verb = if outcome == Set::Added { "set" } else { "updated" };
        let mut out = format!("breakpoint {verb} at {name}:{line}: `{}`", at.text);
        if let Some(asked) = at.moved_from {
            out.push_str(&format!("\nNote: line {asked} holds no code (a comment, blank line, annotation, brace or declaration), so it was set on line {line}, the next one that does. Read the file first and pick a line with a statement."));
        }
        out.push_str("\nThe user sees it in the gutter (marked as set by the assistant) and can disable or delete it.");
        if !log_message.is_empty() {
            out.push_str(" It is a logpoint: it prints and does not stop.");
        } else if thread_only {
            out.push_str(" A stop suspends only the thread that hit it: the others keep running and can stop too (debug_select_thread switches between them).");
        }
        if !capture.is_empty() {
            out.push_str(&format!(" At each stop Bennu reads {} and hands you the values, so you do not have to ask.", capture.join(", ")));
        }
        out.push_str(&self.breakpoint_state_note(&shown, line));
        Ok(out)
    }

    /// What the running session made of a breakpoint, or when it will take effect.
    fn breakpoint_state_note(&self, file: &str, line: u32) -> String {
        let Ok((backend, _)) = self.held() else { return "\nIt takes effect at the next debug launch.".into() };
        let states = backend.breakpoint_states();
        match states.iter().find(|s| s.line == line && debug_bp::canon(&s.file).eq_ignore_ascii_case(&debug_bp::canon(file))) {
            Some(s) => format!("\nIn the running program: {}.", debug_bp::state_word(s)),
            None => "\nIt takes effect in the running program.".into(),
        }
    }

    fn debug_list(&self) -> Result<String, String> {
        let cfg: DebugConfig = crate::repo_config::load(&self.session.root, "debug");
        let states = self.held().map(|(b, _)| b.breakpoint_states()).unwrap_or_default();
        Ok(debug_bp::list_text(&cfg, &states))
    }

    /// Threads that were stopped on a breakpoint just removed and were let go, if the session says any were.
    fn released_note(&self) -> String {
        let backend = lock(&self.debug.hold).as_ref().and_then(|h| debug_backend::get(&h.session_id).ok());
        match backend.map(|b| b.take_released()) {
            Some(n) if n > 0 => format!(" {n} thread(s) that were stopped on it were resumed."),
            _ => String::new(),
        }
    }

    fn debug_remove(&self, input: &Value) -> Result<String, String> {
        let root = self.session.root.clone();
        let mut cfg: DebugConfig = crate::repo_config::load(&root, "debug");
        if input.get("all").and_then(Value::as_bool).unwrap_or(false) {
            let n = debug_bp::remove_all_assistant(&mut cfg);
            if n > 0 {
                apply_config(&root, &cfg)?;
                self.notify_breakpoints_changed(&root);
            }
            return Ok(format!("removed {n} breakpoint(s) you had set. The user's were left alone.{}", self.released_note()));
        }
        let path = self.resolve(str_arg(input, "file")?);
        let line = line_arg(input)?;
        let root = self.root_of(&path);
        let mut cfg: DebugConfig = crate::repo_config::load(&root, "debug");
        let shown = path.to_string_lossy().to_string();
        match debug_bp::remove(&mut cfg, &shown, line) {
            Removal::Removed => {
                apply_config(&root, &cfg)?;
                self.notify_breakpoints_changed(&root);
                Ok(format!("breakpoint at line {line} removed.{}", self.released_note()))
            }
            Removal::NotFound => Err("there is no breakpoint there (debug_list_breakpoints shows them)".into()),
            Removal::UserOwns => Err("that breakpoint is the user's, so it was left alone".into()),
        }
    }

    // ── Starting and ending ──────────────────────────────────────────────────────────────────────

    fn debug_launch(&self, input: &Value) -> Result<String, String> {
        self.ensure_no_session()?;
        let root = self.session.root.clone();
        let configs: RunConfigSet = crate::repo_config::load(&root, "run");
        let plan = debug_launch::plan(input, &configs)?;

        // Fresh classes, or the debugger shows code that is not the code on disk. Cheap when nothing changed.
        let built = crate::build_queue::with_context(self.caller_context(), || crate::build::compile_project(&self.session.ctx, &root, plan.module.as_deref()))?.value;
        if !built.ok {
            return Err(format!("the build failed with {} problem(s), so nothing was launched. Fix them first (bennu_problems, bennu_build).", built.diagnostics.len()));
        }

        // The launch gets a sink that also keeps the program's output: the panel sees what it always sees.
        let capture = Arc::new(Capture::default());
        let tee: Arc<dyn EventSink> = Arc::new(TeeSink::new(Arc::clone(&self.session.sink), Arc::clone(&capture)));
        let mut state = BennuState::new(tee);
        if let Some(host) = self.session.ctx.host_caller() {
            state = state.with_host_caller(host);
        }
        let params = json!({ "args": {
            "root": root, "main_class": plan.main_class, "args": plan.args, "vm_args": plan.vm_args,
            "module": plan.module, "working_dir": plan.working_dir, "env": plan.env,
            "classpath_scope": plan.classpath_scope, "debug": true, "debug_suspend": plan.suspend,
        }});
        let handle = call_with(&state, "bennu_run", params)?;
        let run_id = handle.get("run_id").and_then(Value::as_str).ok_or("the launch did not return a run id")?.to_string();

        // Held from here on: if anything below fails or the run is cancelled, the program is stopped with the run.
        self.open_session(Hold { session_id: run_id.clone(), launched: true, capture: Some(Arc::clone(&capture)), label: plan.label.clone(), lease: self.new_lease() }, root.clone());
        // Its output is kept under the run id, past the end of the session and apart from other conversations' runs.
        debug_consoles::register(Console { session_id: run_id.clone(), conversation: self.session.conversation_id.clone(), label: plan.label.clone(), capture: Arc::clone(&capture) });
        // So the Run console shows it like any launch the user made, and the Debug panel follows it.
        self.session.sink.emit("arbor://bennu/assistant-run", json!({
            "run_id": run_id, "root": root, "label": plan.label, "main_class": plan.main_class,
            "module": plan.module.clone().unwrap_or_default(), "args": plan.args, "vm_args": plan.vm_args,
            "command": handle.get("command"), "working_dir": handle.get("working_dir"),
            "suspend": plan.suspend, "classpath_scope": plan.classpath_scope.clone().unwrap_or_default(),
        }));

        let deadline = Instant::now() + CONNECT_WAIT;
        let vm = loop {
            if let Ok(backend) = debug_backend::get(&run_id) {
                break backend.describe();
            }
            if capture.exit().is_some() {
                let why = self.session_ended(Some(&capture));
                return Err(format!("the program exited before the debugger could connect.\n{why}"));
            }
            if self.session.cancelled() {
                return Err("cancelled while the program was starting".into());
            }
            if Instant::now() >= deadline {
                return Err("the program did not connect to the debugger in time; it is stopped with the run. Look at debug_output.".into());
            }
            std::thread::sleep(POLL);
        };
        let held_note = if plan.suspend { "It is held at startup until the breakpoints are installed, then runs." } else { "It is already running: a breakpoint on code that has run is missed." };
        Ok(format!(
            "launched `{}` under the debugger{} (build: {}). {held_note}\nConsole id: `{run_id}` — debug_output reads it, even after the program has exited. Call debug_wait_stop to wait for a breakpoint.",
            plan.label,
            if vm.is_empty() { String::new() } else { format!(" ({vm})") },
            built.tool
        ))
    }

    fn debug_attach(&self, input: &Value) -> Result<String, String> {
        self.ensure_no_session()?;
        let port = port_arg(input)?;
        let root = self.session.root.clone();
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
        let id = format!("ai-attach-{port}-{nanos}");
        crate::debug::attach(id.clone(), root.clone(), port, Arc::clone(&self.session.sink))?;
        self.open_session(Hold { session_id: id.clone(), launched: false, capture: None, label: format!("JVM on port {port}"), lease: self.new_lease() }, root);
        let deadline = Instant::now() + ATTACH_WAIT;
        let vm = loop {
            if let Ok(backend) = debug_backend::get(&id) {
                break backend.describe();
            }
            if Instant::now() >= deadline || self.session.cancelled() {
                return Err("the debugger connected but the session did not start".into());
            }
            std::thread::sleep(POLL);
        };
        Ok(format!("attached to the JVM on port {port} ({vm}). It keeps running; breakpoints are installed. Call debug_wait_stop. Detaching resumes it and leaves it running."))
    }

    fn debug_detach(&self) -> Result<String, String> {
        let Some(hold) = lock(&self.debug.hold).take() else {
            return Ok("no debug session was open.".into());
        };
        let sink = lock(&self.debug.past).as_ref().map(|p| Arc::clone(&p.sink)).unwrap_or_else(|| Arc::clone(&self.session.sink));
        let done = release_session(&hold, &sink);
        let mut out = format!("debug session closed: {}.", if done.is_empty() { "the program had already gone".to_string() } else { done.join(", ") });
        if let Some(c) = &hold.capture {
            out.push_str(&self.fresh_output(c, FRESH_OUTPUT_LINES));
            out.push_str(&format!("\nThe whole output stays readable: debug_output (console `{}`).", hold.session_id));
        }
        out.push_str("\nYour breakpoints stay set for another launch until the run ends; debug_remove_breakpoint (all) takes them down.");
        Ok(out)
    }

    // ── Waiting, stepping, running on ────────────────────────────────────────────────────────────

    /// Wait for the program to stop. `Ended` when the session is gone.
    fn wait_for_stop(&self, limit: Duration) -> Wait {
        // While this runs a stop is handed back by the wait itself, not queued as a note (see `stop_watch`).
        let _waiting = crate::assistant::stop_watch::waiting(&self.session.conversation_id);
        let deadline = Instant::now() + limit;
        loop {
            let (backend, capture) = match self.held() {
                Ok(held) => held,
                Err(ended) => return Wait::Ended(ended),
            };
            if let Some(pause) = backend.paused() {
                return Wait::Stopped(pause);
            }
            if let Some(c) = &capture {
                if c.exit().is_some() {
                    // The socket closes just after; let the session notice rather than race it.
                    std::thread::sleep(POLL);
                    return Wait::Ended(self.session_ended(Some(c)));
                }
            }
            if self.session.cancelled() {
                return Wait::Cancelled;
            }
            if Instant::now() >= deadline {
                return Wait::TimedOut;
            }
            std::thread::sleep(POLL);
        }
    }

    fn debug_wait(&self, input: &Value) -> Result<String, String> {
        let secs = text::wait_seconds(input.get("timeout_s").and_then(Value::as_u64), DEFAULT_WAIT_S, MAX_WAIT_S);
        match self.wait_for_stop(Duration::from_secs(secs)) {
            Wait::Stopped(p) => Ok(self.stop_report(&p)),
            Wait::Ended(why) => Ok(why),
            Wait::Cancelled => Err("the run was cancelled while waiting; the debug session will be released".into()),
            Wait::TimedOut => {
                let mut out = format!("Nothing stopped within {secs} s: the program is still RUNNING and no breakpoint was hit. This is not an error. Wait again, trigger the code path (an HTTP request with http_request, for example), or check the breakpoints with debug_list_breakpoints.");
                if let Ok((_, Some(c))) = self.held() {
                    out.push_str(&self.fresh_output(&c, FRESH_OUTPUT_LINES));
                }
                Ok(out)
            }
        }
    }

    /// A stop as the model reads it: why, the top of the stack, and what the program printed meanwhile.
    fn stop_report(&self, p: &DebugPause) -> String {
        let mut out = text::stack_text(p, 8);
        out.push_str(&self.stop_values());
        if let Ok((_, Some(c))) = self.held() {
            out.push_str(&self.fresh_output(&c, FRESH_OUTPUT_LINES));
        }
        out.push_str("\nNext: debug_variables, debug_evaluate, debug_step, debug_resume — or debug_detach when you have what you need.");
        out
    }

    /// The stopped program, or the error saying it is not stopped.
    fn stopped(&self) -> Result<(Arc<dyn DebugBackend>, DebugPause), String> {
        let (backend, _) = self.held()?;
        match backend.paused() {
            Some(p) => Ok((backend, p)),
            None => Err("the program is RUNNING, not stopped, so there is nothing to read. Use debug_wait_stop.".into()),
        }
    }

    fn debug_stack(&self, input: &Value) -> Result<String, String> {
        let (_, pause) = self.stopped()?;
        let max = input.get("max_frames").and_then(Value::as_u64).map(|m| m as usize).unwrap_or(MAX_STACK_FRAMES).clamp(1, 60);
        Ok(text::stack_text(&pause, max))
    }

    fn debug_step(&self, input: &Value) -> Result<String, String> {
        let kind = str_arg(input, "kind")?;
        if !matches!(kind, "over" | "into" | "out") {
            return Err("`kind` is over, into or out".into());
        }
        let count = input.get("count").and_then(Value::as_u64).unwrap_or(1).clamp(1, MAX_STEP_COUNT) as usize;
        self.pick_thread(input)?;
        let want_return = input.get("return_value").and_then(Value::as_bool).unwrap_or(false);
        let mut taken = 0;
        loop {
            let (backend, _) = self.stopped()?;
            // Only the last step of a count has a value worth reading.
            if want_return && taken + 1 == count {
                backend.want_return_value()?;
            }
            backend.step(Step::parse(kind))?;
            match self.wait_for_stop(STEP_WAIT) {
                Wait::Stopped(p) => {
                    taken += 1;
                    // Something other than the step stopped the program: that is the news.
                    if taken < count && p.reason == "step" {
                        continue;
                    }
                    let times = if taken > 1 { format!(" {taken} times") } else { String::new() };
                    let early = if taken < count { format!(" (stopped after {taken} of {count}: the program stopped for another reason)") } else { String::new() };
                    return Ok(format!("stepped {kind}{times}{early}.\n{}{}", text::stack_text(&p, 5), self.stop_values()));
                }
                Wait::Ended(why) => return Ok(why),
                Wait::Cancelled => return Err("the run was cancelled while stepping; the debug session will be released".into()),
                Wait::TimedOut => return Ok(format!("stepped {kind}, but the program did not stop again within {} s: it is RUNNING (the line may be waiting on something). Use debug_wait_stop.", STEP_WAIT.as_secs())),
            }
        }
    }

    /// The values Bennu captured with the latest stop (the breakpoint's `capture`, the top frame's locals), once.
    fn stop_values(&self) -> String {
        let id = lock(&self.debug.hold).as_ref().map(|h| h.session_id.clone());
        id.map(|id| crate::assistant::stop_watch::take_values(&id)).filter(|v| !v.is_empty()).map(|v| format!("\n{}", v.trim_end())).unwrap_or_default()
    }

    fn debug_resume(&self, input: &Value) -> Result<String, String> {
        self.pick_thread(input)?;
        let (backend, _) = self.stopped()?;
        backend.resume()?;
        match backend.paused() {
            // Another thread was waiting: it is the stop now, already suspended, so the program is not running.
            Some(next) => Ok(format!("resumed. Another thread was stopped too, and it is the stop now:\n{}", text::stack_text(&next, 8))),
            None => Ok("resumed: the program is RUNNING. Call debug_wait_stop for the next stop.".into()),
        }
    }

    /// `thread` on a tool: make that stopped thread the one the tool acts on.
    fn pick_thread(&self, input: &Value) -> Result<(), String> {
        let Some(which) = input.get("thread").and_then(Value::as_str).map(str::trim).filter(|t| !t.is_empty()) else { return Ok(()) };
        let (backend, _) = self.stopped()?;
        backend.select_thread(which).map(|_| ())
    }

    // ── Reading values ───────────────────────────────────────────────────────────────────────────

    fn frame_arg(input: &Value, pause: &DebugPause) -> Result<usize, String> {
        let frame = input.get("frame").and_then(Value::as_u64).unwrap_or(0) as usize;
        text::check_frame(frame, pause.frames.len())?;
        Ok(frame)
    }

    fn debug_variables(&self, input: &Value) -> Result<String, String> {
        let (backend, pause) = self.stopped()?;
        let frame = Self::frame_arg(input, &pause)?;
        let at = pause.frames.get(frame).map(text::frame_line).unwrap_or_default();
        let Some(path) = input.get("path").and_then(Value::as_str).map(str::trim).filter(|p| !p.is_empty()) else {
            let rows = backend.variables(frame)?;
            return Ok(format!("variables at {at}:\n{}", text::rows_text(&rows)));
        };
        text::parse_expression(path)?;
        let root = backend.watch(frame, path)?;
        let Some(handle) = &root.object else {
            return Ok(format!("{} (nothing inside it)", text::row_text(&root)));
        };
        let inner = backend.expand(handle)?;
        Ok(format!("{}\ninside {path}:\n{}", text::row_text(&root), text::rows_text(&inner)))
    }

    fn debug_evaluate(&self, input: &Value) -> Result<String, String> {
        let expression = str_arg(input, "expression")?;
        // Before anything touches the program: the rule is in `parse_expression`.
        text::parse_expression(expression)?;
        let (backend, pause) = self.stopped()?;
        let frame = Self::frame_arg(input, &pause)?;
        let value = backend.watch(frame, expression)?;
        Ok(text::row_text(&value))
    }

    // ── Output ───────────────────────────────────────────────────────────────────────────────────

    fn debug_output(&self, input: &Value) -> Result<String, String> {
        let console = self.console_for(input.get("session").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()))?;
        let capture = &console.capture;
        let tail = input.get("tail").and_then(Value::as_u64).map(|t| t as usize).unwrap_or(40).clamp(1, 200);
        let since = input.get("since").and_then(Value::as_u64).unwrap_or(0);
        let (lines, last) = capture.read(since, tail);
        self.debug.cursor.store(last, Ordering::SeqCst);
        let state = match capture.exit() {
            Some(code) => format!("exited{}", text::exit_text(Some(code))),
            None => "still running".to_string(),
        };
        let whose = if console.conversation == self.session.conversation_id { "" } else { ", launched by another conversation" };
        let body = lines.iter().map(|(seq, l)| format!("#{seq} {l}")).collect::<Vec<_>>().join("\n");
        Ok(format!("console `{}` ({}{whose}): {} line(s), the program is {state}; next_since={last}\n{body}", console.session_id, console.label, lines.len()))
    }

    /// The console to read: the one asked for by id, else the program this session holds, else the latest this
    /// conversation launched. Never "the newest in the process" — that would be another agent's.
    fn console_for(&self, asked: Option<&str>) -> Result<Console, String> {
        if let Some(id) = asked {
            return debug_consoles::find(id).ok_or_else(|| format!("no console `{id}`; {}", debug_consoles::list_text()));
        }
        let open = lock(&self.debug.hold).as_ref().map(|h| (h.session_id.clone(), h.capture.is_some()));
        match open {
            Some((_, false)) => Err("the program was attached to, not launched here, so its output is not available".into()),
            Some((id, true)) => debug_consoles::find(&id).ok_or_else(|| "this session's console is no longer kept".to_string()),
            None => debug_consoles::latest_of(&self.session.conversation_id)
                .ok_or_else(|| format!("this conversation has not launched a program yet; {}", debug_consoles::list_text())),
        }
    }
}

/// How a wait ended.
enum Wait {
    Stopped(DebugPause),
    /// The session is gone; the text says how the program ended.
    Ended(String),
    TimedOut,
    Cancelled,
}

fn port_arg(input: &Value) -> Result<u16, String> {
    input.get("port").and_then(Value::as_u64).and_then(|p| u16::try_from(p).ok()).filter(|p| *p > 0).ok_or_else(|| "`port` is required (1-65535)".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_family_is_exactly_the_specs() {
        let specs = specs();
        assert_eq!(specs.len(), NAMES.len() + more::specs().len());
        for s in &specs {
            assert!(is_debug_tool(&s.name), "{}", s.name);
        }
        assert!(!is_debug_tool("bennu_debug_resume"), "the panel's handlers are not the assistant's tools");
        assert!(!is_debug_tool("run_start"));
    }

    #[test]
    fn a_port_and_a_line_are_checked() {
        assert_eq!(port_arg(&json!({ "port": 5005 })).unwrap(), 5005);
        assert!(port_arg(&json!({ "port": 0 })).is_err());
        assert!(port_arg(&json!({ "port": 70000 })).is_err());
        assert!(port_arg(&json!({})).is_err());
        assert_eq!(line_arg(&json!({ "line": 42 })).unwrap(), 42);
        assert!(line_arg(&json!({ "line": 0 })).is_err());
        assert!(line_arg(&json!({ "line": -3 })).is_err());
    }

    #[test]
    fn evaluate_is_documented_as_reading_only() {
        let s = specs();
        let eval = s.iter().find(|t| t.name == "debug_evaluate").unwrap();
        assert!(eval.description.contains("refused") && eval.description.contains("nothing is run"));
    }
}
