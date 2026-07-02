//! Canonical entry point for `bennu-index`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `bennu_index::prelude::...`. The submodules stay `pub` for rustdoc navigation,
//! but the prelude is the canonical call-site path.

// Schema: the multi-source symbol / relation records + their tags.
pub use crate::schema::{Relation, RelationKind, Source, Symbol, SymbolKind};

// Store: the framed-blob writer/reader + fst map opener + format constants/errors.
pub use crate::store::{
    open_fst_map, BlobReader, BlobWriter, StoreError, FORMAT_VERSION, MAGIC, RECORD_ALIGN,
};

// Query: the typed index view (exact / prefix / fuzzy) + the serialize helper.
pub use crate::query::{serialize_symbol, SymbolIndex};

// Builder: ingest records per source file, persist, and incrementally patch one file;
// plus the persisted read view the completion query serves from.
pub use crate::builder::{IndexBuilder, IndexRecord, PersistedIndex};

// Relations: the config-graph / type-hierarchy edge store (write side + read side).
pub use crate::relations::{serialize_relation, RelationReader, RelationWriter};
