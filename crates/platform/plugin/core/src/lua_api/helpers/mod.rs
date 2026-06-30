//! Pure helpers and shared infrastructure used across `lua_api::ns::*` and
//! the host-shell namespace installers in src-tauri.
//!
//! Every helper is `pub` (rather than `pub(crate)`) because src-tauri's ns/*
//! still consume them through the `arbor_plugin_core::lua_api::helpers::*`
//! path until those namespaces migrate into plugin-core themselves (Step 6).

pub mod contrib_write;
pub mod convert;
pub mod fs_perm;
pub mod glob;
pub mod http_worker;
pub mod json_patch;
pub mod settings_scope;
pub mod timer;
pub mod tuple;
pub mod xml_patch;
