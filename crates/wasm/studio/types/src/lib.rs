//! `arbor-studio-types` — DTOs that cross the `StudioFormatBackend`
//! trait / IPC boundary, plus `StudioError`.
//!
//! Zero logic beyond derives + trivial ctors. No Tauri, no launcher
//! coupling. This is the bottom of the studio crate DAG (`core`, the
//! format crates, and `api` all depend on it) and is exactly the WASM-
//! guest compile target for Studio-as-plugin.
//!
//! ## Public API: use the [`prelude`]
//!
//! Workspace convention — reach this crate's surface through
//! `arbor_studio_types::prelude::...`. The submodules stay `pub` for
//! rustdoc navigation but are not the canonical call-site path.

pub mod descriptor;
pub mod dto;
pub mod errors;
pub mod prelude;
pub mod schema;

#[cfg(test)]
mod tests;
