//! [`IssueTracker`] — the async, object-safe contract every tracker impl
//! satisfies, so the host holds them uniformly as `Arc<dyn IssueTracker>` in the
//! [`crate::registry::IssueTrackerRegistry`].
//!
//! An impl is constructed with its credentials already injected (an
//! `Arc<dyn arbor_ipc::prelude::SessionProvider>`), so the trait methods take no
//! token: the impl reaches the keyring only through the shell.

use async_trait::async_trait;

use crate::error::Result;
use crate::provider::{AuthStatus, NewIssue, ProviderDescriptor};
use crate::types::{Issue, IssueComment, IssueFilterOptions, IssueFilters};

/// One issue tracker (Linear, Jira, GitHub/GitLab Issues, …).
#[async_trait]
pub trait IssueTracker: Send + Sync {
    /// What the FE needs to list and connect this tracker (id, icon, auth form).
    fn descriptor(&self) -> ProviderDescriptor;

    /// Whether a credential is configured and valid, plus the current user.
    async fn auth_status(&self) -> Result<AuthStatus>;

    /// Search issues matching `filters`.
    async fn search_issues(&self, filters: IssueFilters) -> Result<Vec<Issue>>;

    /// Fetch one issue by its provider id.
    async fn get_issue(&self, id: &str) -> Result<Issue>;

    /// Resolve one issue by its human identifier (e.g. `"ENG-42"`), or `None`.
    async fn lookup_by_identifier(&self, identifier: &str) -> Result<Option<Issue>>;

    /// The option lists (teams, statuses, labels, …) for building filter UIs.
    async fn get_filter_options(&self) -> Result<IssueFilterOptions>;

    /// Move an issue to a new status.
    async fn transition_issue(&self, id: &str, status_id: &str) -> Result<Issue>;

    /// Assign (or, with `None`, unassign) an issue.
    async fn assign_issue(&self, id: &str, user_id: Option<&str>) -> Result<Issue>;

    /// Add a comment to an issue.
    async fn add_comment(&self, issue_id: &str, body: &str) -> Result<IssueComment>;

    /// Create a new issue.
    async fn create_issue(&self, req: NewIssue) -> Result<Issue>;

    /// Fetch bytes of an image referenced by an issue/comment body, with its
    /// content type — for inline preview. Auth is applied on-host only.
    async fn fetch_image_bytes(&self, url: &str) -> Result<(Vec<u8>, Option<String>)>;
}
