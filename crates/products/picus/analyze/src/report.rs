//! [`Report`] and [`analyze`] — running the fourteen rules over a repository.
//!
//! A report is three lists, and the second and third are not decoration:
//!
//! * **findings** — what is wrong, suppressed ones included and marked;
//! * **skipped** — rules that could not run, and why. A rule that quietly passes
//!   because it had nothing to work with is indistinguishable, in a report, from
//!   a rule that passed. `VER003` on a project whose filenames carry no starting
//!   version is the case this list exists for;
//! * **rejected suppressions** — comments somebody wrote that silence nothing.

use std::collections::HashMap;

use picus_inventory::prelude::{Inventory, ParsedProject};
use picus_project::prelude::ProjectConfig;
use serde::Serialize;

use crate::context::Context;
use crate::finding::Finding;
use crate::rule::{RuleId, Severity};
use crate::suppress::{self, RejectedSuppression};

/// A rule that did not run, and what would make it run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedRule {
    pub rule: RuleId,
    /// What it was asked about: a folder path, a file, or the project when empty.
    pub scope: String,
    /// Written for the person who could make it run, not for a log.
    pub reason: String,
}

/// What the analysis concluded.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    /// Worst first, then by file and line. Suppressed findings keep their place.
    pub findings: Vec<Finding>,
    pub skipped: Vec<SkippedRule>,
    pub rejected_suppressions: Vec<RejectedSuppression>,
}

impl Report {
    /// Findings that are not silenced — the numbers the status bar shows.
    pub fn open(&self) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(|f| !f.is_suppressed())
    }

    pub fn count(&self, severity: Severity) -> usize {
        self.open().filter(|f| f.severity == severity).count()
    }

    pub fn suppressed_count(&self) -> usize {
        self.findings.iter().filter(|f| f.is_suppressed()).count()
    }

    pub fn of_rule(&self, rule: RuleId) -> impl Iterator<Item = &Finding> {
        self.findings.iter().filter(move |f| f.rule == rule)
    }

    pub fn was_skipped(&self, rule: RuleId) -> bool {
        self.skipped.iter().any(|s| s.rule == rule)
    }
}

/// Where the rules put what they find.
#[derive(Debug, Default)]
pub(crate) struct Output {
    pub findings: Vec<Finding>,
    pub skipped: Vec<SkippedRule>,
}

impl Output {
    pub(crate) fn skip(&mut self, rule: RuleId, scope: impl Into<String>, reason: impl Into<String>) {
        self.skipped.push(SkippedRule {
            rule,
            scope: scope.into(),
            reason: reason.into(),
        });
    }
}

/// Run every rule.
///
/// The inventory is a parameter rather than something built here: the interface
/// renders it too, and rebuilding it behind the caller's back would mean the
/// coverage table and the findings could be computed from two different reads of
/// the same repository.
pub fn analyze(
    project: &ParsedProject<'_>,
    config: &ProjectConfig,
    inventory: &Inventory,
) -> Report {
    let context = Context::new(project, config, inventory);
    let mut output = Output::default();
    crate::rules::run_all(&context, &mut output);

    // Worst first, then in reading order. Deterministic before suppressions are
    // applied, so a re-run produces the same list in the same order.
    output.findings.sort_by(|a, b| {
        (a.severity, &a.file, a.line.unwrap_or(0), a.rule, &a.id)
            .cmp(&(b.severity, &b.file, b.line.unwrap_or(0), b.rule, &b.id))
    });
    output.skipped.sort_by(|a, b| (a.rule, &a.scope).cmp(&(b.rule, &b.scope)));

    let mut by_file: HashMap<String, Vec<suppress::Suppression>> = HashMap::new();
    let mut rejected = Vec::new();
    for (script, _) in project.placed() {
        let (accepted, bad) = suppress::scan(script.path, script.source, script.parsed);
        if !accepted.is_empty() {
            by_file.insert(script.path.to_string(), accepted);
        }
        rejected.extend(bad);
    }
    rejected.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    suppress::apply(&mut output.findings, &by_file);

    Report {
        findings: output.findings,
        skipped: output.skipped,
        rejected_suppressions: rejected,
    }
}
