//! The DAP shapes Bennu actually uses.
//!
//! A bounded subset, deliberately, and the same judgement `bennu-lsp`'s `types.rs` makes: the spec
//! has around sixty requests and Bennu's debugger asks eleven of them, the mapping onto Bennu's own
//! wire types has to be written either way, and a crate tracking the whole spec is a breaking-minor
//! treadmill for surface nothing touches.
//!
//! ## Everything is optional, because adapters differ
//!
//! DAP is one spec with a dozen implementations, and the ones Bennu talks to disagree about which
//! optional fields they fill in. `codelldb` sends a `Source` with `path`; `lldb-dap` sometimes sends
//! one with only a `name`; `gdb` in DAP mode omits `column` on frames it cannot place. So every field
//! that the spec marks optional is `Option` here even when every adapter we know of sends it — a
//! missing field must degrade the view, never fail the parse and take the whole stack with it.
//!
//! `#[serde(default)]` throughout for the same reason, and `deny_unknown_fields` nowhere: an adapter
//! adding a field of its own must not break a client that does not read it.

use serde::{Deserialize, Serialize};

// ── the handshake ─────────────────────────────────────────────────────────────

/// What we tell the adapter about ourselves.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeArguments {
    /// Free-form, and it shows up in adapter logs — worth being recognisable in a bug report.
    ///
    /// **`clientID`, not `clientId`.** The spec capitalises the `ID` in this field and in
    /// `adapterID`, and nowhere else in the whole protocol — so `rename_all = "camelCase"` gets both
    /// of them wrong, and only one of them fails loudly. See [`InitializeArguments::for_adapter`].
    #[serde(rename = "clientID")]
    pub client_id: String,
    pub client_name: String,
    /// **Required**, and `adapterID` with a capital `ID`. Sending `adapterId` makes an adapter refuse
    /// `initialize` with "missing value at arguments.adapterID", which is the whole session.
    #[serde(rename = "adapterID")]
    pub adapter_id: String,
    /// `path` or `uri`. **`path`**: every offset and file Bennu has is a path, and asking for URIs
    /// would mean converting in both directions for no gain.
    pub path_format: String,
    /// One-based, both. Which is what an editor gutter counts in, and the alternative is an
    /// off-by-one in every jump.
    pub lines_start_at1: bool,
    pub columns_start_at1: bool,
    /// We can render a variable's type, so ask for it.
    pub supports_variable_type: bool,
    /// We cannot yet run a command in a terminal on the adapter's behalf, and claiming otherwise
    /// makes an adapter choose a launch mode we would then have to refuse. See `runInTerminal`.
    pub supports_run_in_terminal_request: bool,
    /// We do handle the progress events, as console output.
    pub supports_progress_reporting: bool,
}

impl InitializeArguments {
    /// The handshake arguments for a named adapter.
    ///
    /// `adapterID` is the **adapter's** id, not ours — `codelldb`, `lldb-dap`, `gdb`. Some adapters
    /// key their own behaviour on it, so passing a placeholder is passing the wrong answer to a
    /// question that was about them.
    pub fn for_adapter(adapter_id: &str) -> Self {
        InitializeArguments { adapter_id: adapter_id.to_string(), ..Self::default() }
    }
}

impl Default for InitializeArguments {
    fn default() -> Self {
        InitializeArguments {
            client_id: "arbor-bennu".to_string(),
            client_name: "Bennu".to_string(),
            adapter_id: "bennu".to_string(),
            path_format: "path".to_string(),
            lines_start_at1: true,
            columns_start_at1: true,
            supports_variable_type: true,
            supports_run_in_terminal_request: false,
            supports_progress_reporting: true,
        }
    }
}

