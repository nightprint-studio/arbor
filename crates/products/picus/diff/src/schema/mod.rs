//! Comparing two schemas.
//!
//! Five checks over the same pair of snapshots, kept in separate modules because
//! they are separately switchable, separately filtered and — the part that
//! matters — separately *skippable*: a snapshot read without indexes must not
//! produce "no index differences".
//!
//! * [`relations`] — tables, views and their columns
//! * [`indexes`]
//! * [`sequences`]
//! * [`constraints`] — primary and foreign keys
//! * [`triggers`]
//!
//! ## What "the same object" means here
//!
//! By name, folded per [`DiffConfig::case_insensitive`]. Nothing in a catalogue
//! survives a rename, so a renamed table is a drop and an add — which is also
//! what it is to a script that has to install one from the other.
//!
//! ## Objects hanging off a relation only one side has
//!
//! Not reported. A table missing on B has all of its indexes, keys and triggers
//! missing on B too, and listing them turns one finding into forty. The relation
//! is the finding; the rest is its content.

pub mod constraints;
pub mod indexes;
pub mod relations;
pub mod sequences;
pub mod triggers;

pub use constraints::{
    compare_constraints, ConstraintComparison, ConstraintDiff, ConstraintKind, ConstraintRef,
};
pub use indexes::{compare_indexes, IndexComparison, IndexDiff, IndexRef};
pub use relations::{compare_schema, ColumnDiff, RelationDiff, RelationRef, SchemaComparison};
pub use sequences::{compare_sequences, SequenceComparison, SequenceDiff};
pub use triggers::{compare_triggers, TriggerComparison, TriggerDiff, TriggerRef};

use picus_types::prelude::{SchemaSnapshot, TableInfo};

use crate::config::DiffConfig;
use crate::names::fold_name;

/// Every relation of a snapshot the run's filters accept, tables before views.
pub(crate) fn accepted_relations<'a>(
    snapshot: &'a SchemaSnapshot,
    config: &DiffConfig,
) -> Vec<&'a TableInfo> {
    snapshot
        .tables
        .iter()
        .chain(snapshot.views.iter())
        .filter(|r| config.accepts(r.kind, &r.name))
        .collect()
}

/// The relations both sides have, paired, in A's order.
///
/// The unit of work for every check that hangs off a relation. See the module
/// docs for why the unpaired ones are not in it.
pub(crate) fn paired_relations<'a, 'b>(
    a: &'a SchemaSnapshot,
    b: &'b SchemaSnapshot,
    config: &DiffConfig,
) -> Vec<(&'a TableInfo, &'b TableInfo)> {
    let ci = config.case_insensitive;
    let list_b = accepted_relations(b, config);
    accepted_relations(a, config)
        .into_iter()
        .filter_map(|ra| {
            let folded = fold_name(&ra.name, ci);
            list_b
                .iter()
                .find(|rb| fold_name(&rb.name, ci) == folded)
                .map(|rb| (ra, *rb))
        })
        .collect()
}
