//! Shim — the real implementation moved to
//! [`arbor_plugin_core::hook_registry`] in PR #4 (session 3). Both this
//! shim and the underlying module are slated for removal in session 7
//! when [`HookDispatcher`] + `LuaHookListener` supersede direct calls.

#[allow(unused_imports)]
pub use arbor_plugin_core::hook_registry::*;
