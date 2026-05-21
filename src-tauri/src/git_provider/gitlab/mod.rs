//! GitLab provider — `struct GitlabProvider` + `impl GitProvider`.
//!
//! One instance per host: a default for `gitlab.com` is registered at
//! boot; self-hosted instances are added via `GitlabProvider::new_self_hosted`
//! when discovered through the credential store.

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

pub struct GitlabProvider {
    host:         String,
    web_base_url: String,
    /// Host root used as the `base_url` argument to legacy `*_gitlab_*` impl
    /// functions in `mr_impl` / `ci_impl` / `gitlab::{auth,repo}`.  Those
    /// functions append `/api/v4/...` themselves, so this MUST stay as a bare
    /// host (e.g. `https://gitlab.com`) — never with `/api/v4` appended.
    api_base_url: String,
    /// `true` for instances other than `gitlab.com` — disables OAuth refresh
    /// and uses host-keyed credentials.
    self_hosted:  bool,
}

impl GitlabProvider {
    /// Default `gitlab.com` instance (OAuth-enabled).
    pub fn new() -> Self {
        Self {
            host:         "gitlab.com".into(),
            web_base_url: api::GITLAB_COM_WEB.into(),
            api_base_url: api::GITLAB_COM_WEB.into(),
            self_hosted:  false,
        }
    }

    /// Self-hosted instance. `base_url` is the *host* root
    /// (e.g. `https://gitlab.example.org`); `/api/v4` is appended by callees.
    pub fn new_self_hosted(host: impl Into<String>) -> Self {
        let host_str: String = host.into();
        let trimmed = host_str.trim_end_matches('/').to_string();
        // Extract just the bare hostname for registry lookups.
        let bare_host = trimmed
            .strip_prefix("https://")
            .or_else(|| trimmed.strip_prefix("http://"))
            .unwrap_or(&trimmed)
            .to_string();
        Self {
            host:         bare_host,
            web_base_url: trimmed.clone(),
            api_base_url: trimmed,
            self_hosted:  true,
        }
    }

    /// Host root passed to legacy impl functions; they append `/api/v4/...`.
    fn api(&self) -> &str { &self.api_base_url }
}

