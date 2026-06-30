//! The shared auth-status and issue-creation shapes for the tracker contract.
//!
//! A tracker's *self-description* (what the FE renders to connect it) is the
//! shared [`ProviderDescriptor`](corvus_provider_descriptor::prelude::ProviderDescriptor),
//! returned by [`IssueTracker::descriptor`](crate::tracker::IssueTracker::descriptor)
//! — defined in `corvus-provider-descriptor` so the git-host side and the single
//! generic frontend speak one vocabulary.

use serde::{Deserialize, Serialize};

use crate::types::IssueUser;

/// Provider-agnostic auth status (superset of the per-provider shapes).
///
/// This is the tracker domain's internal status; the shell maps it onto the
/// FE-facing [`corvus_provider_descriptor::prelude::AuthStatus`] for the generic
/// connection IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuthStatus {
    pub authenticated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<IssueUser>,
    /// Tenant host where it applies (Jira); `None` for single-tenant trackers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Which auth method is active (`"oauth"` | `"pat"` | `"basic"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_method: Option<String>,
}

/// Fields for creating an issue — the superset across trackers; an impl uses
/// what it supports and ignores the rest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewIssue {
    pub title: String,
    pub description: Option<String>,
    /// Linear team id / Jira project key — the container the issue lands in.
    pub team_id: Option<String>,
    pub status_id: Option<String>,
    pub assignee_id: Option<String>,
    pub label_ids: Vec<String>,
    pub priority: Option<u32>,
    pub project_id: Option<String>,
    pub milestone_id: Option<String>,
    pub due_date: Option<String>,
    pub estimate: Option<f64>,
    /// Jira issue type (`"Bug"`, `"Task"`, …); ignored by trackers without types.
    pub issue_type: Option<String>,
}
