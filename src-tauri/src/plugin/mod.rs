//! Tauri-shell glue around `arbor-plugin-core`.
//!
//! After PR #4 the plugin runtime, sandbox, lifecycle, hook routing, and the
//! host-pure `arbor.*` namespaces all live in `arbor-plugin-core` (reach them
//! through [`arbor_plugin_core::prelude`]). The git product's `arbor.*`
//! namespaces relocated to `corvus-be` (crate `corvus-plugin-ns`) with the
//! product-relocation flip. What stays here is only:
//!   · [`api_installer`] — the launcher's `LuaApiInstaller`, publishing the
//!     host-pure base surface (no product-specific extras).

pub mod api_installer;
