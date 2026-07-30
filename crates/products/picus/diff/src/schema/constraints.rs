//! Primary and foreign keys.
//!
//! ## Why matching by name is not the default
//!
//! A constraint created without an explicit name gets one from the server, and
//! that name is different in every database the script was ever run into
//! (`SYS_C0011423` on one, `SYS_C0028871` on the next). Matching on it would
//! report every foreign key in the schema as missing on both sides at once,
//! which is not a report — it is a wall. So the default
//! ([`ConstraintCheck::ignore_names`]) matches a foreign key by **what it does**:
//! its columns, the relation it points at and the columns there. Two keys with
//! the same definition are the same key however they are called, which is also
//! the only reading under which "these two databases agree" is true.
//!
//! Turn it off when the names are deliberate — a repository whose scripts name
//! every constraint, where a renamed key really is a change.
//!
//! [`ConstraintCheck::ignore_names`]: crate::config::ConstraintCheck::ignore_names

use serde::{Deserialize, Serialize};

use picus_types::prelude::{ForeignKey, SchemaSnapshot, TableInfo};

use crate::change::FieldChange;
use crate::config::DiffConfig;
use crate::names::{fold_all, fold_name};
use crate::schema::paired_relations;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConstraintKind {
    PrimaryKey,
    ForeignKey,
}

/// A constraint named — as far as it is named at all.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintRef {
    pub table: String,
    /// `None` for a constraint the snapshot carries without a name — a primary
    /// key read from the columns that make it up, typically.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub kind: ConstraintKind,
    pub columns: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referenced_table: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub referenced_columns: Vec<String>,
}

/// A constraint that exists on both sides and does something different.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintDiff {
    pub table: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub kind: ConstraintKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<FieldChange<Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referenced_table: Option<FieldChange<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub referenced_columns: Option<FieldChange<Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on_delete: Option<FieldChange<Option<String>>>,
    /// Only ever set for a primary key, and only when the run compares names: a
    /// table has one primary key, so the two are matched by the relation and the
    /// name is a property of it rather than its identity. Foreign keys matched by
    /// name cannot differ in it, and foreign keys matched by definition are not
    /// being judged on it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name_changed: Option<FieldChange<Option<String>>>,
}