/// What the adapter says it can do.
///
/// Only the ones that change what Bennu *sends* are here. A capability we read but never branch on
/// would be a field to keep in step for nothing.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Capabilities {
    /// Whether the configuration handshake ends with `configurationDone`. Sending it to an adapter
    /// that did not ask for it is an error on several of them; **not** sending it to one that did
    /// leaves the debuggee suspended forever, which is the worse failure and the one that looks like
    /// a hang.
    pub supports_configuration_done_request: bool,
    /// Whether a breakpoint may carry a condition.
    pub supports_conditional_breakpoints: bool,
    /// Whether a breakpoint may carry a hit count.
    pub supports_hit_conditional_breakpoints: bool,
    /// Whether `terminate` exists. When it does it is the polite stop — it lets the debuggee run its
    /// exit path — and `disconnect` is the blunt one.
    pub supports_terminate_request: bool,
    /// Whether `evaluate` may be asked for a hover.
    pub supports_evaluate_for_hovers: bool,
    /// Whether a variable can be assigned to.
    pub supports_set_variable: bool,
    /// Whether a stack trace may be asked for in pages. Without it, `startFrame`/`levels` are
    /// ignored and a runaway recursion sends its whole stack.
    pub supports_delayed_stack_trace_loading: bool,
    /// Whether `stepInTargets` exists — which call to step into, on a line with several.
    pub supports_step_in_targets_request: bool,
    /// The exception categories this adapter offers, if any. Rust's are not Java's: `lldb` offers
    /// `rust_panic`, not "caught / uncaught throwable".
    pub exception_breakpoint_filters: Vec<ExceptionFilter>,
}

/// One exception category an adapter can stop on.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExceptionFilter {
    pub filter: String,
    pub label: String,
    pub description: Option<String>,
    /// Whether the adapter suggests it be on by default.
    pub default: bool,
}

// ── source, breakpoints ───────────────────────────────────────────────────────

/// A file, as DAP refers to one.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Source {
    /// The file name only, sometimes all an adapter gives for a frame it cannot place on disk.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The absolute path. Present for anything the debuggee has debug info for.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// A handle for source the adapter can produce but that is not on disk — a decompilation, a
    /// generated file. Bennu does not fetch these yet; carried so a frame holding one is still a
    /// frame rather than a parse failure.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_reference: Option<i64>,
}

/// A breakpoint we ask for, in the shape `setBreakpoints` takes.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceBreakpoint {
    pub line: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    /// An expression the adapter evaluates in the debuggee; it stops only when it is true. Sent only
    /// when the adapter said it supports them — see [`Capabilities`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hit_condition: Option<String>,
}

