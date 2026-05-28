//! `arbor.*` Lua API surface — Tauri-shell side.
//!
//! After PR #4 Step 5, the orchestrator (`register(...)`) and the shared
//! [`ApiCtx`] both live in [`arbor_plugin_core::lua_api`]. This module now
//! only keeps the shell-side glue that the migrated plugin-core code still
//! reaches back into:
//!   · [`ctx`] — shim re-exporting [`arbor_plugin_core::prelude::ApiCtx`]
//!     plus the `ApiCtxExt::app_handle()` accessor.
//!   · [`helpers`] — Tauri-flavoured conversion / tuple / fs-perm helpers
//!     shared by the shell-side namespace installers.
//!
//! The per-namespace installers that still need src-tauri-internal types
//! live in [`crate::plugin::ns_shell`], which also owns `shell_installers()`.

pub(crate) mod ctx;
pub(crate) mod helpers;