impl ConstraintDiff {
    fn is_empty(&self) -> bool {
        self.columns.is_none()
            && self.referenced_table.is_none()
            && self.referenced_columns.is_none()
            && self.on_delete.is_none()
            && self.name_changed.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConstraintComparison {
    pub only_in_a: Vec<ConstraintRef>,
    pub only_in_b: Vec<ConstraintRef>,
    pub changed: Vec<ConstraintDiff>,
    /// Relations whose foreign keys were absent from the snapshot on one side.
    /// Their keys were not compared; primary keys still were, since those are
    /// read from the columns.
    pub foreign_keys_not_read: Vec<String>,
}

impl ConstraintComparison {
    pub fn has_differences(&self) -> bool {
        !self.only_in_a.is_empty() || !self.only_in_b.is_empty() || !self.changed.is_empty()
    }

    pub fn is_partial(&self) -> bool {
        !self.foreign_keys_not_read.is_empty()
    }
}

pub fn compare_constraints(
    a: &SchemaSnapshot,
    b: &SchemaSnapshot,
    config: &DiffConfig,
) -> ConstraintComparison {
    let mut out = ConstraintComparison::default();
    for (ta, tb) in paired_relations(a, b, config) {
        compare_primary_key(ta, tb, config, &mut out);
        match (ta.foreign_keys.as_ref(), tb.foreign_keys.as_ref()) {
            (Some(fa), Some(fb)) => compare_foreign_keys(ta, fa, fb, config, &mut out),
            _ => out.foreign_keys_not_read.push(ta.name.clone()),
        }
    }
    out
}

/// The columns that make up the primary key, in declaration order.
fn primary_key_columns(t: &TableInfo) -> Vec<String> {
    t.columns.iter().filter(|c| c.primary_key).map(|c| c.name.clone()).collect()
}

fn in_scope(name: Option<&String>, config: &DiffConfig) -> bool {
    // An unnamed constraint is filtered as the empty name: an exclude list never
    // matches it, an include list never accepts it. Both are the reading that
    // does not silently drop something the user did not write a pattern for.
    config
        .constraints
        .filter
        .accepts(name.map(String::as_str).unwrap_or(""), config.case_insensitive)
}

fn compare_primary_key(
    ta: &TableInfo,
    tb: &TableInfo,
    config: &DiffConfig,
    out: &mut ConstraintComparison,
) {
    let (ca, cb) = (primary_key_columns(ta), primary_key_columns(tb));
    if !in_scope(ta.primary_key_name.as_ref(), config)
        && !in_scope(tb.primary_key_name.as_ref(), config)
    {
        return;
    }

    let reference = |t: &TableInfo, columns: Vec<String>| ConstraintRef {
        table: t.name.clone(),
        name: t.primary_key_name.clone(),
        kind: ConstraintKind::PrimaryKey,
        columns,
        referenced_table: None,
        referenced_columns: Vec::new(),
    };

    match (ca.is_empty(), cb.is_empty()) {
        (true, true) => {}
        (false, true) => out.only_in_a.push(reference(ta, ca)),
        (true, false) => out.only_in_b.push(reference(tb, cb)),
        (false, false) => {
            let ci = config.case_insensitive;
            let diff = ConstraintDiff {
                table: ta.name.clone(),
                name: ta.primary_key_name.clone(),
                kind: ConstraintKind::PrimaryKey,
                columns: (fold_all(&ca, ci) != fold_all(&cb, ci))
                    .then(|| FieldChange::new(ca, cb)),
                referenced_table: None,
                referenced_columns: None,
                on_delete: None,
                name_changed: if config.constraints.ignore_names {
                    None
                } else {
                    FieldChange::changed(
                        ta.primary_key_name.clone(),
                        tb.primary_key_name.clone(),
                    )
                },
            };
            if !diff.is_empty() {
                out.changed.push(diff);
            }
        }
    }
}

/// What a foreign key is matched by when names are not trusted.
fn signature(fk: &ForeignKey, ci: bool) -> String {
    format!(
        "{}->{}({})",
        fold_all(&fk.columns, ci).join(","),
        fold_name(&fk.referenced_table, ci),
        fold_all(&fk.referenced_columns, ci).join(",")
    )
}

fn identity(fk: &ForeignKey, config: &DiffConfig) -> String {
    if config.constraints.ignore_names {
        signature(fk, config.case_insensitive)
    } else {
        fold_name(&fk.name, config.case_insensitive)
    }
}

fn reference_of(table: &str, fk: &ForeignKey) -> ConstraintRef {
    ConstraintRef {
        table: table.to_string(),
        name: Some(fk.name.clone()),
        kind: ConstraintKind::ForeignKey,
        columns: fk.columns.clone(),
        referenced_table: Some(fk.referenced_table.clone()),
        referenced_columns: fk.referenced_columns.clone(),
    }
}

fn compare_foreign_keys(
    table: &TableInfo,
    fa: &[ForeignKey],
    fb: &[ForeignKey],
    config: &DiffConfig,
    out: &mut ConstraintComparison,
) {
    let list_a: Vec<&ForeignKey> =
        fa.iter().filter(|f| in_scope(Some(&f.name), config)).collect();
    let list_b: Vec<&ForeignKey> =
        fb.iter().filter(|f| in_scope(Some(&f.name), config)).collect();

    for fk in &list_a {
        let id = identity(fk, config);
        match list_b.iter().find(|o| identity(o, config) == id) {
            None => out.only_in_a.push(reference_of(&table.name, fk)),
            Some(other) => {
                let diff = diff_of(&table.name, fk, other, config);
                if !diff.is_empty() {
                    out.changed.push(diff);
                }
            }
        }
    }
    for fk in &list_b {
        let id = identity(fk, config);
        if !list_a.iter().any(|o| identity(o, config) == id) {
            out.only_in_b.push(reference_of(&table.name, fk));
        }
    }
}

fn diff_of(table: &str, a: &ForeignKey, b: &ForeignKey, config: &DiffConfig) -> ConstraintDiff {
    let ci = config.case_insensitive;
    ConstraintDiff {
        table: table.to_string(),
        name: Some(a.name.clone()),
        kind: ConstraintKind::ForeignKey,
        columns: (fold_all(&a.columns, ci) != fold_all(&b.columns, ci))
            .then(|| FieldChange::new(a.columns.clone(), b.columns.clone())),
        referenced_table: (fold_name(&a.referenced_table, ci)
            != fold_name(&b.referenced_table, ci))
        .then(|| FieldChange::new(a.referenced_table.clone(), b.referenced_table.clone())),
        referenced_columns: (fold_all(&a.referenced_columns, ci)
            != fold_all(&b.referenced_columns, ci))
        .then(|| {
            FieldChange::new(a.referenced_columns.clone(), b.referenced_columns.clone())
        }),
        // Not folded: `CASCADE` and `cascade` are the same rule, but no server
        // reports it in two spellings, and folding a value would be a habit worth
        // not starting.
        on_delete: FieldChange::changed(a.on_delete.clone(), b.on_delete.clone()),
        name_changed: None,
    }
}
