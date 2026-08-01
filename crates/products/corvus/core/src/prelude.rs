//! Canonical entry point for `corvus-core`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `corvus_core::prelude::...`. The submodules stay `pub` for rustdoc navigation.

pub use crate::state::CorvusState;

// The module, not its contents: a call site reads `hooks::CHECKOUT`, which says
// what the string is, where a glob would drop bare `CHECKOUT` / `PUSH` / `COMMIT`
// into every handler's scope.
pub use crate::hooks;

// The cross-product `arbor:` hook names, re-exported as `arbor_hooks` so a corvus
// call site can name them without its own `arbor-plugin-types` dependency.
// `repo_open` / `repo_close` / `tab_switch` live there rather than under `corvus:`
// because the shell and `arbor-plugin-rpc` fire them too and have no product id —
// see the note in `crate::hooks`.
pub use arbor_plugin_types::prelude::hook_names::arbor as arbor_hooks;

// Re-exported because `CorvusState::hooks_handle` hands back an
// `Arc<HookDispatcher>` (and `PluginValue` is needed to fire onto it) — a
// background task firing hooks reaches both through this prelude without a
// direct `arbor-plugin-api` dependency.
pub use arbor_plugin_api::prelude::{HookDispatcher, PluginValue};
