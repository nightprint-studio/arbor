//! `Dispatcher` — assembles a backend's method → handler routing, including the
//! method-name union and the multi-context dispatch, so the product only declares
//! its handler *groups*.
//!
//! A backend's handlers don't all share one context type: the `#[handler]`
//! inventory downcasts the type-erased `&dyn Any` to the primary state `&S`, while
//! a reusable bundle (e.g. `arbor-plugin-rpc`'s `PluginRpc`) downcasts to its own
//! adapter, built fresh per call. The dispatcher models that as the **primary**
//! inventory (ctx = `&S`, borrowed from a shared `Arc<S>`) plus any number of
//! **extra groups**, each carrying a `make_ctx` that produces an owned context
//! boxed as `dyn Any` per request. The right `&dyn Any` reaches each handler with
//! no per-call branching in the product.

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use arbor_rpc::{async_registry_for, registry_for, AsyncCallFn, CallFn};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The reserved method every backend answers with its AI-tool self-description.
///
/// Double-underscored because it is host plumbing, not product surface — the same
/// convention as the shell's `__open_path` / `__set_config` reverse-channel methods. It
/// rides the ordinary request path, so discovering a backend's tools needs no protocol
/// change and no widening of the `Hello` frame (which must stay exactly what it is:
/// the first frame on the wire, with nothing allowed to precede it).
pub const TOOLS_METHOD: &str = "__tools";

/// The reserved method a backend answers with **what its own memory is holding** — the process
/// monitor's per-backend breakdown.
///
/// Reserved for the same reasons as [`TOOLS_METHOD`]: host plumbing on the ordinary request path, no
/// protocol change. Unlike it, it is **opt-in**. Describing tools needs no state, so every backend
/// can answer; describing memory means walking the product's own structures, which only the product
/// knows how to do. So a dispatcher advertises it only once [`Dispatcher::memory`] has given it a
/// reporter — and that is what lets the monitor tell "this backend does not measure itself" from
/// "it measured, and holds nothing worth listing".
pub const MEMORY_METHOD: &str = "__memory";

/// One thing a backend is holding in memory, as the process monitor lists it.
///
/// **An estimate unless it says otherwise.** There is no allocator keeping per-category statistics,
/// so a product sizes its structures by walking them: exact where that is a sum of lengths (sources
/// held as text), approximate where it is entries × the size of the structure plus the strings they
/// own. The monitor shows the backend's measured total beside the items, so the gap is visible
/// rather than hidden.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MemoryItem {
    /// What the memory belongs to: a project root, or `"process"` for what the backend holds once
    /// whatever is open.
    pub scope: String,
    /// What it is, in words a person reads — "Reference index", "Sources held as text".
    pub label: String,
    /// Bytes, when the structure can be sized. `None` when it can only be counted cheaply: sizing a
    /// deeply nested structure would mean serialising it, which costs more than it tells.
    pub bytes: Option<u64>,
    /// How many entries it holds, when that says something the bytes do not.
    pub count: Option<u64>,
    /// `bytes` is a sum of lengths, not an estimate.
    pub exact: bool,
    /// Memory-mapped files. They count towards the process's resident size, but the operating system
    /// can take the pages back whenever it wants them — so they are not memory the backend is
    /// *keeping* in the sense the rest of the list is.
    pub mapped: bool,
}

impl MemoryItem {
    /// A structure sized by walking it — an estimate.
    pub fn estimate(scope: impl Into<String>, label: impl Into<String>, count: usize, bytes: usize) -> Self {
        Self::new(scope, label, Some(bytes), Some(count), false)
    }

    /// A sum of lengths actually held — text, sample buffers, a Lua heap's own count.
    pub fn exact(scope: impl Into<String>, label: impl Into<String>, count: usize, bytes: usize) -> Self {
        Self::new(scope, label, Some(bytes), Some(count), true)
    }

    /// A structure only counted: sizing it would cost more than it tells.
    pub fn counted(scope: impl Into<String>, label: impl Into<String>, count: usize) -> Self {
        Self::new(scope, label, None, Some(count), false)
    }

