//! Shim — the real implementation moved to
//! [`arbor_plugin_core::lua_ctx`] in PR #4. The stash now carries an
//! `Option<Arc<dyn AppCtx>>` instead of an `Option<AppHandle>`; the existing
//! `install` / `record` call sites continue to compile through this
//! re-export. Removed in the final cleanup step of PR #4.

pub use arbor_plugin_core::lua_ctx::*;
