//! `arbor-studio-json` — the Studio JSON backend, extracted from the
//! launcher's `src-tauri/src/json_studio/`.
//!
//! JSON is a **hand-written special** (NOT `DefaultBackend`): it keeps a
//! full hand-written [`prelude::StudioFormatBackend`] impl because of its
//! dual parser (`simd-json` read path + `jsonc-parser` byte-splice edit
//! path), sticky per-doc stream mode for multi-MB files, JSONC comments /
//! trailing commas, and an AST tree-diff that distinguishes `1.0` from
//! `1.00` via `Number.raw`. It still calls `arbor-studio-core`'s engines
//! (history / diff / query / edit_expr / refactor / persist).
//!
//! F12 (cross-ref rename) and F13 (project-wide bulk edit) are
//! **self-serving**: the backend runs its own project-wide previews
//! against the caller-supplied [`index_provider::JsonIndexProvider`] (the
//! repo scanner + cross-ref index live in the launcher /
//! `arbor-studio-api`, which the crate must not name). The launcher wires
//! a provider; tests use [`index_provider::NoIndexProvider`].
//!
//! Schema support: this crate exposes [`schema_provider::JsonSchemaProvider`]
//! (a `core::SchemaProvider`) for the simple formats that route their
//! JSON-Schema panel through JSON; JSON's own backend serves its schema
//! methods straight from [`schema`].

pub mod ast;
pub mod backend_impl;
pub mod bulk_edits;
pub mod edits;
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

/// Build the JSON `StudioFormatBackend` with no index/scanner provider.
///
/// Project-wide F12/F13 + `list_files` then surface empty results. The
/// launcher uses [`backend_with_index`]; this variant exists for tests +
/// callers that only need the active-doc paths.
pub fn backend() -> Arc<dyn StudioFormatBackend> {
    backend_impl::backend()
}

/// Build the JSON backend wired to the caller's repo scanner + cross-ref
/// index (project-wide F12/F13 + `list_files`).
pub fn backend_with_index(index: SharedIndexProvider) -> Arc<dyn StudioFormatBackend> {
    backend_impl::backend_with_index(index)
}

/// Parse `text` as JSON (lenient: `.jsonc` comments + trailing commas
/// allowed) and project it to `serde_json::Value`.
///
/// Scanner contract — matches `arbor-studio-toml` / `arbor-studio-yaml`'s
/// `parse_to_value`. Returns `None` on parse error (best-effort, matching
/// the scanner's policy).
pub fn parse_to_value(text: &str) -> Option<serde_json::Value> {
    ast::parse_with(text, /* strict */ false)
        .ok()
        .map(|a| ast::ast_to_value(&a))
}
