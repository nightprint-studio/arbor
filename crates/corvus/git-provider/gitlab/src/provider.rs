//! `struct GitlabProvider` + `impl GitProvider` — keyring-free assembly.
//!
//! Mirrors the old `git_provider::gitlab::mod` EXACTLY (same kind/host/
//! web_base_url/capabilities, identical `Unsupported` feature strings), but each
//! method delegates to this crate's domain free-fns passing `&self.http` instead
//! of the old `gitlab/<domain>` delegates. Credentials/OAuth/revoke stay
//! out-of-band in the shell.
//!
//! One instance per host: the shell registers a default for `gitlab.com` and
//! adds self-hosted instances via [`GitlabProvider::new_self_hosted`] when they
//! are discovered through the credential store. For GitLab the injected
//! `SessionProvider` `account` IS the instance base URL.

use std::sync::Arc;

use async_trait::async_trait;

use arbor_ipc::prelude::SessionProvider;
use corvus_git_provider_api::prelude::*;

use crate::http::GitlabHttp;
use crate::{auth, branch, ci, issues, mr, releases, repo, security, webhooks};

/// A GitLab provider bound to one instance's injected credentials.
pub struct GitlabProvider {
    http:         GitlabHttp,
    host:         String,
    web_base_url: String,
    /// `true` for instances other than `gitlab.com` — disables OAuth and uses
    /// host-keyed credentials.
    self_hosted:  bool,
}

impl GitlabProvider {
    /// Default `gitlab.com` instance (OAuth-enabled). The session `account`
    /// (credential base URL) is `https://gitlab.com`.
    pub fn new(session: Arc<dyn SessionProvider>) -> Self {
        Self {
            http:         GitlabHttp::new(session, "https://gitlab.com"),
            host:         "gitlab.com".into(),
            web_base_url: "https://gitlab.com".into(),
            self_hosted:  false,
        }
    }

    /// Self-hosted instance. `host` is the *host* root
    /// (e.g. `https://gitlab.example.org`); the session `account` IS that
    /// trimmed base URL, and `/api/v4`/`/api/graphql` are appended by callees.
    pub fn new_self_hosted(session: Arc<dyn SessionProvider>, host: impl Into<String>) -> Self {
        let host_str: String = host.into();
        let trimmed = host_str.trim_end_matches('/').to_string();
        // Extract just the bare hostname for registry lookups.
        let bare_host = trimmed
            .strip_prefix("https://")
            .or_else(|| trimmed.strip_prefix("http://"))
            .unwrap_or(&trimmed)
            .to_string();
        Self {
            http:         GitlabHttp::new(session, trimmed.clone()),
            host:         bare_host,
            web_base_url: trimmed,
            self_hosted:  true,
        }
    }
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
    fn has_token(&self) -> bool { self.http.has_credentials() }
    async fn current_user(&self) -> Result<ProviderUser, ProviderError> {
        auth::current_user(&self.http).await
    }
    async fn start_oauth(&self) -> Result<OAuthHandle, ProviderError> {
        // Legacy flow needs an AppHandle the trait can't carry — command layer
        // drives via `oauth::gitlab::start` instead.
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
        // The shell still drives OAuth + revoke out-of-band; the crate must NOT
        // call oauth/keyring code.
        Err(ProviderError::Unsupported {
            feature: "revoke_token (handled by oauth::gitlab::revoke_token)".into(),
        })
    }

