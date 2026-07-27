//! Canonical entry point for `picus-types`'s public API.
//!
//! Workspace convention: call sites reach this crate through
//! `picus_types::prelude::...`. In practice most code sees these types re-exported
//! from `picus_db_api::prelude` or `picus_ast::prelude` — whichever half it is
//! already working in — which is deliberate: the shared vocabulary should not make
//! every consumer name a third crate.

pub use crate::kind::EngineKind;
pub use crate::schema::{
    Column, ForeignKey, IndexInfo, RelationKind, SchemaSnapshot, SequenceInfo, TableInfo,
    TriggerInfo,
};