/// What the adapter made of a breakpoint.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Breakpoint {
    pub id: Option<i64>,
    /// Whether it will actually stop the program. `false` with no message is the useless answer some
    /// adapters give before the module is loaded; it resolves itself, and Bennu says so.
    pub verified: bool,
    /// Why it is not verified, or where it really bound. "That line has no code" reads very
    /// differently from "the module is not loaded yet".
    pub message: Option<String>,
    pub source: Option<Source>,
    /// Where it ended up — an adapter is entitled to move a breakpoint to the next line that has
    /// code, and a gutter that keeps drawing it where it was clicked is lying.
    pub line: Option<u32>,
    pub column: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetBreakpointsArguments {
    pub source: Source,
    pub breakpoints: Vec<SourceBreakpoint>,
    /// Whether the lines are the ones the user sees. Always true for us.
    pub source_modified: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct SetBreakpointsBody {
    /// One per requested breakpoint, **in the order asked**. The spec is explicit about that, and it
    /// is the only way to map an answer back onto the line that produced it.
    pub breakpoints: Vec<Breakpoint>,
}

// ── threads and frames ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(default)]
pub struct Thread {
    pub id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ThreadsBody {
    pub threads: Vec<Thread>,
}

/// One frame of a stopped thread's stack.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StackFrame {
    /// The adapter's handle, valid only while the thread stays stopped.
    pub id: i64,
    /// One string, and this is where DAP and JDWP genuinely differ: there is no class/method split
    /// to be had. Rust's is `geode::mine::dig`, and a synthetic frame's is
    /// `core::ops::function::FnOnce::call_once{{vtable.shim}}` — which is why splitting it at the
    /// last `::` to invent a "class" produces nonsense on exactly the frames that need reading.
    pub name: String,
    pub source: Option<Source>,
    pub line: u32,
    pub column: u32,
    /// `normal` · `label` · `subtle`. A `label` frame is not real code — it is a marker the adapter
    /// inserted, like a thread boundary — and stepping to it means nothing.
    pub presentation_hint: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StackTraceArguments {
    pub thread_id: i64,
    pub start_frame: u32,
    /// 0 means "all of them" to most adapters, and is ignored entirely by one that did not claim
    /// `supportsDelayedStackTraceLoading`.
    pub levels: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StackTraceBody {
    pub stack_frames: Vec<StackFrame>,
    /// The real depth, when the adapter knows it and paged. Absent means "what you got is all of it".
    pub total_frames: Option<u32>,
}

// ── scopes and variables ──────────────────────────────────────────────────────

/// A group of variables in a frame — `Locals`, `Statics`, `Registers`.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Scope {
    pub name: String,
    pub variables_reference: i64,
    /// `arguments` · `locals` · `registers`. Set by adapters that bother; the name is the fallback.
    pub presentation_hint: Option<String>,
    /// Whether opening it is slow enough to be worth not doing automatically. Registers usually are.
    pub expensive: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct ScopesBody {
    pub scopes: Vec<Scope>,
}

/// One variable, field or element.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Variable {
    pub name: String,
    /// Already rendered by the adapter. Bennu shows it verbatim: the adapter knows how to print a
    /// `Vec<T>` and a second opinion here would be a worse one.
    pub value: String,
    /// Only sent when the client asked for it — see [`InitializeArguments`].
    #[serde(rename = "type")]
    pub type_name: Option<String>,
    /// Non-zero when there is something inside. The handle to pass back to `variables`, and it is
    /// valid only while the thread stays stopped.
    pub variables_reference: i64,
    /// How many of the children are named fields, when the adapter counted them.
    pub named_variables: Option<u32>,
    /// …and how many are indexed elements. Together they are what says "this is a collection of
    /// 4000 things" before any of them is fetched.
    pub indexed_variables: Option<u32>,
    /// An expression that would evaluate to this value, when it is not simply the name — what makes
    /// "add this to the watches" possible on a nested field.
    pub evaluate_name: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VariablesArguments {
    pub variables_reference: i64,
    /// `named` · `indexed`, or absent for both.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<u32>,
    /// A cap, because a `Vec` with a million elements is a legal thing to have stopped inside.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct VariablesBody {
    pub variables: Vec<Variable>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateArguments {
    pub expression: String,
    /// Which frame to evaluate in. Absent means the global scope, which for a debugger panel is
    /// almost never what was meant.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame_id: Option<i64>,
    /// `watch` · `repl` · `hover` · `clipboard`. The adapter is entitled to behave differently:
    /// `repl` may have side effects, `watch` should not.
    pub context: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EvaluateBody {
    pub result: String,
    #[serde(rename = "type")]
    pub type_name: Option<String>,
    pub variables_reference: i64,
    pub named_variables: Option<u32>,
    pub indexed_variables: Option<u32>,
}

// ── events ────────────────────────────────────────────────────────────────────

/// The debuggee stopped.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct StoppedEvent {
    /// `step` · `breakpoint` · `exception` · `pause` · `entry` · … Free-form by spec: an adapter may
    /// invent one, and the reason is shown rather than matched on wherever that is possible.
    pub reason: String,
    /// Which thread. Optional in the spec — and an adapter that omits it means "all of them", which
    /// leaves a client with no thread to ask for a stack trace. Bennu falls back to the first thread.
    pub thread_id: Option<i64>,
    /// Prose for the user, when the reason alone is not enough. This is where a panic message lands.
    pub description: Option<String>,
    pub text: Option<String>,
    /// Whether every thread stopped, not just this one.
    pub all_threads_stopped: bool,
    /// The breakpoints that caused it, by id.
    pub hit_breakpoint_ids: Vec<i64>,
}

/// The debuggee is running again.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ContinuedEvent {
    pub thread_id: i64,
    pub all_threads_continued: bool,
}

/// The debuggee wrote something.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OutputEvent {
    /// `console` · `stdout` · `stderr` · `important` · `telemetry`. `telemetry` is the adapter
    /// talking to its vendor and is dropped rather than shown.
    pub category: Option<String>,
    pub output: String,
    /// Where in the source it came from, for an adapter that tracks that.
    pub source: Option<Source>,
    pub line: Option<u32>,
}

/// The debuggee exited, with its code.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ExitedEvent {
    pub exit_code: i64,
}

/// The debug session is over. `restart` asks the client to start another one — Bennu does not, and
/// treats it as a plain end.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TerminatedEvent {
    pub restart: Option<serde_json::Value>,
}