    // ── Repo CRUD ────────────────────────────────────────────────────────
    async fn create_repo(&self, req: RepoCreateRequest) -> Result<RemoteRepoInfo, ProviderError> {
        repo::create_repo(&self.http, req).await
    }
    async fn get_repo(&self, owner: &str, name: &str) -> Result<RemoteRepoInfo, ProviderError> {
        repo::get_repo(&self.http, owner, name).await
    }
    async fn list_user_repos(&self, opts: ListReposOpts) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
        repo::list_user_repos(&self.http, opts).await
    }
    async fn list_org_repos(&self, org: &str, opts: ListReposOpts) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
        repo::list_org_repos(&self.http, org, opts).await
    }
    async fn search_repos(&self, query: &str) -> Result<Vec<RemoteRepoInfo>, ProviderError> {
        repo::search_repos(&self.http, query).await
    }

    // ── MR / PR ──────────────────────────────────────────────────────────
    async fn list_mrs(&self, repo: &RepoRef, filter: MrFilter) -> Result<Vec<MrInfo>, ProviderError> {
        mr::list_mrs(&self.http, repo, filter).await
    }
    async fn get_mr(&self, id: &MrId) -> Result<MrDetail, ProviderError> {
        mr::get_mr(&self.http, id).await
    }
    async fn create_mr(&self, repo: &RepoRef, req: MrCreateRequest) -> Result<MrInfo, ProviderError> {
        mr::create_mr(&self.http, repo, req).await
    }
    async fn update_mr(&self, id: &MrId, req: MrUpdateRequest) -> Result<MrInfo, ProviderError> {
        mr::update_mr(&self.http, id, req).await
    }
    async fn close_mr(&self, id: &MrId) -> Result<(), ProviderError> {
        mr::close_mr(&self.http, id).await
    }
    async fn reopen_mr(&self, id: &MrId) -> Result<(), ProviderError> {
        mr::reopen_mr(&self.http, id).await
    }
    async fn merge_mr(&self, id: &MrId, opts: MergeOpts) -> Result<(), ProviderError> {
        mr::merge_mr(&self.http, id, opts).await
    }
    async fn list_mr_comments(&self, id: &MrId) -> Result<Vec<MrComment>, ProviderError> {
        mr::list_mr_comments(&self.http, id).await
    }
    async fn add_mr_comment(&self, id: &MrId, body: &str) -> Result<MrComment, ProviderError> {
        mr::add_mr_comment(&self.http, id, body).await
    }
    async fn list_mr_files(&self, id: &MrId) -> Result<Vec<MrFile>, ProviderError> {
        mr::list_mr_files(&self.http, id).await
    }
    async fn fetch_mr_diff(&self, id: &MrId) -> Result<String, ProviderError> {
        mr::fetch_mr_diff(&self.http, id).await
    }
    async fn check_mr_conflict(&self, id: &MrId) -> Result<MrConflict, ProviderError> {
        mr::check_mr_conflict(&self.http, id).await
    }
    async fn list_mr_reviewers(&self, id: &MrId) -> Result<Vec<ProviderUser>, ProviderError> {
        mr::list_mr_reviewers(&self.http, id).await
    }
    async fn request_mr_review(&self, id: &MrId, user: &str) -> Result<(), ProviderError> {
        mr::request_mr_review(&self.http, id, user).await
    }
    async fn approve_mr(&self, id: &MrId) -> Result<(), ProviderError> {
        mr::approve_mr(&self.http, id).await
    }

    // ── CI / CD ──────────────────────────────────────────────────────────
    async fn list_ci_runs(&self, repo: &RepoRef, filter: CiFilter) -> Result<Vec<CiRun>, ProviderError> {
        ci::list_ci_runs(&self.http, repo, filter).await
    }
    async fn get_ci_run(&self, repo: &RepoRef, run_id: &str) -> Result<CiRun, ProviderError> {
        ci::get_ci_run(&self.http, repo, run_id).await
    }
    async fn fetch_ci_jobs(&self, repo: &RepoRef, run_id: &str) -> Result<Vec<CiJob>, ProviderError> {
        ci::fetch_ci_jobs(&self.http, repo, run_id).await
    }
    async fn fetch_ci_job_log(&self, repo: &RepoRef, job_id: &str) -> Result<String, ProviderError> {
        ci::fetch_ci_job_log(&self.http, repo, job_id).await
    }
    async fn retrigger_ci_run(&self, repo: &RepoRef, run_id: &str) -> Result<(), ProviderError> {
        ci::retrigger_ci_run(&self.http, repo, run_id).await
    }
    async fn cancel_ci_run(&self, repo: &RepoRef, run_id: &str) -> Result<(), ProviderError> {
        ci::cancel_ci_run(&self.http, repo, run_id).await
    }
    async fn list_ci_workflows(&self, repo: &RepoRef) -> Result<Vec<CiWorkflow>, ProviderError> {
        ci::list_ci_workflows(&self.http, repo).await
    }
    async fn create_ci_pipeline(&self, repo: &RepoRef, req: PipelineCreateRequest) -> Result<CiRun, ProviderError> {
        ci::create_ci_pipeline(&self.http, repo, req).await
    }

    // ── Releases (STUB) ──────────────────────────────────────────────────
    async fn list_releases(&self, repo: &RepoRef) -> Result<Vec<Release>, ProviderError> {
        releases::list_releases(&self.http, repo).await
    }
    async fn get_release(&self, repo: &RepoRef, id: &str) -> Result<Release, ProviderError> {
        releases::get_release(&self.http, repo, id).await
    }
    async fn create_release(&self, repo: &RepoRef, req: ReleaseCreateRequest) -> Result<Release, ProviderError> {
        releases::create_release(&self.http, repo, req).await
    }
    async fn delete_release(&self, repo: &RepoRef, id: &str) -> Result<(), ProviderError> {
        releases::delete_release(&self.http, repo, id).await
    }

    // ── Repo issues (STUB) ───────────────────────────────────────────────
    async fn list_repo_issues(&self, repo: &RepoRef, filter: IssueFilter) -> Result<Vec<RepoIssue>, ProviderError> {
        issues::list_repo_issues(&self.http, repo, filter).await
    }
    async fn get_repo_issue(&self, repo: &RepoRef, id: &str) -> Result<RepoIssue, ProviderError> {
        issues::get_repo_issue(&self.http, repo, id).await
    }
    async fn create_repo_issue(&self, repo: &RepoRef, req: IssueCreateRequest) -> Result<RepoIssue, ProviderError> {
        issues::create_repo_issue(&self.http, repo, req).await
    }
    async fn comment_repo_issue(&self, repo: &RepoRef, id: &str, body: &str) -> Result<(), ProviderError> {
        issues::comment_repo_issue(&self.http, repo, id, body).await
    }
    async fn close_repo_issue(&self, repo: &RepoRef, id: &str) -> Result<(), ProviderError> {
        issues::close_repo_issue(&self.http, repo, id).await
    }

    // ── Webhooks (STUB) ──────────────────────────────────────────────────
    async fn list_webhooks(&self, repo: &RepoRef) -> Result<Vec<Webhook>, ProviderError> {
        webhooks::list_webhooks(&self.http, repo).await
    }
    async fn create_webhook(&self, repo: &RepoRef, req: WebhookCreateRequest) -> Result<Webhook, ProviderError> {
        webhooks::create_webhook(&self.http, repo, req).await
    }
    async fn delete_webhook(&self, repo: &RepoRef, id: &str) -> Result<(), ProviderError> {
        webhooks::delete_webhook(&self.http, repo, id).await
    }

    // ── Branches via REST (STUB) ─────────────────────────────────────────
    async fn list_remote_branches(&self, repo: &RepoRef) -> Result<Vec<String>, ProviderError> {
        branch::list_remote_branches(&self.http, repo).await
    }
    async fn get_default_branch(&self, repo: &RepoRef) -> Result<String, ProviderError> {
        branch::get_default_branch(&self.http, repo).await
    }
    async fn protect_branch(&self, repo: &RepoRef, branch: &str, req: BranchProtection) -> Result<(), ProviderError> {
        crate::branch::protect_branch(&self.http, repo, branch, req).await
    }

    // ── Security dashboard ───────────────────────────────────────────────
    async fn supports_security(&self, repo: &RepoRef) -> Result<bool, ProviderError> {
        security::supports_security(&self.http, repo).await
    }
    async fn fetch_security_summary(&self, repo: &RepoRef, range_days: u32) -> Result<SecuritySummary, ProviderError> {
        security::fetch_security_summary(&self.http, repo, range_days).await
    }
    async fn fetch_security_findings(&self, repo: &RepoRef, filters: SecurityFilters) -> Result<Vec<SecurityFinding>, ProviderError> {
        security::fetch_security_findings(&self.http, repo, filters).await
    }
}
