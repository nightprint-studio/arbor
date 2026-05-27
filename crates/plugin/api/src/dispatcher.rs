//! `HookDispatcher` — the runtime broker of fired hooks.
//!
//! Decision **D8**: the dispatcher knows nothing about scripting runtimes.
//! It holds:
//!
//! - a metadata table of registered [`HookDef`]s (so it can answer "is this
//!   hook vetoable?" without a string match), and
//! - a vector of [`HookListener`]s — one per runtime adapter (one `LuaHookListener`
//!   today, one `WasmHookListener` tomorrow).
//!
//! When a domain crate fires `dispatcher.fire("on_mr_created", payload).await`
//! the dispatcher walks the listeners and lets each one decide how to fan out
//! to its plugins. Adding a new runtime = registering a new listener, no
//! changes here.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

use crate::hook::HookDef;
use crate::value::PluginValue;

/// One subscriber to the dispatcher's stream of fired hooks.
///
/// Implemented once per scripting runtime — the impl knows how to walk that
/// runtime's set of plugins and invoke their handlers. Both methods receive
/// the payload by reference so a single fired hook doesn't pay a clone per
/// listener; listeners that need to mutate must clone internally.
#[async_trait]
pub trait HookListener: Send + Sync {
    /// Fire-and-forget delivery. Errors raised by individual plugin handlers
    /// must be logged inside the listener — they never abort the chain and
    /// never propagate out.
    async fn fire(&self, name: &str, ctx: &PluginValue);

    /// Vetoable delivery. The listener invokes its subscribed plugins in
    /// order and returns `Some(reason)` for the first one that aborts; the
    /// dispatcher uses that to short-circuit the remaining listeners.
    async fn fire_vetoable(&self, name: &str, ctx: &PluginValue) -> Option<String>;
}

/// The broker. Cheap to clone the `Arc`s it holds — there's typically one
/// dispatcher per app.
pub struct HookDispatcher {
    hooks:     HashMap<&'static str, HookDef>,
    listeners: Vec<Arc<dyn HookListener>>,
}

impl HookDispatcher {
    pub fn new() -> Self {
        Self {
            hooks:     HashMap::new(),
            listeners: Vec::new(),
        }
    }

    /// Register a hook definition. Panics on duplicate names.
    pub fn register_hook(&mut self, h: HookDef) {
        if self.hooks.contains_key(h.name) {
            panic!("arbor-plugin-api: duplicate hook '{}'", h.name);
        }
        self.hooks.insert(h.name, h);
    }

    /// Register a new listener — typically one call per runtime at boot.
    pub fn register_listener(&mut self, l: Arc<dyn HookListener>) {
        self.listeners.push(l);
    }

    /// Look up a hook definition by name. Returns `None` for hook names the
    /// host doesn't know about (e.g. plugin-defined action hooks fired
    /// through `arbor.events.emit`).
    pub fn lookup(&self, name: &str) -> Option<&HookDef> {
        self.hooks.get(name)
    }

    /// Iterate every registered hook definition. Order is unspecified.
    pub fn iter_hooks(&self) -> impl Iterator<Item = &HookDef> {
        self.hooks.values()
    }

    /// Fire-and-forget delivery to every registered listener, sequentially.
    ///
    /// Sequential rather than concurrent on purpose — the typical listener
    /// dispatches into a single-threaded VM (mlua), so a `join_all` would
    /// just serialise inside the listener anyway, while obscuring ordering.
    pub async fn fire(&self, name: &str, ctx: PluginValue) {
        for l in &self.listeners {
            l.fire(name, &ctx).await;
        }
    }

    /// Vetoable delivery. Returns the first `Some(reason)` produced by any
    /// listener and skips the remaining ones; returns `None` if every
    /// listener lets the action proceed.
    pub async fn fire_vetoable(&self, name: &str, ctx: PluginValue) -> Option<String> {
        for l in &self.listeners {
            if let Some(reason) = l.fire_vetoable(name, &ctx).await {
                return Some(reason);
            }
        }
        None
    }
}

impl Default for HookDispatcher {
    fn default() -> Self {
        Self::new()
    }
}