impl Default for GitlabProvider {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl GitProvider for GitlabProvider {
    // ── Identity ─────────────────────────────────────────────────────────
    fn kind(&self) -> ProviderKind { ProviderKind::GitLab }
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
            // OAuth is gitlab.com-only; self-hosted relies on PATs.
            oauth:             !self.self_hosted,
            search:            false,
            // Probed dynamically — `true` means the provider implements the
            // surface; runtime gating in the UI uses `supports_security` for
            // per-project answers.
            security:          true,
        }
    }

    // ── Auth ─────────────────────────────────────────────────────────────
    fn has_token(&self) -> bool { auth::has_token(self.api()) }
    async fn current_user(&self) -> Result<ProviderUser, ProviderError> {
        auth::current_user(self.api()).await
    }
    async fn start_oauth(&self) -> Result<OAuthHandle, ProviderError> {
        // Same caveat as GitHub: legacy flow needs an AppHandle the trait
        // can't carry. Command layer drives via `oauth::gitlab::start`.
        Err(ProviderError::Unsupported {
            feature: "start_oauth (use oauth::gitlab::start with AppHandle)".into(),
        })
    }
    async fn complete_oauth(&self, _h: OAuthHandle, _code: &str) -> Result<(), ProviderError> {
        Err(ProviderError::Unsupported {
            feature: "complete_oauth (handled by start_gitlab_oauth listener)".into(),
        })
    }
    async fn revoke_token(&self) -> Result<(), ProviderError> {
        crate::git_provider::oauth::gitlab::revoke_token()
    }

    // ── Repo CRUD ────────────────────────────────────────────────────────
    async fn create_repo(&self, req: RepoCreateRequest) -> Result<RemoteRepoInfo, ProviderError> {
        repo::create_repo(self.api(), req).await
    }
    async fn get_repo(&self, owner: &str, name: &str) -> Result<RemoteRepoInfo, ProviderError> {
        repo::get_repo(self.api(), owner, name).await
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
        mr::list_mrs(self.api(), repo, filter).await
    }
    async fn get_mr(&self, id: &MrId) -> Result<MrDetail, ProviderError> {
        mr::get_mr(self.api(), id).await
    }
    async fn create_mr(&self, repo: &RepoRef, req: MrCreateRequest) -> Result<MrInfo, ProviderError> {
        mr::create_mr(self.api(), repo, req).await
    }
    async fn update_mr(&self, id: &MrId, req: MrUpdateRequest) -> Result<MrInfo, ProviderError> {
        mr::update_mr(self.api(), id, req).await
    }
    async fn close_mr(&self, id: &MrId) -> Result<(), ProviderError> {
        mr::close_mr(self.api(), id).await
    }
    async fn reopen_mr(&self, id: &MrId) -> Result<(), ProviderError> {
        mr::reopen_mr(self.api(), id).await
    }
    async fn merge_mr(&self, id: &MrId, opts: MergeOpts) -> Result<(), ProviderError> {
        mr::merge_mr(self.api(), id, opts).await
    }
    async fn list_mr_comments(&self, id: &MrId) -> Result<Vec<MrComment>, ProviderError> {
        mr::list_mr_comments(self.api(), id).await
    }
    async fn add_mr_comment(&self, id: &MrId, body: &str) -> Result<MrComment, ProviderError> {
        mr::add_mr_comment(self.api(), id, body).await
    }
    async fn list_mr_files(&self, id: &MrId) -> Result<Vec<MrFile>, ProviderError> {
        mr::list_mr_files(self.api(), id).await
    }
    async fn fetch_mr_diff(&self, id: &MrId) -> Result<String, ProviderError> {
        mr::fetch_mr_diff(self.api(), id).await
    }
    async fn check_mr_conflict(&self, id: &MrId) -> Result<MrConflict, ProviderError> {
        mr::check_mr_conflict(self.api(), id).await
    }
    async fn list_mr_reviewers(&self, id: &MrId) -> Result<Vec<ProviderUser>, ProviderError> {
        mr::list_mr_reviewers(self.api(), id).await
    }
    async fn request_mr_review(&self, id: &MrId, user: &str) -> Result<(), ProviderError> {
        mr::request_mr_review(self.api(), id, user).await
    }
    async fn approve_mr(&self, id: &MrId) -> Result<(), ProviderError> {
        mr::approve_mr(self.api(), id).await
    }

    // ── CI / CD ──────────────────────────────────────────────────────────
    async fn list_ci_runs(&self, repo: &RepoRef, filter: CiFilter) -> Result<Vec<CiRun>, ProviderError> {
        ci::list_ci_runs(self.api(), repo, filter).await
    }
    async fn get_ci_run(&self, repo: &RepoRef, run_id: &str) -> Result<CiRun, ProviderError> {
        ci::get_ci_run(self.api(), repo, run_id).await
    }
    async fn fetch_ci_jobs(&self, repo: &RepoRef, run_id: &str) -> Result<Vec<CiJob>, ProviderError> {
        ci::fetch_ci_jobs(self.api(), repo, run_id).await
    }
    async fn fetch_ci_job_log(&self, repo: &RepoRef, job_id: &str) -> Result<String, ProviderError> {
        ci::fetch_ci_job_log(self.api(), repo, job_id).await
    }
    async fn retrigger_ci_run(&self, repo: &RepoRef, run_id: &str) -> Result<(), ProviderError> {
        ci::retrigger_ci_run(self.api(), repo, run_id).await
    }
    async fn cancel_ci_run(&self, repo: &RepoRef, run_id: &str) -> Result<(), ProviderError> {
        ci::cancel_ci_run(self.api(), repo, run_id).await
    }
    async fn list_ci_workflows(&self, repo: &RepoRef) -> Result<Vec<CiWorkflow>, ProviderError> {
        ci::list_ci_workflows(self.api(), repo).await
    }
    async fn create_ci_pipeline(&self, repo: &RepoRef, req: PipelineCreateRequest) -> Result<CiRun, ProviderError> {
        ci::create_ci_pipeline(self.api(), repo, req).await
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
        security::supports_security(self.api(), repo).await
    }
    async fn fetch_security_summary(&self, repo: &RepoRef, range_days: u32) -> Result<SecuritySummary, ProviderError> {
        security::fetch_security_summary(self.api(), repo, range_days).await
    }
    async fn fetch_security_findings(&self, repo: &RepoRef, filters: SecurityFilters) -> Result<Vec<SecurityFinding>, ProviderError> {
        security::fetch_security_findings(self.api(), repo, filters).await
    }
}
