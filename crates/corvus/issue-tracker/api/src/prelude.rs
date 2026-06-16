//! Canonical entry point for `corvus-issue-tracker-api`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `corvus_issue_tracker_api::prelude::...` (or a single glob import). The
//! `types` submodule stays `pub` for rustdoc navigation but is not the
//! canonical call-site path.

pub use crate::branch_name_for_issue;
pub use crate::types::{
    BodyFormat, Issue, IssueAttachment, IssueComment, IssueCycle, IssueFilterOptions, IssueFilters,
    IssueLabel, IssueMilestone, IssueProject, IssueStatus, IssueTeam, IssueUser, LinearAuthStatus,
};
