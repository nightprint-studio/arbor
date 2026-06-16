//! `struct GithubProvider` + `impl GitProvider` — keyring-free assembly.
//!
//! Mirrors the old `git_provider::github::mod` EXACTLY (same kind/host/
//! web_base_url/capabilities, identical `Unsupported` feature strings, same
//! OAuth-Unsupported messages), but each method delegates to this crate's
//! domain free-fns passing `&self.http` instead of the old `github/<domain>`
//! delegates. Credentials/OAuth/revoke stay out-of-band in the shell.

use std::sync::Arc;

use async_trait::async_trait;

use arbor_ipc::prelude::SessionProvider;
use corvus_git_provider_api::prelude::*;

use crate::http::GithubHttp;
use crate::{auth, branch, ci, issues, mr, releases, repo, security, webhooks};

/// A GitHub provider bound to one account's injected credentials.
///
/// Single host-keyed instance: one `GithubProvider` serves every `github.com`
/// repo on the user's tabs. Repo context is supplied via `RepoRef` / `MrId`
/// parameters on each method.
pub struct GithubProvider {
    http: GithubHttp,
}

impl GithubProvider {
    /// Build a provider. `account` is the opaque credential path the shell maps
    /// to the keyring (the shell passes `"github.com"`); the base URL + auth
    /// header come from the injected session.
    pub fn new(session: Arc<dyn SessionProvider>, account: impl Into<String>) -> Self {
        Self { http: GithubHttp::new(session, account) }
    }
}

#[async_trait]
impl GitProvider for GithubProvider {
    // ── Identity ─────────────────────────────────────────────────────────
    fn kind(&self) -> ProviderKind { ProviderKind::GitHub }
    fn host(&self) -> &str { "github.com" }
    fn web_base_url(&self) -> &str { "https://github.com" }
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
    fn has_token(&self) -> bool { self.http.has_credentials() }
    async fn current_user(&self) -> Result<ProviderUser, ProviderError> {
        auth::current_user(&self.http).await
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
        // The shell still drives OAuth + revoke out-of-band; the crate must NOT
        // call oauth/keyring code.
        Err(ProviderError::Unsupported {
            feature: "revoke_token (handled by oauth::github::revoke_token)".into(),
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
