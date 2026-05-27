//! Shim — the real implementation moved to
//! [`arbor_plugin_core::settings_store`] in PR #4. Kept as a thin `pub use`
//! so the existing call sites keep compiling without per-import churn.
//! Removed in the final cleanup step of PR #4.

pub use arbor_plugin_core::settings_store::*;