/// A thread started or exited.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ThreadEvent {
    /// `started` · `exited`.
    pub reason: String,
    pub thread_id: i64,
}

/// A breakpoint changed after the fact — bound to a real line once the module loaded, or lost.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct BreakpointEvent {
    /// `changed` · `new` · `removed`.
    pub reason: String,
    pub breakpoint: Breakpoint,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole reason every field is optional: an adapter that omits half of them must still
    /// produce a usable frame rather than taking the stack down with it.
    #[test]
    fn a_frame_with_only_the_required_fields_parses() {
        let f: StackFrame =
            serde_json::from_str(r#"{"id":1,"name":"main","line":10,"column":5}"#).unwrap();
        assert_eq!(f.name, "main");
        assert!(f.source.is_none(), "a frame with no source is a library frame, not an error");
        assert!(f.presentation_hint.is_none());
    }

    #[test]
    fn a_source_with_only_a_name_parses() {
        let s: Source = serde_json::from_str(r#"{"name":"mod.rs"}"#).unwrap();
        assert_eq!(s.name.as_deref(), Some("mod.rs"));
        assert!(s.path.is_none());
    }

    /// An adapter adding fields of its own must not break a client that does not read them.
    #[test]
    fn unknown_fields_are_ignored_rather_than_refused() {
        let v: Variable = serde_json::from_str(
            r#"{"name":"n","value":"7","variablesReference":0,"__lldb_extra":{"x":1},"memoryReference":"0x1"}"#,
        )
        .unwrap();
        assert_eq!(v.value, "7");
    }

    #[test]
    fn a_variable_reports_what_is_inside_it_before_it_is_opened() {
        let v: Variable = serde_json::from_str(
            r#"{"name":"items","value":"Vec<i32>(size=4000)","type":"alloc::vec::Vec<i32>","variablesReference":42,"indexedVariables":4000}"#,
        )
        .unwrap();
        assert_eq!(v.variables_reference, 42);
        assert_eq!(v.indexed_variables, Some(4000));
        assert_eq!(v.type_name.as_deref(), Some("alloc::vec::Vec<i32>"));
    }

    #[test]
    fn a_leaf_variable_has_no_handle() {
        let v: Variable =
            serde_json::from_str(r#"{"name":"n","value":"7","variablesReference":0}"#).unwrap();
        assert_eq!(v.variables_reference, 0, "zero is the spec's way of saying `nothing inside`");
    }

    /// `thread_id` is optional in the spec, and an adapter that omits it leaves a client with no
    /// thread to ask about — worth being explicit that the parse survives it.
    #[test]
    fn a_stopped_event_without_a_thread_still_parses() {
        let e: StoppedEvent = serde_json::from_str(r#"{"reason":"breakpoint"}"#).unwrap();
        assert_eq!(e.reason, "breakpoint");
        assert_eq!(e.thread_id, None);
    }

    #[test]
    fn a_panic_stop_carries_its_message() {
        let e: StoppedEvent = serde_json::from_str(
            r#"{"reason":"exception","threadId":1,"description":"panicked at 'index out of bounds'","allThreadsStopped":true}"#,
        )
        .unwrap();
        assert_eq!(e.thread_id, Some(1));
        assert!(e.description.unwrap().contains("index out of bounds"));
        assert!(e.all_threads_stopped);
    }

    #[test]
    fn capabilities_default_to_unsupported_when_the_adapter_says_nothing() {
        let c: Capabilities = serde_json::from_str("{}").unwrap();
        assert!(!c.supports_configuration_done_request);
        assert!(!c.supports_terminate_request);
        assert!(c.exception_breakpoint_filters.is_empty());
    }

    #[test]
    fn the_exception_filters_an_adapter_offers_come_through() {
        let c: Capabilities = serde_json::from_str(
            r#"{"supportsConfigurationDoneRequest":true,"exceptionBreakpointFilters":[{"filter":"rust_panic","label":"Rust: panic","default":true}]}"#,
        )
        .unwrap();
        assert!(c.supports_configuration_done_request);
        assert_eq!(c.exception_breakpoint_filters.len(), 1);
        assert_eq!(c.exception_breakpoint_filters[0].filter, "rust_panic");
        assert!(c.exception_breakpoint_filters[0].default);
    }

    #[test]
    fn a_breakpoint_the_adapter_moved_says_where_it_went() {
        let b: Breakpoint = serde_json::from_str(
            r#"{"id":3,"verified":true,"line":42,"source":{"path":"/p/src/main.rs"}}"#,
        )
        .unwrap();
        assert!(b.verified);
        assert_eq!(b.line, Some(42));
        assert_eq!(b.source.unwrap().path.as_deref(), Some("/p/src/main.rs"));
    }

    #[test]
    fn an_unverified_breakpoint_carries_its_reason() {
        let b: Breakpoint =
            serde_json::from_str(r#"{"verified":false,"message":"no code at that line"}"#).unwrap();
        assert!(!b.verified);
        assert_eq!(b.message.as_deref(), Some("no code at that line"));
    }

    /// Absent optional arguments are omitted, not sent as null: several adapters validate strictly
    /// and reject a null where they expect a number.
    #[test]
    fn optional_arguments_are_omitted_rather_than_nulled() {
        let args = VariablesArguments { variables_reference: 7, ..VariablesArguments::default() };
        let json = serde_json::to_string(&args).unwrap();
        assert_eq!(json, r#"{"variablesReference":7}"#);

        let bp = SourceBreakpoint { line: 12, ..SourceBreakpoint::default() };
        assert_eq!(serde_json::to_string(&bp).unwrap(), r#"{"line":12}"#);
    }

    /// The two fields the spec capitalises, and the one that is required.
    ///
    /// Pinned as an exact key set because the failure is asymmetric: `clientID` is optional, so
    /// misspelling it does nothing visible, while `adapterID` is required and misspelling it makes
    /// every adapter refuse `initialize` — the whole session, on the first message.
    #[test]
    fn the_two_capitalised_id_fields_are_spelled_the_way_the_spec_spells_them() {
        let json = serde_json::to_value(InitializeArguments::for_adapter("codelldb")).unwrap();
        let object = json.as_object().unwrap();

        assert_eq!(json["adapterID"], "codelldb", "required, and capital ID: {json}");
        assert_eq!(json["clientID"], "arbor-bennu", "capital ID here too: {json}");
        // …and the camelCase spellings must NOT be there: an adapter reading `adapterID` and finding
        // only `adapterId` reports it as missing, which is exactly what happened.
        assert!(!object.contains_key("adapterId"), "{json}");
        assert!(!object.contains_key("clientId"), "{json}");
        // Everything else in this request IS plain camelCase, so the rename is per-field on purpose
        // rather than a different convention for the whole struct.
        assert!(object.contains_key("clientName"), "{json}");
        assert!(object.contains_key("pathFormat"), "{json}");
    }

    #[test]
    fn what_we_tell_the_adapter_about_ourselves_is_one_based_and_path_shaped() {
        let json = serde_json::to_value(InitializeArguments::default()).unwrap();
        assert_eq!(json["pathFormat"], "path");
        assert_eq!(json["linesStartAt1"], true);
        assert_eq!(json["columnsStartAt1"], true);
        // Claiming a capability we do not have makes the adapter pick a launch mode we must refuse.
        assert_eq!(json["supportsRunInTerminalRequest"], false);
        assert_eq!(json["supportsVariableType"], true);
    }

    #[test]
    fn a_source_serialises_without_the_fields_it_does_not_have() {
        let s = Source { path: Some("/p/src/main.rs".into()), ..Source::default() };
        assert_eq!(serde_json::to_string(&s).unwrap(), r#"{"path":"/p/src/main.rs"}"#);
    }
}
