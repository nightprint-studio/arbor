//! Shim — the real implementation moved to
//! [`arbor_plugin_core::contribution`] in PR #4.
//!
//! API note for the migration: `notify_changed` / `notify_containers_changed`
//! no longer take a `&Option<AppHandle>` argument. The registry now stores
//! the host `AppCtx` internally (installed once at app boot via
//! [`ContributionRegistry::install_app_ctx`]), and the notify methods emit
//! through that. Call sites that used to pass a handle drop the argument.
//!
//! Removed in the final cleanup step of PR #4.

pub use arbor_plugin_core::contribution::*;
