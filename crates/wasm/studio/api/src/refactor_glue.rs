//! Glue between the api-side studio index (`crate::index`) and the
//! format-agnostic `arbor_studio_core::refactor` orchestrators.
//!
//! `core::refactor` deliberately knows nothing about the api's
//! `StudioIndex` / `StudioFileKind`. The backends feed it plain input
//! slices; these mappers do the one-line shape conversion so the
//! conversion lives in exactly one place rather than copy-pasted into all
//! five `backend_impl.rs`.

use arbor_studio_core::prelude::{OpenDocState, RenameDefInput, RenameUsageInput};
use arbor_studio_types::prelude::{BulkEditOpenDoc, RenameOpenDoc};

use crate::index::{self, StudioIndex};
use crate::scanner::StudioFileKind;

/// Aggregate the index's cross-ref definitions (filtered to `kinds`,
/// empty = all) into the core rename-def input shape.
pub fn collect_rename_defs(idx: &StudioIndex, kinds: &[StudioFileKind]) -> Vec<RenameDefInput> {
    index::aggregate_cross_refs_for(idx, kinds)
        .into_iter()
        .map(|d| RenameDefInput {
            id_value:      d.id_value,
            absolute_path: d.absolute_path,
            relative_path: d.relative_path,
            file_name:     d.file_name,
            def_path:      d.def_path,
            def_field:     d.def_field,
        })
        .collect()
}

/// Aggregate the index's usages of `target` (filtered to `kinds`) into
/// the core rename-usage input shape.
pub fn collect_rename_usages(
    idx:    &StudioIndex,
    target: &str,
    kinds:  &[StudioFileKind],
) -> Vec<RenameUsageInput> {
    index::aggregate_usages_for(idx, target, kinds)
        .into_iter()
        .map(|u| RenameUsageInput {
            absolute_path: u.absolute_path,
            relative_path: u.relative_path,
            file_name:     u.file_name,
            field_path:    u.field_path,
            key_name:      u.key_name,
        })
        .collect()
}

/// Map the FE-supplied `RenameOpenDoc`s to the core dirty-state shape.
pub fn rename_open_doc_states(docs: Vec<RenameOpenDoc>) -> Vec<OpenDocState> {
    docs.into_iter()
        .map(|d| OpenDocState {
            doc_id:      d.doc_id,
            source_path: d.source_path,
            dirty:       d.dirty,
        })
        .collect()
}

/// Map the FE-supplied `BulkEditOpenDoc`s to the core dirty-state shape.
/// `RenameOpenDoc` and `BulkEditOpenDoc` are field-identical on the wire;
/// we keep two mappers so the call sites read declaratively.
pub fn bulk_open_doc_states(docs: Vec<BulkEditOpenDoc>) -> Vec<OpenDocState> {
    docs.into_iter()
        .map(|d| OpenDocState {
            doc_id:      d.doc_id,
            source_path: d.source_path,
            dirty:       d.dirty,
        })
        .collect()
}
