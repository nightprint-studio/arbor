//! Canonical entry point for `corvus-issues`' public API. Designed as a drop-in
//! for the shell's former `crate::integrations::*` re-export, so call sites move
//! by changing only the path they import from.

pub use crate::build::{jira_new_issue, linear_new_issue};
pub use crate::registry::build_registry;
pub use crate::types::JiraAuthStatus;

// The whole issue-tracker contract — DTOs (`Issue`, `IssueComment`, `IssueUser`,
// `IssueFilters`, `LinearAuthStatus`, …), the `IssueTracker` trait, `NewIssue`,
// `IssueTrackerRegistry`/`IssueTrackerError`, and `branch_name_for_issue` — flows
// through the api prelude, mirroring today's `integrations` re-export.
pub use corvus_issue_tracker_api::prelude::*;
// The concrete Jira handle (its inherent methods aren't on the trait) + Linear's
// token-validation free fn and endpoint, which the shell connect path re-uses.
pub use corvus_issue_tracker_jira::prelude::JiraTracker;
pub use corvus_issue_tracker_linear::prelude::{validate_token, LINEAR_GQL};
