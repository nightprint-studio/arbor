//! Canonical entry point for `corvus-core`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `corvus_core::prelude::...`. The submodules stay `pub` for rustdoc navigation.

pub use crate::state::CorvusState;

// Re-exported because `CorvusState::hooks_handle` hands back an
// `Arc<HookDispatcher>` (and `PluginValue` is needed to fire onto it) — a
// background task firing hooks reaches both through this prelude without a
// direct `arbor-plugin-api` dependency.
pub use arbor_plugin_api::prelude::{HookDispatcher, PluginValue};
