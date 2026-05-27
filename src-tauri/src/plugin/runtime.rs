//! Shim — the runtime modules (`consts`, `loaded`, `manifest/*`,
//! `scheduler/*`, `host/*`) moved to [`arbor_plugin_core::runtime`] in
//! PR #4 (session 3). Removed in the final cleanup step of PR #4.

#[allow(unused_imports)]
pub use arbor_plugin_core::runtime::*;

// Sub-namespace re-exports so existing call sites that reach for
// `crate::plugin::runtime::manifest::*` / `crate::plugin::runtime::host::*`
// keep resolving. The pub-use of the host module also gives downstream
// consumers access to `dep_cascade::{EnableBlocker, EnablePreview}`.
#[allow(unused_imports)]
pub use arbor_plugin_core::runtime::{host, manifest, scheduler};
