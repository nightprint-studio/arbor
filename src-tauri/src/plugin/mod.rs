//! Tauri-shell glue around `arbor-plugin-core`.
//!
//! After PR #4 the plugin runtime, sandbox, lifecycle, hook routing, and the
//! host-pure `arbor.*` namespaces all live in `arbor-plugin-core` (reach them
//! through [`arbor_plugin_core::prelude`]). What stays here is only the glue
//! that still needs src-tauri-internal types:
//!   · [`ns_shell`]      — the `arbor.*` namespaces that depend on `git::*`,
//!     `pipeline::*`, `jobs::*`, `terminal::*`, `workspace::*`, `brp::*`,
//!     `cloud::*`, … plus the `LuaNamespaceInstaller` wrappers and
//!     `shell_installers()` that wire them into the runtime at boot.
//!   · [`api_installer`] — the `LuaApiInstaller` adapter that hands
//!     `shell_installers()` to `arbor_plugin_core::prelude::register_lua_api`.
//!
//! As each shell namespace migrates into its own domain crate (PR #6+), its
//! wrapper drops out of `shell_installers()` and, once the list is empty,
//! this whole module disappears.

pub mod api_installer;
pub mod ns_shell;
