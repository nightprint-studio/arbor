//! Triggers.
//!
//! Compared on what they answer to — when they fire, on which statements, per row
//! or per statement — and on whether they are **enabled**, which is the one
//! difference a schema dump will not show you and the one most likely to explain
//! why two databases stopped agreeing about their data.
//!
//! Bodies are not compared. [`TriggerInfo`] does not carry one on purpose (a
//! routine body is orders of magnitude larger than the facts around it, and a
//! schema read for a diff would carry every one of them); comparing bodies is a
//! second read, asked for one trigger at a time.

use serde::{Deserialize, Serialize};

use picus_types::prelude::{SchemaSnapshot, TriggerInfo};

use crate::change::FieldChange;
use crate::config::DiffConfig;
use crate::names::{fold_all, fold_name};

/// A trigger named, with the two facts that identify what it is for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerRef {
    pub table: String,
    pub name: String,
    pub timing: String,
    pub events: Vec<String>,
}

impl From<&TriggerInfo> for TriggerRef {
    fn from(t: &TriggerInfo) -> Self {
        Self {
            table: t.table.clone(),
            name: t.name.clone(),
            timing: t.timing.clone(),
            events: t.events.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerDiff {
    pub table: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing: Option<FieldChange<String>>,
    /// Compared as a set: a server listing `UPDATE, INSERT` and one listing
    /// `INSERT, UPDATE` describe the same trigger.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub events: Option<FieldChange<Vec<String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<FieldChange<bool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub for_each_row: Option<FieldChange<bool>>,
}

impl TriggerDiff {
    fn is_empty(&self) -> bool {
        self.timing.is_none()
            && self.events.is_none()
            && self.enabled.is_none()
            && self.for_each_row.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggerComparison {
    pub only_in_a: Vec<TriggerRef>,
    pub only_in_b: Vec<TriggerRef>,
    pub changed: Vec<TriggerDiff>,
}

impl TriggerComparison {
    pub fn has_differences(&self) -> bool {
        !self.only_in_a.is_empty() || !self.only_in_b.is_empty() || !self.changed.is_empty()
    }
}

pub fn compare_triggers(
    a: &SchemaSnapshot,
    b: &SchemaSnapshot,
    config: &DiffConfig,
) -> TriggerComparison {
    let ci = config.case_insensitive;
    let scoped = |t: &TriggerInfo| {
        config.accepts_any_kind(&t.table) && config.triggers.filter.accepts(&t.name, ci)
    };
    let list_a: Vec<&TriggerInfo> = a.triggers.iter().filter(|t| scoped(t)).collect();
    let list_b: Vec<&TriggerInfo> = b.triggers.iter().filter(|t| scoped(t)).collect();

    // A trigger is identified by its relation and its name: two relations may
    // carry triggers of the same name, and comparing across them would pair a
    // trigger with a stranger.
    let identity = |t: &TriggerInfo| (fold_name(&t.table, ci), fold_name(&t.name, ci));

    let mut out = TriggerComparison::default();
    for t in &list_a {
        let id = identity(t);
        match list_b.iter().find(|o| identity(o) == id) {
            None => out.only_in_a.push(TriggerRef::from(*t)),
            Some(other) => {
                let diff = diff_of(t, other, config);
                if !diff.is_empty() {
                    out.changed.push(diff);
                }
            }
        }
    }
    for t in &list_b {
        let id = identity(t);
        if !list_a.iter().any(|o| identity(o) == id) {
            out.only_in_b.push(TriggerRef::from(*t));
        }
    }
    out
}

fn diff_of(a: &TriggerInfo, b: &TriggerInfo, config: &DiffConfig) -> TriggerDiff {
    let ci = config.case_insensitive;
    let sorted = |events: &[String]| {
        let mut folded = fold_all(events, ci);
        folded.sort();
        folded
    };

    TriggerDiff {
        table: a.table.clone(),
        name: a.name.clone(),
        timing: (fold_name(&a.timing, ci) != fold_name(&b.timing, ci))
            .then(|| FieldChange::new(a.timing.clone(), b.timing.clone())),
        events: (sorted(&a.events) != sorted(&b.events))
            .then(|| FieldChange::new(a.events.clone(), b.events.clone())),
        enabled: if config.triggers.compare_enabled_state {
            FieldChange::changed(a.enabled, b.enabled)
        } else {
            None
        },
        for_each_row: FieldChange::changed(a.for_each_row, b.for_each_row),
    }
}