    fn new(
        scope: impl Into<String>,
        label: impl Into<String>,
        bytes: Option<usize>,
        count: Option<usize>,
        exact: bool,
    ) -> Self {
        Self {
            scope: scope.into(),
            label: label.into(),
            bytes: bytes.map(|b| b as u64),
            count: count.map(|c| c as u64),
            exact,
            mapped: false,
        }
    }
}

/// The scope of what a backend holds once, whatever is open — [`MemoryItem::scope`]'s other value.
pub const PROCESS_SCOPE: &str = "process";

/// A backend's whole [`MEMORY_METHOD`] answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MemoryReport {
    pub items: Vec<MemoryItem>,
}

/// [`TOOLS_METHOD`]'s body: every `#[handler(mcp(...))]` this binary links.
///
/// Ignores the context — a backend describing itself needs no state — which is what
/// lets it be a plain fn pointer registered for any `S`.
fn describe_tools(_ctx: &(dyn Any + 'static), _params: Value) -> Result<Value, String> {
    serde_json::to_value(arbor_rpc::tools()).map_err(|e| e.to_string())
}

/// [`crate::focus::FOCUS_METHOD`]'s body: record whether any window still has the OS focus.
///
/// Registered for every backend for the same reason `__tools` is — a backend that had to opt in
/// is a backend that silently keeps emitting into a window nobody is looking at. Stateless, so
/// like `describe_tools` it is a plain fn pointer valid for any `S`.
///
/// Tolerant on the way in: anything that is not `{ "focused": <bool> }` (a bare boolean, an older
/// shell, a typo) reads as focused, which is the behaviour this replaced.
fn set_focus(_ctx: &(dyn Any + 'static), params: Value) -> Result<Value, String> {
    let focused = params
        .get("focused")
        .or(Some(&params))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    crate::focus::set_app_focused(focused);
    Ok(Value::Null)
}

/// One extra handler group: its method map + a factory for the per-call context
/// the group's handlers downcast to.
struct ExtraGroup {
    map: HashMap<&'static str, CallFn>,
    make_ctx: Box<dyn Fn() -> Box<dyn Any> + Send + Sync>,
}

/// Routes a method+params to the right handler. `S` is the **primary** context
/// type — the one the `#[handler]` inventory downcasts to.
pub struct Dispatcher<S: 'static> {
    state: Arc<S>,
    handle: tokio::runtime::Handle,
    sync: HashMap<&'static str, CallFn>,
    asyncs: HashMap<&'static str, AsyncCallFn>,
    extra: Vec<ExtraGroup>,
    /// The product's [`MEMORY_METHOD`] reporter, when it has one. A closure over the state rather
    /// than a `CallFn`: a fn pointer cannot carry the product's knowledge of its own structures, and
    /// the method exists precisely to carry that.
    memory: Option<Arc<dyn Fn(&S) -> MemoryReport + Send + Sync>>,
    /// Lines the runtime adds to any backend's answer — what `arbor-be` itself hosts, the plugin
    /// VMs, which no product reporter should have to know how to size.
    memory_extras: Vec<MemoryExtra>,
}

type MemoryExtra = Arc<dyn Fn() -> Vec<MemoryItem> + Send + Sync>;

impl<S: 'static> Dispatcher<S> {
    /// A dispatcher whose primary context is `state`. Async handlers are awaited on
    /// `handle` (a serve-loop worker thread drives the `block_on`, never the
    /// runtime itself).
    ///
    /// Every dispatcher answers [`TOOLS_METHOD`] out of the box — the self-description
    /// the host reads to build its AI tool surface. It is registered here rather than
    /// left to each product because a backend that forgets it is not a backend with one
    /// missing method, it is a backend that is silently invisible to the tool layer.
    pub fn new(state: Arc<S>, handle: tokio::runtime::Handle) -> Self {
        let mut sync: HashMap<&'static str, CallFn> = HashMap::new();
        sync.insert(TOOLS_METHOD, describe_tools);
        sync.insert(crate::focus::FOCUS_METHOD, set_focus);
        Self {
            state,
            handle,
            sync,
            asyncs: HashMap::new(),
            extra: Vec::new(),
            memory: None,
            memory_extras: Vec::new(),
        }
    }

    /// Answer [`MEMORY_METHOD`] with `reporter`.
    ///
    /// Called on demand — when someone opens a backend's breakdown in the process monitor — never on
    /// a timer, so a reporter may walk what it needs to. It should still count rather than
    /// serialise: a report that allocates gigabytes to say how much memory is in use has answered
    /// its own question badly.
    pub fn memory(mut self, reporter: impl Fn(&S) -> MemoryReport + Send + Sync + 'static) -> Self {
        self.memory = Some(Arc::new(reporter));
        self
    }

    /// Append `extra`'s lines to the [`MEMORY_METHOD`] answer, and advertise the method even when
    /// the product has no reporter of its own. For the runtime's own holdings (see
    /// [`crate::App::run`]), so every backend reports them without each product repeating it.
    pub(crate) fn memory_extra(mut self, extra: impl Fn() -> Vec<MemoryItem> + Send + Sync + 'static) -> Self {
        self.memory_extras.push(Arc::new(extra));
        self
    }

    fn reports_memory(&self) -> bool {
        self.memory.is_some() || !self.memory_extras.is_empty()
    }

    fn memory_report(&self) -> MemoryReport {
        let mut report = self.memory.as_ref().map(|r| r(&self.state)).unwrap_or_default();
        for extra in &self.memory_extras {
            report.items.extend(extra());
        }
        report
    }

    /// Add every `#[handler]` registered under `program` (sync + async),
    /// dispatched with the primary `&S`. Use `""` for the default program.
    pub fn inventory(mut self, program: &str) -> Self {
        for (n, f) in registry_for(program) {
            self.sync.insert(n, f);
        }
        for (n, f) in async_registry_for(program) {
            self.asyncs.insert(n, f);
        }
        self
    }

    /// Add an extra sync handler group with its **own** per-call context. `make`
    /// builds the context fresh for each request; the group's handlers downcast
    /// the type-erased `&dyn Any` back to `C` (e.g. an RPC adapter over the state).
    pub fn group<C: 'static>(
        mut self,
        map: HashMap<&'static str, CallFn>,
        make: impl Fn() -> C + Send + Sync + 'static,
    ) -> Self {
        self.extra.push(ExtraGroup {
            map,
            // Explicit return so `Box<C>` coerces to `Box<dyn Any>` here — a
            // `Fn() -> Box<C>` would not coerce to `Fn() -> Box<dyn Any>`.
            make_ctx: Box::new(move || -> Box<dyn Any> { Box::new(make()) }),
        });
        self
    }

    /// Every advertised method name (primary ∪ extra groups), sorted + deduped —
    /// the `Hello` payload.
    pub fn methods(&self) -> Vec<String> {
        let mut m: Vec<String> = self
            .sync
            .keys()
            .chain(self.asyncs.keys())
            .chain(self.extra.iter().flat_map(|g| g.map.keys()))
            // Advertised only with a reporter behind it — see [`MEMORY_METHOD`].
            .chain(self.reports_memory().then_some(&MEMORY_METHOD))
            .map(|s| s.to_string())
            .collect();
        m.sort();
        m.dedup();
        m
    }

    /// Consume into the dispatch closure the serve loop calls. Primary handlers
    /// get `&S`; async ones `block_on` the runtime handle; extra-group handlers get
    /// a freshly-built `&C`.
    pub fn into_fn(self) -> impl Fn(&str, Value) -> Result<Value, String> + Send + Sync + 'static
    where
        S: Send + Sync,
    {
        move |method: &str, params: Value| {
            if method == MEMORY_METHOD && self.reports_memory() {
                return serde_json::to_value(self.memory_report()).map_err(|e| e.to_string());
            }
            if let Some(call) = self.sync.get(method) {
                return call(&*self.state as &dyn Any, params);
            }
            if let Some(acall) = self.asyncs.get(method) {
                return self.handle.block_on(acall(&*self.state as &dyn Any, params));
            }
            for g in &self.extra {
                if let Some(call) = g.map.get(method) {
                    let ctx = (g.make_ctx)();
                    return call(&*ctx as &dyn Any, params);
                }
            }
            Err(format!("unknown method: {method}"))
        }
    }
}
