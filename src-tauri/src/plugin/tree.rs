//! Shim — the real implementation moved to
//! [`arbor_plugin_core::tree`] in PR #4. Kept as a thin `pub use` so the
//! ~22 existing call sites under `src/plugin/**` keep compiling without
//! per-import churn. Removed in the final cleanup step of PR #4 once those
//! call sites migrate to `arbor_plugin_core::prelude::*`.

pub use arbor_plugin_core::tree::*;
