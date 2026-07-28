//! Canonical entry point for `picus-analyze`'s public API.
//!
//! Workspace convention: call sites reach this crate through
//! `picus_analyze::prelude::...`. The submodules stay `pub` for rustdoc
//! navigation, but the diff always goes through here.

pub use crate::compare::{comparable_rows, render as render_row, row_fingerprint, RowFingerprint};
pub use crate::context::{
    branch_label, engine_label, fold_identifier, folders_with_role, Context,
};
pub use crate::finding::{Anchor, Finding, FindingDraft};
pub use crate::report::{analyze, Report, SkippedRule};
pub use crate::rule::{RuleId, Severity};
pub use crate::suppress::{RejectedSuppression, Scope, Suppression};
