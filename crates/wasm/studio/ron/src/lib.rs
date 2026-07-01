//! `arbor-studio-ron` — the Studio RON backend, extracted from the
//! launcher's `src-tauri/src/ron_studio/`.
//!
//! RON is a **hand-written special** (NOT `DefaultBackend`): it keeps a
//! full hand-written [`prelude::StudioFormatBackend`] impl because of its
//! tag-preserving custom AST (`ast::RonAst` carries enum variant tags
//! `Some(...)` / named struct / named tuple that `ron::Value` drops),
//! forced float `.0` disambiguator, RON-special tree-diff (struct/tuple
//! name-match + synthetic `Some` segment) and query projection, and a
//! syn-based `.rs` schema loader. It still calls `arbor-studio-core`'s
//! engines (history with dedup + cap 128, encoding-aware persist).
//!
//! F12 (cross-ref rename) and F13 (project-wide bulk edit) are
//! **self-serving**: the backend runs its own project-wide previews
//! against the caller-supplied [`index_provider::RonIndexProvider`] (the
//! repo scanner + cross-ref index live in the launcher /
//! `arbor-studio-api`, which the crate must not name). The launcher wires
//! a provider; tests use [`index_provider::NoIndexProvider`].
//!
//! Schema support: this crate exposes [`schema_provider::RsSchemaProvider`]
//! (a `core::SchemaProvider`) for the simple formats that route their
//! `.rs` schema panel through Rust; RON's own backend serves its schema
//! methods straight from [`schema`].

pub mod ast;
pub mod backend_impl;
pub mod err;
pub mod index_provider;
pub mod registry;
pub mod schema;
pub mod schema_provider;

pub mod prelude;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use arbor_studio_core::prelude::StudioFormatBackend;

use crate::index_provider::SharedIndexProvider;

/// Build the RON `StudioFormatBackend` with no index/scanner provider.
///
/// Project-wide F12/F13 + `list_files` then surface empty results. The
/// launcher uses [`backend_with_index`]; this variant exists for tests +
/// callers that only need the active-doc paths.
pub fn backend() -> Arc<dyn StudioFormatBackend> {
    backend_impl::backend()
}

/// Build the RON backend wired to the caller's repo scanner + cross-ref
/// index (project-wide F12/F13 + `list_files`).
pub fn backend_with_index(index: SharedIndexProvider) -> Arc<dyn StudioFormatBackend> {
    backend_impl::backend_with_index(index)
}

/// Parse `text` as RON (tag-preserving AST) and project it to
/// `serde_json::Value`.
///
/// Scanner contract — matches `arbor-studio-json` / `arbor-studio-toml`'s
/// `parse_to_value`. Returns `None` on parse error (best-effort, matching
/// the scanner's policy). Uses `ast::to_json` (the user-facing RON→JSON
/// projection: tags become `$type`/`$tag`/`$items`, `Some` unwraps).
pub fn parse_to_value(text: &str) -> Option<serde_json::Value> {
    ast::parse(text).ok().map(|a| ast::to_json(&a))
}
