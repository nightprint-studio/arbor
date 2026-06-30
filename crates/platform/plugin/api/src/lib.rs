//! Runtime-agnostic plugin extension API.
//!
//! This crate gives every Arbor domain crate (git-provider, issue-tracker,
//! pipeline, …) a uniform way to contribute plugin-facing surface — namespaces
//! of functions, hook definitions, permission keys — without depending on any
//! concrete scripting runtime. The actual `mlua` (and tomorrow, `wasm`) adapter
//! lives in `arbor-plugin-core`; this crate intentionally knows nothing about
//! them.
//!
//! ## Shape of the API
//!
//! - [`PluginValue`](value::PluginValue) is the in-process bridging value type.
//!   Cheaper than `serde_json::Value` for hot paths, easier to translate to
//!   different runtimes than a generic type parameter.
//! - [`PluginRegistry`](registry::PluginRegistry) is the namespace + permission
//!   + hook collector. Each domain crate ships a [`NamespaceContributor`] that
//!     pours its surface into the registry at boot.
//! - [`HookDispatcher`](dispatcher::HookDispatcher) is the runtime broker that
//!   fans a fired hook out to every registered [`HookListener`]
//!   (one per scripting runtime).
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention — every Arbor library crate exposes its public surface
//! through a `prelude` module. Consumers either glob-import it once per file
//! (`use arbor_plugin_api::prelude::*;`) or fully qualify
//! (`arbor_plugin_api::prelude::PluginRegistry`). The per-feature submodules
//! (`value`, `registry`, `dispatcher`, …) stay `pub` for discoverability and
//! rustdoc navigation, but call sites should go through the prelude.
//!
//! See `docs/plugin-api-architecture.md` for the full design (decisions D1–D8,
//! roadmap, future PRs).

pub mod ctx;
pub mod dispatcher;
pub mod error;
pub mod func;
pub mod hook;
pub mod namespace;
pub mod perm;
pub mod prelude;
pub mod registry;
pub mod value;
