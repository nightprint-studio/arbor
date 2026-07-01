//! Canonical entry point for `arbor-studio-ron`'s public API.
//!
//! Call sites reach this crate through `arbor_studio_ron::prelude::...`.

pub use crate::{backend, backend_with_index, parse_to_value};
pub use crate::index_provider::{
    NoIndexProvider, RonIndexProvider, RonRenameDef, RonRenameInputs, RonRenameUsage,
    ScanFile, SharedIndexProvider,
};
pub use crate::schema_provider::RsSchemaProvider;

// The scanner / def-walk + schema-hint detection in the launcher (→
// `arbor-studio-api` in Stage 4) walk RON via the tag-preserving AST and
// read the file's schema hint, so these are part of the public contract:
// the `ast` module (`parse`, `to_json`, `RonAst`), `detect_schema_hint`,
// and the `SchemaHint`/`SchemaHintOrigin` it returns.
pub use crate::ast::{self, parse, to_json, to_pretty_string, to_pretty_string_with, RonAst};
pub use crate::registry::{detect_schema_hint, SchemaHint, SchemaHintOrigin};
