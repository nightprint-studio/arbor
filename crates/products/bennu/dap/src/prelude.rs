//! Canonical entry point for `bennu-dap`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through `bennu_dap::prelude::...`.
//! The submodules stay `pub` for rustdoc navigation, but the prelude is the canonical call-site path.

// Which adapter, and where.
pub use crate::discovery::{resolve, spec_by_id, survey, Adapter, AdapterSpec, Engine, ADAPTERS};

// What it can show you, and how it reads an expression.
pub use crate::rendering::{
    evaluators, init_commands, launch_extras, rendering, split_dialect, toolchain_etc, Evaluator,
    RustRendering,
};

// The process, and one session on it.
pub use crate::client::{AdapterHandler, DapClient, DapError, Pending};
pub use crate::session::{Launch, Session, SessionHandler, StepDepth};

// The envelope.
pub use crate::protocol::{AdapterRequest, Event, Incoming, Message, Outgoing, Response, Seq};

// What the requests take and the answers carry.
pub use crate::types::{
    Breakpoint, BreakpointEvent, Capabilities, ContinuedEvent, EvaluateArguments, EvaluateBody,
    ExceptionFilter, ExitedEvent, InitializeArguments, OutputEvent, Scope, ScopesBody,
    SetBreakpointsArguments, SetBreakpointsBody, Source, SourceBreakpoint, StackFrame,
    StackTraceArguments, StackTraceBody, StoppedEvent, TerminatedEvent, Thread, ThreadEvent,
    ThreadsBody, Variable, VariablesArguments, VariablesBody,
};
