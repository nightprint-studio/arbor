//! Canonical entry point for `arbor-studio-json`'s public API.
//!
//! Call sites reach this crate through `arbor_studio_json::prelude::...`.

pub use crate::{backend, backend_with_index, parse_to_value};
pub use crate::index_provider::{
    JsonIndexProvider, NoIndexProvider, ScanFile, SharedIndexProvider,
};
pub use crate::schema_provider::JsonSchemaProvider;

// The scanner / def-walk in the launcher (→ `arbor-studio-api` in Stage 4)
// walks JSON via the byte-range AST, so the AST surface is part of the
// public contract: `parse_with`, `ast_to_value`, and the `JsonAst` tree.
pub use crate::ast::{self, ast_to_value, parse_with, JsonAst};
