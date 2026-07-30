//! Row counts, and how far apart two of them are allowed to be.
//!
//! The cheapest check there is, and the one people run first: two environments
//! that are supposed to hold the same reference data, or a copy that is supposed
//! to be complete. The thresholds exist because "same" is rarely the expectation
//! — a staging database drifting 3% from production is fine and drifting 60% is
//! a failed restore, and one report has to say both.

use serde::{Deserialize, Serialize};

use crate::change::Severity;
use crate::config::CountCheck;
use crate::names::fold_name;

/// What one side counted, if it could.
///
/// `rows: None` is "not counted" — the relation is missing on that side, the
/// count timed out, or the session may not read it. It is deliberately not `0`:
/// an empty table and a table nobody could look at are opposite findings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCount {
    pub table: String,
    pub rows: Option<i64>,
}

impl TableCount {
    pub fn new(table: impl Into<String>, rows: i64) -> Self {
        Self { table: table.into(), rows: Some(rows) }
    }

    /// A relation that was in scope but produced no number.
    pub fn unknown(table: impl Into<String>) -> Self {
        Self { table: table.into(), rows: None }
    }
}

/// Two counts of one relation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CountComparison {
    pub table: String,
    pub count_a: Option<i64>,
    pub count_b: Option<i64>,
    /// `b - a`. `None` when either side has no number.
    pub delta: Option<i64>,
    /// `delta` as a percentage of A. `None` when either side has no number, and
    /// also when A is zero and B is not — growth away from nothing has no
    /// percentage, and inventing one (or dividing by zero and shipping `inf`
    /// through JSON, where it does not exist) would be worse than saying so.
    pub delta_percent: Option<f64>,
    pub severity: Severity,
}

impl CountComparison {
    pub fn differs(&self) -> bool {
        // A relation that could not be counted on one side is a difference in the
        // report's sense: the reader must not take the run as clean.
        self.count_a != self.count_b
    }
}

/// Compare two sets of counts.
///
/// The relations are the union of both sides, in A's order and then B's, so a
/// table only one side has still appears. Filtering is by [`CountCheck::filter`];
/// the caller is expected to have already applied the run's relation filter when
/// it decided what to count.
pub fn compare_counts(
    a: &[TableCount],
    b: &[TableCount],
    check: &CountCheck,
    case_insensitive: bool,
) -> Vec<CountComparison> {
    let find = |list: &[TableCount], name: &str| -> Option<Option<i64>> {
        let wanted = fold_name(name, case_insensitive);
        list.iter().find(|c| fold_name(&c.table, case_insensitive) == wanted).map(|c| c.rows)
    };

    let mut names: Vec<&str> = Vec::with_capacity(a.len() + b.len());
    for c in a.iter().chain(b.iter()) {
        let folded = fold_name(&c.table, case_insensitive);
        if !names.iter().any(|n| fold_name(n, case_insensitive) == folded) {
            names.push(&c.table);
        }
    }

    names
        .into_iter()
        .filter(|name| check.filter.accepts(name, case_insensitive))
        .map(|name| {
            // A relation absent from one list was never counted there, which is the
            // same "no number" as a count that failed.
            let count_a = find(a, name).flatten();
            let count_b = find(b, name).flatten();
            compare_one(name, count_a, count_b, check)
        })
        .collect()
}

fn compare_one(
    table: &str,
    count_a: Option<i64>,
    count_b: Option<i64>,
    check: &CountCheck,
) -> CountComparison {
    let (delta, percent, severity) = match (count_a, count_b) {
        (Some(x), Some(y)) => {
            // Saturating rather than wrapping: counts are server-reported and a
            // pathological pair must produce a wrong-but-visible number, never a
            // panic in a report.
            let delta = y.saturating_sub(x);
            let percent = match (x, delta) {
                (_, 0) => Some(0.0),
                (0, _) => None,
                _ => Some((delta as f64 / x as f64) * 100.0),
            };
            let severity = if delta == 0 {
                Severity::Ok
            } else {
                Severity::from_percent(
                    percent,
                    check.warning_threshold_percent,
                    check.error_threshold_percent,
                )
            };
            (Some(delta), percent, severity)
        }
        // One side has no number. Not an error about the data — an admission that
        // the comparison did not happen, which is a warning the reader has to see
        // beside the rows that were compared.
        _ => (None, None, Severity::Warning),
    };

    CountComparison {
        table: table.to_string(),
        count_a,
        count_b,
        delta,
        delta_percent: percent,
        severity,
    }
}
