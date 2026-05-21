//! GitHub provider — `struct GithubProvider` + `impl GitProvider`.
//!
//! Single host-keyed instance: one GithubProvider serves every
//! `github.com` repo on the user's tabs. Repo context is supplied via
//! `RepoRef` / `MrId` parameters on each method.

use async_trait::async_trait;

use crate::git_provider::{
    GitProvider, ProviderKind,
    types::{
        Capabilities, ProviderUser, OAuthHandle,
        RemoteRepoInfo, RepoCreateRequest, ListReposOpts, RepoRef,
        MrInfo, MrId, MrDetail, MrComment, MrFile, MrCreateRequest,
        MrUpdateRequest, MergeOpts, MrConflict, MrFilter,
        CiRun, CiJob, CiWorkflow, CiFilter, PipelineCreateRequest,
        Release, ReleaseCreateRequest,
        RepoIssue, IssueCreateRequest, IssueFilter,
        Webhook, WebhookCreateRequest,
        BranchProtection,
        SecuritySummary, SecurityFinding, SecurityFilters,
        error::ProviderError,
    },
};

pub mod api;
pub mod auth;
pub mod repo;
pub mod mr;
pub mod ci;
pub mod releases;
pub mod issues;
pub mod webhooks;
pub mod security;

pub struct GithubProvider {
    host:         String,
    web_base_url: String,
}

impl GithubProvider {
    /// Default `github.com` instance. Phase 4 may add a constructor for
    /// GitHub Enterprise (`new_enterprise(host)`).
    pub fn new() -> Self {
        Self {
            host:         "github.com".into(),
            web_base_url: api::GITHUB_WEB_BASE.into(),
        }
    }
}

