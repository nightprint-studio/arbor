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
use serde_json::Value;

/// The reserved method every backend answers with its AI-tool self-description.
///
/// Double-underscored because it is host plumbing, not product surface — the same
/// convention as the shell's `__open_path` / `__set_config` reverse-channel methods. It
/// rides the ordinary request path, so discovering a backend's tools needs no protocol
/// change and no widening of the `Hello` frame (which must stay exactly what it is:
/// the first frame on the wire, with nothing allowed to precede it).
pub const TOOLS_METHOD: &str = "__tools";

/// [`TOOLS_METHOD`]'s body: every `#[handler(mcp(...))]` this binary links.
///
/// Ignores the context — a backend describing itself needs no state — which is what
/// lets it be a plain fn pointer registered for any `S`.
fn describe_tools(_ctx: &(dyn Any + 'static), _params: Value) -> Result<Value, String> {
    serde_json::to_value(arbor_rpc::tools()).map_err(|e| e.to_string())
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
}

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
        Self {
            state,
            handle,
            sync,
            asyncs: HashMap::new(),
            extra: Vec::new(),
        }
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
