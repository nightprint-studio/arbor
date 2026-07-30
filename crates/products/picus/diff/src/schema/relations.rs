//! Tables, views, and the columns in them.

use serde::{Deserialize, Serialize};

use picus_types::prelude::{Column, RelationKind, SchemaSnapshot, TableInfo};

use crate::change::FieldChange;
use crate::config::DiffConfig;
use crate::names::fold_name;
use crate::schema::accepted_relations;

/// A relation named, with what it is. Both facts, because "missing in B" reads
/// differently for a view than for a table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationRef {
    pub name: String,
    pub kind: RelationKind,
}

impl From<&TableInfo> for RelationRef {
    fn from(t: &TableInfo) -> Self {
        Self { name: t.name.clone(), kind: t.kind }
    }
}

/// One column that is not the same on both sides.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColumnDiff {
    pub name: String,
    /// The type exactly as each server reports it. Never normalised: `varchar(30)`
    /// and `character varying(30)` are the same type and a normaliser that knew
    /// that would also have to know the fifty pairs where it is not true.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_type: Option<FieldChange<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub not_null: Option<FieldChange<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<FieldChange<Option<String>>>,
    /// 0-based ordinal in the relation. Off by default — see
    /// [`ColumnFilter::ignore_position`](crate::config::ColumnFilter::ignore_position).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<FieldChange<usize>>,
}

impl ColumnDiff {
    fn is_empty(&self) -> bool {
        self.data_type.is_none()
            && self.not_null.is_none()
            && self.default_value.is_none()
            && self.position.is_none()
    }
}

/// One relation that exists on both sides and is not the same.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationDiff {
    pub name: String,
    /// A's kind — what the reader is most likely comparing *from*.
    pub kind: RelationKind,
    /// A table on one side and a view on the other. Rare, and never accidental:
    /// somebody replaced one with the other and the install scripts have to know.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind_changed: Option<FieldChange<RelationKind>>,
    pub columns_only_in_a: Vec<String>,
    pub columns_only_in_b: Vec<String>,
    pub columns_changed: Vec<ColumnDiff>,
    /// Views only, and only when the run asked for it — see
    /// [`SchemaCheck::compare_view_definitions`](crate::config::SchemaCheck::compare_view_definitions).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub definition: Option<FieldChange<String>>,
}

impl RelationDiff {
    fn is_empty(&self) -> bool {
        self.kind_changed.is_none()
            && self.columns_only_in_a.is_empty()
            && self.columns_only_in_b.is_empty()
            && self.columns_changed.is_empty()
            && self.definition.is_none()
    }
}

/// Relations and columns, on both sides.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaComparison {
    pub only_in_a: Vec<RelationRef>,
    pub only_in_b: Vec<RelationRef>,
    pub changed: Vec<RelationDiff>,
}

impl SchemaComparison {
    pub fn has_differences(&self) -> bool {
        !self.only_in_a.is_empty() || !self.only_in_b.is_empty() || !self.changed.is_empty()
    }
}

/// Compare the relations of two snapshots.
pub fn compare_schema(
    a: &SchemaSnapshot,
    b: &SchemaSnapshot,
    config: &DiffConfig,
) -> SchemaComparison {
    let ci = config.case_insensitive;
    let list_a = accepted_relations(a, config);
    let list_b = accepted_relations(b, config);

    let mut out = SchemaComparison::default();
    for ra in &list_a {
        let folded = fold_name(&ra.name, ci);
        match list_b.iter().find(|rb| fold_name(&rb.name, ci) == folded) {
            None => out.only_in_a.push(RelationRef::from(*ra)),
            Some(rb) => {
                let diff = compare_relation(ra, rb, config);
                if !diff.is_empty() {
                    out.changed.push(diff);
                }
            }
        }
    }
    for rb in &list_b {
        let folded = fold_name(&rb.name, ci);
        if !list_a.iter().any(|ra| fold_name(&ra.name, ci) == folded) {
            out.only_in_b.push(RelationRef::from(*rb));
        }
    }
    out
}

fn compare_relation(a: &TableInfo, b: &TableInfo, config: &DiffConfig) -> RelationDiff {
    let (only_in_a, only_in_b, changed) = compare_columns(a, b, config);
    RelationDiff {
        name: a.name.clone(),
        kind: a.kind,
        kind_changed: FieldChange::changed(a.kind, b.kind),
        columns_only_in_a: only_in_a,
        columns_only_in_b: only_in_b,
        columns_changed: changed,
        definition: view_definition_change(a, b, config),
    }
}

/// Compared only when both sides actually carry one: a snapshot read without
/// definitions would otherwise report every view as having lost its body.
fn view_definition_change(
    a: &TableInfo,
    b: &TableInfo,
    config: &DiffConfig,
) -> Option<FieldChange<String>> {
    if !config.schema.compare_view_definitions {
        return None;
    }
    let (da, db) = (a.definition.as_ref()?, b.definition.as_ref()?);
    FieldChange::changed(da.trim().to_string(), db.trim().to_string())
}

type ColumnOutcome = (Vec<String>, Vec<String>, Vec<ColumnDiff>);

fn compare_columns(a: &TableInfo, b: &TableInfo, config: &DiffConfig) -> ColumnOutcome {
    let ci = config.case_insensitive;
    // Positions are taken before filtering, so an ignored column still shifts the
    // ones after it — the ordinal reported is the one the server would give.
    let cols_a = visible_columns(&a.columns, config);
    let cols_b = visible_columns(&b.columns, config);

    let mut only_in_a = Vec::new();
    let mut changed = Vec::new();
    for (pos_a, ca) in &cols_a {
        let folded = fold_name(&ca.name, ci);
        match cols_b.iter().find(|(_, cb)| fold_name(&cb.name, ci) == folded) {
            None => only_in_a.push(ca.name.clone()),
            Some((pos_b, cb)) => {
                let diff = compare_column(ca, *pos_a, cb, *pos_b, config);
                if !diff.is_empty() {
                    changed.push(diff);
                }
            }
        }
    }

    let only_in_b = cols_b
        .iter()
        .filter(|(_, cb)| {
            let folded = fold_name(&cb.name, ci);
            !cols_a.iter().any(|(_, ca)| fold_name(&ca.name, ci) == folded)
        })
        .map(|(_, cb)| cb.name.clone())
        .collect();

    (only_in_a, only_in_b, changed)
}

fn visible_columns<'a>(columns: &'a [Column], config: &DiffConfig) -> Vec<(usize, &'a Column)> {
    columns
        .iter()
        .enumerate()
        .filter(|(_, c)| !config.columns.ignores(&c.name, config.case_insensitive))
        .collect()
}

fn compare_column(
    ca: &Column,
    pos_a: usize,
    cb: &Column,
    pos_b: usize,
    config: &DiffConfig,
) -> ColumnDiff {
    ColumnDiff {
        name: ca.name.clone(),
        data_type: FieldChange::changed(ca.data_type.clone(), cb.data_type.clone()),
        not_null: FieldChange::changed(ca.not_null, cb.not_null),
        default_value: if config.columns.ignore_defaults {
            None
        } else {
            FieldChange::changed(ca.default_value.clone(), cb.default_value.clone())
        },
        position: if config.columns.ignore_position {
            None
        } else {
            FieldChange::changed(pos_a, pos_b)
        },
    }
}
