//! One run's answer, including the parts of the question that went unanswered.
//!
//! ## A skipped check is part of the verdict
//!
//! The reason this is a type and not a bag of comparisons: a diff tool is used to
//! decide whether it is safe to ship something, and "identical" is the sentence
//! that decision is made on. If the run compared schemas but not contents,
//! because contents were disabled — or compared indexes for nine relations and
//! not the tenth, because the snapshot did not carry them — then "identical" is
//! a lie of omission, and the person reading it has no way to know.
//!
//! So every check that did not run leaves a [`SkippedCheck`] behind, and
//! [`DiffReport::finish`] downgrades a clean run to
//! [`Verdict::IdenticalWhereChecked`]. The three verdicts are the three things a
//! reader can act on: ship it, do not ship it, or look again with more turned on.

use serde::{Deserialize, Serialize};

use crate::counts::CountComparison;
use crate::rows::RowsComparison;
use crate::schema::{
    ConstraintComparison, IndexComparison, SchemaComparison, SequenceComparison,
    TriggerComparison,
};

/// What the run concluded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Verdict {
    /// Everything asked for was compared, and it all matched.
    Identical,
    /// Nothing that was compared differs — but something was not compared. Never
    /// collapsed into `Identical`; see the module docs.
    #[default]
    IdenticalWhereChecked,
    Different,
}

/// Which check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CheckKind {
    Schema,
    Counts,
    Contents,
    Indexes,
    Sequences,
    Constraints,
    Triggers,
}

/// Why a check did not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SkipReason {
    /// Switched off in the configuration. Still recorded: the user turning a
    /// check off does not make its absence invisible to whoever reads the report.
    Disabled,
    /// The engine on one side cannot answer this.
    Unsupported,
    /// The input did not carry it — a snapshot read without indexes, a table
    /// whose foreign keys the session may not see.
    NotRead,
    /// Rows could not be matched: no key was given and no positional comparison
    /// was meaningful.
    NoKey,
    /// The check would have exceeded a limit the run set for itself.
    LimitReached,
    /// It was attempted and it failed. `detail` carries what the caller was told.
    Failed,
}

/// One check, or one scope of one check, that did not run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedCheck {
    pub check: CheckKind,
    /// The relation or query it applies to. `None` when the whole check was
    /// skipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    pub reason: SkipReason,
    /// Said in words a reader can act on — "contents are off in this template",
    /// not "contents disabled".
    pub detail: String,
}

/// Everything one comparison run produced.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffReport {
    /// What the two sides are called, for a report that is read months later.
    pub label_a: String,
    pub label_b: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<SchemaComparison>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indexes: Option<IndexComparison>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequences: Option<SequenceComparison>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub constraints: Option<ConstraintComparison>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub triggers: Option<TriggerComparison>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub counts: Option<Vec<CountComparison>>,
    /// One entry per relation (or query pair) whose rows were compared.
    pub rows: Vec<RowsComparison>,
    pub skipped: Vec<SkippedCheck>,
    pub verdict: Verdict,
}

impl DiffReport {
    pub fn new(label_a: impl Into<String>, label_b: impl Into<String>) -> Self {
        Self {
            label_a: label_a.into(),
            label_b: label_b.into(),
            // Nothing has been compared yet, and the default must not be the
            // optimistic one.
            verdict: Verdict::IdenticalWhereChecked,
            ..Default::default()
        }
    }

    /// Record a whole check that did not run.
    pub fn skip(&mut self, check: CheckKind, reason: SkipReason, detail: impl Into<String>) {
        self.skipped.push(SkippedCheck {
            check,
            scope: None,
            reason,
            detail: detail.into(),
        });
    }

    /// Record one relation (or query) a check could not cover.
    pub fn skip_scope(
        &mut self,
        check: CheckKind,
        scope: impl Into<String>,
        reason: SkipReason,
        detail: impl Into<String>,
    ) {
        self.skipped.push(SkippedCheck {
            check,
            scope: Some(scope.into()),
            reason,
            detail: detail.into(),
        });
    }

    /// Did anything that *was* compared come out different?
    pub fn has_differences(&self) -> bool {
        self.schema.as_ref().is_some_and(|c| c.has_differences())
            || self.indexes.as_ref().is_some_and(|c| c.has_differences())
            || self.sequences.as_ref().is_some_and(|c| c.has_differences())
            || self.constraints.as_ref().is_some_and(|c| c.has_differences())
            || self.triggers.as_ref().is_some_and(|c| c.has_differences())
            || self.counts.as_ref().is_some_and(|c| c.iter().any(|r| r.differs()))
            || self.rows.iter().any(|r| r.has_differences())
    }

    /// Was anything in scope left unlooked-at?
    ///
    /// Covers both the checks that never ran and the ones that ran over an input
    /// that was missing part of itself — `not_read` on a comparison is exactly as
    /// disqualifying as a skip.
    pub fn is_partial(&self) -> bool {
        !self.skipped.is_empty()
            || self.indexes.as_ref().is_some_and(|c| c.is_partial())
            || self.constraints.as_ref().is_some_and(|c| c.is_partial())
            || self.rows.iter().any(|r| r.truncated)
    }

    /// Compute the verdict. Call once, when the run is over.
    pub fn finish(mut self) -> Self {
        self.verdict = if self.has_differences() {
            Verdict::Different
        } else if self.is_partial() {
            Verdict::IdenticalWhereChecked
        } else {
            Verdict::Identical
        };
        self
    }
}
