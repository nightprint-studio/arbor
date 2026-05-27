//! Shim — the real implementation moved to
//! [`arbor_plugin_core::event_bus`] in PR #4. The API now takes
//! `&dyn AppCtx` instead of `&AppHandle`; the `pub use` below carries the
//! new signature through. Removed in the final cleanup step of PR #4.

#[allow(unused_imports)]
pub use arbor_plugin_core::event_bus::*;