impl Default for GithubProvider {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl GitProvider for GithubProvider {
    // ── Identity ─────────────────────────────────────────────────────────
    fn kind(&self) -> ProviderKind { ProviderKind::GitHub }
    fn host(&self) -> &str { &self.host }
    fn web_base_url(&self) -> &str { &self.web_base_url }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            mr:                true,
            ci:                true,
            releases:          false,
            issues:            false,
            webhooks:          false,
            branch_protection: false,
            oauth:             true,
            search:            false,
            // Trait-level surface: GithubProvider implements security via
            // code-scanning + secret-scanning + Dependabot aggregation. The
            // UI uses `supports_security` for per-repo runtime gating —
            // some repos won't have any source enabled.
            security:          true,
        }
    }

    // ── Auth ─────────────────────────────────────────────────────────────
    fn has_token(&self) -> bool { auth::has_token() }
    async fn current_user(&self) -> Result<ProviderUser, ProviderError> {
        auth::current_user().await
    }
    async fn start_oauth(&self) -> Result<OAuthHandle, ProviderError> {
        // Legacy flow needs an AppHandle which the trait can't carry —
        // command layer drives it via `oauth::github::start` instead.
        Err(ProviderError::Unsupported {
            feature: "start_oauth (use oauth::github::start with AppHandle)".into(),
        })
    }
    async fn complete_oauth(&self, _h: OAuthHandle, _code: &str) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported {
            feature: "complete_oauth (handled by start_github_device_flow listener)".into(),
        })
    }
    async fn revoke_token(&self) -> Result<(), ProviderError> {
        crate::git_provider::oauth::github::revoke_token()
    }

    // ── Repo CRUD ────────────────────────────────────────────────────────
    async fn create_repo(&self, req: RepoCreateRequest) -> Result<RemoteRepoInfo, ProviderError> {
        repo::create_repo(req).await
    }
    async fn get_repo(&self, owner: &str, name: &str) -> Result<RemoteRepoInfo, ProviderError> {
        repo::get_repo(owner, name).await
    }
    async fn list_user_repos(&self, opts: ListReposOpts) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
        repo::list_user_repos(opts).await
    }
    async fn list_org_repos(&self, org: &str, opts: ListReposOpts) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
        repo::list_org_repos(org, opts).await
    }
    async fn search_repos(&self, query: &str) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
        repo::search_repos(query).await
    }

    // ── MR / PR ──────────────────────────────────────────────────────────
    async fn list_mrs(&self, repo: &RepoRef, filter: MrFilter) -> Result<Vec<MrInfo>, ProviderError> {
        mr::list_mrs(repo, filter).await
    }
    async fn get_mr(&self, id: &MrId) -> Result<MrDetail, ProviderError> {
        mr::get_mr(id).await
    }
    async fn create_mr(&self, repo: &RepoRef, req: MrCreateRequest) -> Result<MrInfo, ProviderError> {
        mr::create_mr(repo, req).await
    }
    async fn update_mr(&self, id: &MrId, req: MrUpdateRequest) -> Result<MrInfo, ProviderError> {
        mr::update_mr(id, req).await
    }
    async fn close_mr(&self, id: &MrId) -> Result<(), ProviderError> { mr::close_mr(id).await }
    async fn reopen_mr(&self, id: &MrId) -> Result<(), ProviderError> { mr::reopen_mr(id).await }
    async fn merge_mr(&self, id: &MrId, opts: MergeOpts) -> Result<(), ProviderError> {
        mr::merge_mr(id, opts).await
    }
    async fn list_mr_comments(&self, id: &MrId) -> Result<Vec<MrComment>, ProviderError> {
        mr::list_mr_comments(id).await
    }
    async fn add_mr_comment(&self, id: &MrId, body: &str) -> Result<MrComment, ProviderError> {
        mr::add_mr_comment(id, body).await
    }
    async fn list_mr_files(&self, id: &MrId) -> Result<Vec<MrFile>, ProviderError> {
        mr::list_mr_files(id).await
    }
    async fn fetch_mr_diff(&self, id: &MrId) -> Result<String, ProviderError> {
        mr::fetch_mr_diff(id).await
    }
    async fn check_mr_conflict(&self, id: &MrId) -> Result<MrConflict, ProviderError> {
        mr::check_mr_conflict(id).await
    }
    async fn list_mr_reviewers(&self, id: &MrId) -> Result<Vec<ProviderUser>, ProviderError> {
        mr::list_mr_reviewers(id).await
    }
    async fn request_mr_review(&self, id: &MrId, user: &str) -> Result<(), ProviderError> {
        mr::request_mr_review(id, user).await
    }
    async fn approve_mr(&self, id: &MrId) -> Result<(), ProviderError> {
        mr::approve_mr(id).await
    }

    // ── CI / CD ──────────────────────────────────────────────────────────
    async fn list_ci_runs(&self, repo: &RepoRef, filter: CiFilter) -> Result<Vec<CiRun>, ProviderError> {
        ci::list_ci_runs(repo, filter).await
    }
    async fn get_ci_run(&self, repo: &RepoRef, run_id: &str) -> Result<CiRun, ProviderError> {
        ci::get_ci_run(repo, run_id).await
    }
    async fn fetch_ci_jobs(&self, repo: &RepoRef, run_id: &str) -> Result<Vec<CiJob>, ProviderError> {
        ci::fetch_ci_jobs(repo, run_id).await
    }
    async fn fetch_ci_job_log(&self, repo: &RepoRef, job_id: &str) -> Result<String, ProviderError> {
        ci::fetch_ci_job_log(repo, job_id).await
    }
    async fn retrigger_ci_run(&self, repo: &RepoRef, run_id: &str) -> Result<(), ProviderError> {
        ci::retrigger_ci_run(repo, run_id).await
    }
    async fn cancel_ci_run(&self, repo: &RepoRef, run_id: &str) -> Result<(), ProviderError> {
        ci::cancel_ci_run(repo, run_id).await
    }
    async fn list_ci_workflows(&self, repo: &RepoRef) -> Result<Vec<CiWorkflow>, ProviderError> {
        ci::list_ci_workflows(repo).await
    }
    async fn create_ci_pipeline(&self, repo: &RepoRef, req: PipelineCreateRequest) -> Result<CiRun, ProviderError> {
        ci::create_ci_pipeline(repo, req).await
    }

    // ── Releases (STUB) ──────────────────────────────────────────────────
    async fn list_releases(&self, repo: &RepoRef) -> Result<Vec<Release>, ProviderError> {
        releases::list_releases(repo).await
    }
    async fn get_release(&self, repo: &RepoRef, id: &str) -> Result<Release, ProviderError> {
        releases::get_release(repo, id).await
    }
    async fn create_release(&self, repo: &RepoRef, req: ReleaseCreateRequest) -> Result<Release, ProviderError> {
        releases::create_release(repo, req).await
    }
    async fn delete_release(&self, repo: &RepoRef, id: &str) -> Result<(), ProviderError> {
        releases::delete_release(repo, id).await
    }

    // ── Repo issues (STUB) ───────────────────────────────────────────────
    async fn list_repo_issues(&self, repo: &RepoRef, filter: IssueFilter) -> Result<Vec<RepoIssue>, ProviderError> {
        issues::list_repo_issues(repo, filter).await
    }
    async fn get_repo_issue(&self, repo: &RepoRef, id: &str) -> Result<RepoIssue, ProviderError> {
        issues::get_repo_issue(repo, id).await
    }
    async fn create_repo_issue(&self, repo: &RepoRef, req: IssueCreateRequest) -> Result<RepoIssue, ProviderError> {
        issues::create_repo_issue(repo, req).await
    }
    async fn comment_repo_issue(&self, repo: &RepoRef, id: &str, body: &str) -> Result<(), ProviderError> {
        issues::comment_repo_issue(repo, id, body).await
    }
    async fn close_repo_issue(&self, repo: &RepoRef, id: &str) -> Result<(), ProviderError> {
        issues::close_repo_issue(repo, id).await
    }

    // ── Webhooks (STUB) ──────────────────────────────────────────────────
    async fn list_webhooks(&self, repo: &RepoRef) -> Result<Vec<Webhook>, ProviderError> {
        webhooks::list_webhooks(repo).await
    }
    async fn create_webhook(&self, repo: &RepoRef, req: WebhookCreateRequest) -> Result<Webhook, ProviderError> {
        webhooks::create_webhook(repo, req).await
    }
    async fn delete_webhook(&self, repo: &RepoRef, id: &str) -> Result<(), ProviderError> {
        webhooks::delete_webhook(repo, id).await
    }

    // ── Branches via REST (STUB) ─────────────────────────────────────────
    async fn list_remote_branches(&self, _repo: &RepoRef) -> Result<Vec<String>, ProviderError> {
        Err(ProviderError::Unsupported { feature: "list_remote_branches".into() })
    }
    async fn get_default_branch(&self, _repo: &RepoRef) -> Result<String, ProviderError> {
        Err(ProviderError::Unsupported { feature: "get_default_branch".into() })
    }
    async fn protect_branch(&self, _repo: &RepoRef, _branch: &str, _req: BranchProtection) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported { feature: "protect_branch".into() })
    }

    // ── Security dashboard ───────────────────────────────────────────────
    async fn supports_security(&self, repo: &RepoRef) -> Result<bool, ProviderError> {
        security::supports_security(repo).await
    }
    async fn fetch_security_summary(&self, repo: &RepoRef, range_days: u32) -> Result<SecuritySummary, ProviderError> {
        security::fetch_security_summary(repo, range_days).await
    }
    async fn fetch_security_findings(&self, repo: &RepoRef, filters: SecurityFilters) -> Result<Vec<SecurityFinding>, ProviderError> {
        security::fetch_security_findings(repo, filters).await
    }
}
