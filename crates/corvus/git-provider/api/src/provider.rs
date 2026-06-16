//! Unified `GitProvider` trait — the only path from Corvus to a remote git
//! host's REST API (GitHub, GitLab, future Gitea/Bitbucket).
//!
//! Adding a new provider is a matter of creating a `corvus-git-provider-<name>`
//! crate with `struct <Name>Provider` and `impl GitProvider for ...`.  The
//! compiler enforces that every required operation is implemented (stub
//! methods may return `ProviderError::Unsupported`).
//!
//! ## Design notes
//! - Trait is `Send + Sync` so providers can be shared across async tasks.
//! - Every method is `async fn` (via `#[async_trait]`).
//! - `ProviderError::Unsupported { feature }` is the canonical "this
//!   provider does not implement X" — capability fields drive UI gating.

use async_trait::async_trait;

use crate::auth::{OAuthHandle, ProviderUser};
use crate::branch::BranchProtection;
use crate::capability::Capabilities;
use crate::ci::{CiFilter, CiJob, CiRun, CiWorkflow, PipelineCreateRequest};
use crate::error::ProviderError;
use crate::issue::{IssueCreateRequest, IssueFilter, RepoIssue};
use crate::kind::ProviderKind;
use crate::mr::{
    MergeOpts, MrComment, MrConflict, MrCreateRequest, MrDetail, MrFile, MrFilter, MrId, MrInfo,
    MrUpdateRequest,
};
use crate::release::{Release, ReleaseCreateRequest};
use crate::repo::{ListReposOpts, RemoteRepoInfo, RepoCreateRequest, RepoRef};
use crate::security::{SecurityFilters, SecurityFinding, SecuritySummary};
use crate::webhook::{Webhook, WebhookCreateRequest};

/// A remote git host's REST surface.
///
/// Every method is required — stub methods on providers that don't
/// implement a feature should return `Err(ProviderError::Unsupported)`.
#[async_trait]
pub trait GitProvider: Send + Sync {
    // ── Identity ─────────────────────────────────────────────────────────
    fn kind(&self) -> ProviderKind;
    /// Hostname this instance is bound to (e.g. `github.com`,
    /// `gitlab.example.org`). Used as the registry key.
    fn host(&self) -> &str;
    /// Base URL for human-facing pages (e.g. `https://github.com`).
    fn web_base_url(&self) -> &str;
    fn capabilities(&self) -> Capabilities;

    // ── Auth ─────────────────────────────────────────────────────────────
    fn has_token(&self) -> bool;
    async fn current_user(&self) -> Result<ProviderUser, ProviderError>;
    async fn start_oauth(&self) -> Result<OAuthHandle, ProviderError>;
    async fn complete_oauth(&self, handle: OAuthHandle, code: &str) -> Result<(), ProviderError>;
    async fn revoke_token(&self) -> Result<(), ProviderError>;

    // ── Repo CRUD ────────────────────────────────────────────────────────
    async fn create_repo(&self, req: RepoCreateRequest) -> Result<RemoteRepoInfo, ProviderError>;
    async fn get_repo(&self, owner: &str, name: &str) -> Result<RemoteRepoInfo, ProviderError>;
    async fn list_user_repos(&self, opts: ListReposOpts) -> Result<Vec<RemoteRepoInfo>, ProviderError>;
    async fn list_org_repos(&self, org: &str, opts: ListReposOpts) -> Result<Vec<RemoteRepoInfo>, ProviderError>;
    async fn search_repos(&self, query: &str) -> Result<Vec<RemoteRepoInfo>, ProviderError>;

    // ── MR / PR ──────────────────────────────────────────────────────────
    //
    // Repo-scoped methods take `&RepoRef` so a single host-keyed provider
    // instance can serve every repo on that host.
    async fn list_mrs(&self, repo: &RepoRef, filter: MrFilter) -> Result<Vec<MrInfo>, ProviderError>;
    async fn get_mr(&self, id: &MrId) -> Result<MrDetail, ProviderError>;
    async fn create_mr(&self, repo: &RepoRef, req: MrCreateRequest) -> Result<MrInfo, ProviderError>;
    async fn update_mr(&self, id: &MrId, req: MrUpdateRequest) -> Result<MrInfo, ProviderError>;
    async fn close_mr(&self, id: &MrId) -> Result<(), ProviderError>;
    async fn reopen_mr(&self, id: &MrId) -> Result<(), ProviderError>;
    async fn merge_mr(&self, id: &MrId, opts: MergeOpts) -> Result<(), ProviderError>;
    async fn list_mr_comments(&self, id: &MrId) -> Result<Vec<MrComment>, ProviderError>;
    async fn add_mr_comment(&self, id: &MrId, body: &str) -> Result<MrComment, ProviderError>;
    async fn list_mr_files(&self, id: &MrId) -> Result<Vec<MrFile>, ProviderError>;
    async fn fetch_mr_diff(&self, id: &MrId) -> Result<String, ProviderError>;
    async fn check_mr_conflict(&self, id: &MrId) -> Result<MrConflict, ProviderError>;
    async fn list_mr_reviewers(&self, id: &MrId) -> Result<Vec<ProviderUser>, ProviderError>;
    async fn request_mr_review(&self, id: &MrId, user: &str) -> Result<(), ProviderError>;
    async fn approve_mr(&self, id: &MrId) -> Result<(), ProviderError>;

