//! Canonical entry point for `picus-diff`'s public API.
//!
//! Workspace convention: call sites reach this crate through
//! `picus_diff::prelude::...`. The submodules stay `pub` for rustdoc navigation,
//! but the diff always goes through here.

pub use crate::change::{FieldChange, Severity};
pub use crate::config::{
    ColumnFilter, ConstraintCheck, ContentCheck, CountCheck, DiffConfig, FilterMode, IndexCheck,
    NameFilter, SchemaCheck, SequenceCheck, TableRules, TriggerCheck,
};
pub use crate::counts::{compare_counts, CountComparison, TableCount};
pub use crate::error::{DiffError, Side};
pub use crate::names::{fold_all, fold_name, glob_match, matches_any, missing_from};
pub use crate::report::{CheckKind, DiffReport, SkipReason, SkippedCheck, Verdict};
pub use crate::rows::{
    compare_rows, CellDiff, ChangedRow, DiffRow, RowCompareOptions, RowKey, RowSet,
    RowsComparison,
};
pub use crate::schema::{
    compare_constraints, compare_indexes, compare_schema, compare_sequences, compare_triggers,
    ColumnDiff, ConstraintComparison, ConstraintDiff, ConstraintKind, ConstraintRef,
    IndexComparison, IndexDiff, IndexRef, RelationDiff, RelationRef, SchemaComparison,
    SequenceComparison, SequenceDiff, TriggerComparison, TriggerDiff, TriggerRef,
};
pub use crate::template::{DiffTemplate, DiffTemplates};
pub use crate::value::DiffValue;
