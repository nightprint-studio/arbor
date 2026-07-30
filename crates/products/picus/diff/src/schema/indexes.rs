//! Indexes.
//!
//! Keyed by `(relation, index name)` rather than by name alone: two relations may
//! legally carry an index of the same name in engines that scope index names to
//! the table, and a global key would compare one against the other.

use serde::{Deserialize, Serialize};

use picus_types::prelude::{IndexInfo, SchemaSnapshot, TableInfo};

use crate::change::FieldChange;
use crate::config::DiffConfig;
use crate::names::{fold_all, fold_name};
use crate::schema::paired_relations;

/// An index named, with enough of its shape to be recognised in a list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexRef {
    pub table: String,
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

impl IndexRef {
    fn of(table: &str, index: &IndexInfo) -> Self {
        Self {
            table: table.to_string(),
            name: index.name.clone(),
            columns: index.columns.clone(),
            unique: index.unique,
        }
    }
}

/// An index that exists on both sides and is not the same index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexDiff {
    pub table: String,
    pub name: String,
    /// Order is part of the answer: an index on `(A, B)` is not the index on
    /// `(B, A)`, and the query that used the first one will not use the second.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<FieldChange<Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unique: Option<FieldChange<bool>>,
    /// The access method, when the run asked for it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<FieldChange<Option<String>>>,
}

impl IndexDiff {
    fn is_empty(&self) -> bool {
        self.columns.is_none() && self.unique.is_none() && self.kind.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexComparison {
    pub only_in_a: Vec<IndexRef>,
    pub only_in_b: Vec<IndexRef>,
    pub changed: Vec<IndexDiff>,
    /// Relations whose indexes were not in the snapshot on one side or the other.
    /// Their indexes were **not compared**, and a report that omitted this would
    /// be claiming they matched.
    pub not_read: Vec<String>,
}

impl IndexComparison {
    pub fn has_differences(&self) -> bool {
        !self.only_in_a.is_empty() || !self.only_in_b.is_empty() || !self.changed.is_empty()
    }

    /// True when something in scope was not looked at.
    pub fn is_partial(&self) -> bool {
        !self.not_read.is_empty()
    }
}

pub fn compare_indexes(
    a: &SchemaSnapshot,
    b: &SchemaSnapshot,
    config: &DiffConfig,
) -> IndexComparison {
    let mut out = IndexComparison::default();
    for (ta, tb) in paired_relations(a, b, config) {
        match (ta.indexes.as_ref(), tb.indexes.as_ref()) {
            (Some(ia), Some(ib)) => compare_one(ta, ia, tb, ib, config, &mut out),
            _ => out.not_read.push(ta.name.clone()),
        }
    }
    out
}

fn compare_one(
    ta: &TableInfo,
    ia: &[IndexInfo],
    tb: &TableInfo,
    ib: &[IndexInfo],
    config: &DiffConfig,
    out: &mut IndexComparison,
) {
    let ci = config.case_insensitive;
    let in_scope = |i: &IndexInfo| {
        !(config.indexes.ignore_primary_key_indexes && i.primary_key)
            && config.indexes.filter.accepts(&i.name, ci)
    };
    let list_a: Vec<&IndexInfo> = ia.iter().filter(|i| in_scope(i)).collect();
    let list_b: Vec<&IndexInfo> = ib.iter().filter(|i| in_scope(i)).collect();

    for index in &list_a {
        let folded = fold_name(&index.name, ci);
        match list_b.iter().find(|o| fold_name(&o.name, ci) == folded) {
            None => out.only_in_a.push(IndexRef::of(&ta.name, index)),
            Some(other) => {
                let diff = diff_of(&ta.name, index, other, config);
                if !diff.is_empty() {
                    out.changed.push(diff);
                }
            }
        }
    }
    for index in &list_b {
        let folded = fold_name(&index.name, ci);
        if !list_a.iter().any(|o| fold_name(&o.name, ci) == folded) {
            out.only_in_b.push(IndexRef::of(&tb.name, index));
        }
    }
}

fn diff_of(table: &str, a: &IndexInfo, b: &IndexInfo, config: &DiffConfig) -> IndexDiff {
    let ci = config.case_insensitive;
    IndexDiff {
        table: table.to_string(),
        name: a.name.clone(),
        columns: (fold_all(&a.columns, ci) != fold_all(&b.columns, ci))
            .then(|| FieldChange::new(a.columns.clone(), b.columns.clone())),
        unique: FieldChange::changed(a.unique, b.unique),
        kind: if config.indexes.compare_kind {
            FieldChange::changed(a.kind.clone(), b.kind.clone())
        } else {
            None
        },
    }
}