    // ── CI / CD ──────────────────────────────────────────────────────────
    async fn list_ci_runs(&self, repo: &RepoRef, filter: CiFilter) -> Result<Vec<CiRun>, ProviderError>;
    async fn get_ci_run(&self, repo: &RepoRef, run_id: &str) -> Result<CiRun, ProviderError>;
    async fn fetch_ci_jobs(&self, repo: &RepoRef, run_id: &str) -> Result<Vec<CiJob>, ProviderError>;
    async fn fetch_ci_job_log(&self, repo: &RepoRef, job_id: &str) -> Result<String, ProviderError>;
    async fn retrigger_ci_run(&self, repo: &RepoRef, run_id: &str) -> Result<(), ProviderError>;
    async fn cancel_ci_run(&self, repo: &RepoRef, run_id: &str) -> Result<(), ProviderError>;
    async fn list_ci_workflows(&self, repo: &RepoRef) -> Result<Vec<CiWorkflow>, ProviderError>;
    async fn create_ci_pipeline(&self, repo: &RepoRef, req: PipelineCreateRequest) -> Result<CiRun, ProviderError>;

    // ── Releases (STUB on launch) ────────────────────────────────────────
    async fn list_releases(&self, repo: &RepoRef) -> Result<Vec<Release>, ProviderError>;
    async fn get_release(&self, repo: &RepoRef, id: &str) -> Result<Release, ProviderError>;
    async fn create_release(&self, repo: &RepoRef, req: ReleaseCreateRequest) -> Result<Release, ProviderError>;
    async fn delete_release(&self, repo: &RepoRef, id: &str) -> Result<(), ProviderError>;

    // ── Repo-native issues — NOT Linear/Jira (STUB on launch) ────────────
    async fn list_repo_issues(&self, repo: &RepoRef, filter: IssueFilter) -> Result<Vec<RepoIssue>, ProviderError>;
    async fn get_repo_issue(&self, repo: &RepoRef, id: &str) -> Result<RepoIssue, ProviderError>;
    async fn create_repo_issue(&self, repo: &RepoRef, req: IssueCreateRequest) -> Result<RepoIssue, ProviderError>;
    async fn comment_repo_issue(&self, repo: &RepoRef, id: &str, body: &str) -> Result<(), ProviderError>;
    async fn close_repo_issue(&self, repo: &RepoRef, id: &str) -> Result<(), ProviderError>;

    // ── Webhooks (STUB on launch) ────────────────────────────────────────
    async fn list_webhooks(&self, repo: &RepoRef) -> Result<Vec<Webhook>, ProviderError>;
    async fn create_webhook(&self, repo: &RepoRef, req: WebhookCreateRequest) -> Result<Webhook, ProviderError>;
    async fn delete_webhook(&self, repo: &RepoRef, id: &str) -> Result<(), ProviderError>;

    // ── Branches via REST (separate from local git2) ─────────────────────
    async fn list_remote_branches(&self, repo: &RepoRef) -> Result<Vec<String>, ProviderError>;
    async fn get_default_branch(&self, repo: &RepoRef) -> Result<String, ProviderError>;
    async fn protect_branch(&self, repo: &RepoRef, branch: &str, req: BranchProtection) -> Result<(), ProviderError>;

    // ── Security dashboard ───────────────────────────────────────────────
    //
    // `supports_security` is a fast capability probe (no findings fetch):
    // GitLab returns `true` when a token is present and the project exposes
    // vulnerability data; GitHub returns `true` when the security_events
    // scope is granted.  The frontend uses it to gate the ActivityBar +
    // StatusBar entries without paying for a full summary fetch.
    async fn supports_security(&self, repo: &RepoRef) -> Result<bool, ProviderError> {
        let _ = repo;
        Ok(self.capabilities().security)
    }
    async fn fetch_security_summary(&self, repo: &RepoRef, range_days: u32) -> Result<SecuritySummary, ProviderError> {
        let _ = (repo, range_days);
        Err(ProviderError::Unsupported { feature: "fetch_security_summary".into() })
    }
    async fn fetch_security_findings(&self, repo: &RepoRef, filters: SecurityFilters) -> Result<Vec<SecurityFinding>, ProviderError> {
        let _ = (repo, filters);
        Err(ProviderError::Unsupported { feature: "fetch_security_findings".into() })
    }
}
