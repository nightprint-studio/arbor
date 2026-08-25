//! Canonical entry point for `picus-lineage`'s public API.
//!
//! Workspace convention: call sites reach this crate through
//! `picus_lineage::prelude::...`, never through the submodules.

pub use crate::model::{Hop, Ingredient, Lineage, Trace, Verdict};
pub use crate::resolve::{trace_relation, trace_statement, Catalogue, MAX_DEPTH};
