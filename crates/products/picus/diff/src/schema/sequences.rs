//! Sequences: their definition, and how far apart their current values are.
//!
//! `last_value` is state, not structure, and two live databases are never on the
//! same number — so a bare inequality would fire on every sequence, every run.
//! [`SequenceCheck::warning_threshold`] is what makes the check mean something:
//! below it the drift is two databases being used, above it somebody restored a
//! dump without its sequences and the next insert is going to collide.
//!
//! [`SequenceCheck::warning_threshold`]: crate::config::SequenceCheck::warning_threshold

use serde::{Deserialize, Serialize};

use picus_types::prelude::{SchemaSnapshot, SequenceInfo};

use crate::change::{FieldChange, Severity};
use crate::config::DiffConfig;
use crate::names::fold_name;

/// A sequence that exists on both sides and is not in the same state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceDiff {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_value: Option<FieldChange<i64>>,
    /// `b - a`, present whenever `last_value` is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta: Option<i64>,
    /// [`Severity::Ok`] for a drift inside the threshold: still reported, because
    /// a sequence that moved is a fact, but not something to colour red.
    pub severity: Severity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub increment_by: Option<FieldChange<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_value: Option<FieldChange<Option<i64>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_value: Option<FieldChange<Option<i64>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cycle: Option<FieldChange<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_size: Option<FieldChange<Option<i64>>>,
}

impl SequenceDiff {
    fn is_empty(&self) -> bool {
        self.last_value.is_none()
            && self.increment_by.is_none()
            && self.min_value.is_none()
            && self.max_value.is_none()
            && self.cycle.is_none()
            && self.cache_size.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SequenceComparison {
    pub only_in_a: Vec<String>,
    pub only_in_b: Vec<String>,
    pub changed: Vec<SequenceDiff>,
}

impl SequenceComparison {
    pub fn has_differences(&self) -> bool {
        !self.only_in_a.is_empty() || !self.only_in_b.is_empty() || !self.changed.is_empty()
    }
}

pub fn compare_sequences(
    a: &SchemaSnapshot,
    b: &SchemaSnapshot,
    config: &DiffConfig,
) -> SequenceComparison {
    let ci = config.case_insensitive;
    let in_scope = |s: &SequenceInfo| config.sequences.filter.accepts(&s.name, ci);
    let list_a: Vec<&SequenceInfo> = a.sequences.iter().filter(|s| in_scope(s)).collect();
    let list_b: Vec<&SequenceInfo> = b.sequences.iter().filter(|s| in_scope(s)).collect();

    let mut out = SequenceComparison::default();
    for s in &list_a {
        let folded = fold_name(&s.name, ci);
        match list_b.iter().find(|o| fold_name(&o.name, ci) == folded) {
            None => out.only_in_a.push(s.name.clone()),
            Some(other) => {
                let diff = diff_of(s, other, config);
                if !diff.is_empty() {
                    out.changed.push(diff);
                }
            }
        }
    }
    for s in &list_b {
        let folded = fold_name(&s.name, ci);
        if !list_a.iter().any(|o| fold_name(&o.name, ci) == folded) {
            out.only_in_b.push(s.name.clone());
        }
    }
    out
}

fn diff_of(a: &SequenceInfo, b: &SequenceInfo, config: &DiffConfig) -> SequenceDiff {
    let last_value = FieldChange::changed(a.last_value, b.last_value);
    // Saturating: a sequence sitting near `i64::MAX` is a real (if alarming)
    // state, and the diff of two of them must not be an arithmetic panic.
    let delta = last_value.as_ref().map(|_| b.last_value.saturating_sub(a.last_value));
    let severity = match delta {
        None => Severity::Ok,
        Some(d) if d.saturating_abs() >= config.sequences.warning_threshold => Severity::Warning,
        Some(_) => Severity::Ok,
    };
    let attributes = config.sequences.compare_attributes;

    SequenceDiff {
        name: a.name.clone(),
        last_value,
        delta,
        severity,
        increment_by: attributes
            .then(|| FieldChange::changed(a.increment_by, b.increment_by))
            .flatten(),
        min_value: attributes.then(|| FieldChange::changed(a.min_value, b.min_value)).flatten(),
        max_value: attributes.then(|| FieldChange::changed(a.max_value, b.max_value)).flatten(),
        cycle: attributes.then(|| FieldChange::changed(a.cycle, b.cycle)).flatten(),
        cache_size: attributes.then(|| FieldChange::changed(a.cache_size, b.cache_size)).flatten(),
    }
}
