//! Behavioural tests, one module per rule family.
//!
//! These are deliberately not coverage tests. Every rule here is easy to make
//! fire; what is hard is making it **stay quiet** on a repository that is doing
//! the right thing, and a false positive costs more than a missed finding —
//! people stop reading a report that is wrong about things they know are fine.
//! So most of what follows is a case that must produce nothing.

mod consistency;
mod dialect;
mod disabled;
mod dml;
mod duplicate;
mod encoding;
mod propagation;
mod report;
mod suppression;
mod tree;
mod version;

use crate::report::Report;
use crate::rule::RuleId;

/// Findings of one rule that are not silenced.
pub(crate) fn open_of(report: &Report, rule: RuleId) -> Vec<&crate::finding::Finding> {
    report.of_rule(rule).filter(|f| !f.is_suppressed()).collect()
}
