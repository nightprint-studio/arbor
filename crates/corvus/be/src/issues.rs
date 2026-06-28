//! `issues` domain — Linear / Jira issue-tracker handlers, served
//! out-of-process by `corvus-be`.
//!
//! These mirror the shell's `crate::integrations::*`-backed handlers
//! (`src-tauri/src/ipc/corvus/issues.rs`) one-for-one, but resolve credentials
//! over the **reverse channel** instead of the keyring: the
//! [`build_registry`](corvus_issues::prelude::build_registry) here is injected a
//! [`ChildSessionProvider`], which marshals `session`/`refresh` back to the
//! shell's `VaultSessionProvider` (the sole keyring holder). The handler logic
//! is otherwise identical, down to the error wire string (see [`err`]).
//!
//! What does **not** move here: `jira_get_auth_status` (it reads the keyring
//! config directly for domain/auth-method — keyring-coupled, stays in the
//! shell) and the two pure/metadata sync helpers (`list_issue_providers`,
//! `branch_name_for_issue`) — those keep being served in-process. The shell's
//! `SplitBroker` routes per-method, so the domain splitting across the two
//! processes is transparent to the caller.

use std::path::Path;
use std::sync::{Arc, OnceLock};

use arbor_ipc::prelude::{ChildSessionProvider, HostCaller, SessionProvider};
use corvus_core::prelude::CorvusState;
use corvus_issues::prelude::{
    build_registry, jira_new_issue, linear_new_issue, Issue, IssueComment, IssueFilterOptions,
    IssueFilters, IssueTracker, IssueTrackerError, IssueTrackerRegistry, JiraTracker,
    LinearAuthStatus,
};

/// The reverse-channel-backed tracker registry, built once from the shell's
/// `HostCaller`. Both trackers share a single `ChildSessionProvider` factory —
/// the provider is account-agnostic (it forwards the tracker's stored `account`
/// through `session(account)`), so the same instance serves Linear and Jira.
static ISSUES: OnceLock<(IssueTrackerRegistry, Arc<JiraTracker>)> = OnceLock::new();

/// Wire the issue-tracker registry to the reverse channel. Called once from
/// `main` after the `FrameHostCaller` is built; idempotent (a second call is a
/// no-op via `OnceLock::set`).
pub fn init(host: Arc<dyn HostCaller>) {
    let _ = ISSUES.set(build_registry(move |_id| {
        Arc::new(ChildSessionProvider::new(Arc::clone(&host))) as Arc<dyn SessionProvider>
    }));
}

fn issues() -> &'static (IssueTrackerRegistry, Arc<JiraTracker>) {
    ISSUES.get().expect("issues::init must run before dispatch")
}

/// The registered Linear tracker (always present once the registry is built).
/// `pub(crate)` so `CorvusNsHost`'s `arbor.issues.*` DIRECT methods (in `main.rs`)
/// can drive the same tracker the OOP handlers do.
pub(crate) fn linear() -> Arc<dyn IssueTracker> {
    issues().0.get("linear").expect("linear tracker is always registered")
}

/// The whole tracker registry — the `arbor.issues.lookup` path routes per-repo
/// (resolving the repo's configured tracker name → its tracker), so it needs the
/// full registry, not just the Linear handle. `pub(crate)` for `CorvusNsHost`.
pub(crate) fn registry() -> &'static IssueTrackerRegistry {
    &issues().0
}

/// The concrete Jira handle (its inherent methods aren't on the trait).
fn jira() -> Arc<JiraTracker> {
    issues().1.clone()
}

/// Map a tracker error to the **exact** wire string the shell's in-process path
/// produces (`to_app_error(e).to_string()`), so an OOP result is byte-identical
/// to the in-process fallback: auth/connection failures gain the
/// `"Authentication failed: "` prefix (`AppError::AuthFailed`'s `Display`),
/// API/network failures pass through verbatim (`AppError::Other`'s `Display`).
pub(crate) fn err(e: IssueTrackerError) -> String {
    match e {
        IssueTrackerError::Auth(m) | IssueTrackerError::NotConnected(m) => {
            format!("Authentication failed: {m}")
        }
        IssueTrackerError::Api(m) | IssueTrackerError::Network(m) => m,
    }
}

// ── Linear ─────────────────────────────────────────────────────────────────────

#[arbor_rpc::handler]
async fn linear_get_auth_status(_ctx: &CorvusState) -> Result<LinearAuthStatus, String> {
    let status = linear().auth_status().await.map_err(err)?;
    Ok(LinearAuthStatus { authenticated: status.authenticated, user: status.user })
}

#[arbor_rpc::handler]
async fn linear_search_issues(_ctx: &CorvusState, filters: IssueFilters) -> Result<Vec<Issue>, String> {
    linear().search_issues(filters).await.map_err(err)
}

#[arbor_rpc::handler]
async fn linear_get_issue(_ctx: &CorvusState, id: String) -> Result<Issue, String> {
    linear().get_issue(&id).await.map_err(err)
}

#[arbor_rpc::handler]
async fn linear_get_filter_options(_ctx: &CorvusState) -> Result<IssueFilterOptions, String> {
    linear().get_filter_options().await.map_err(err)
}

#[arbor_rpc::handler]
async fn linear_transition_issue(
    _ctx: &CorvusState,
    id: String,
    status_id: String,
) -> Result<Issue, String> {
    linear().transition_issue(&id, &status_id).await.map_err(err)
}

#[arbor_rpc::handler]
async fn linear_assign_issue(
    _ctx: &CorvusState,
    id: String,
    user_id: Option<String>,
) -> Result<Issue, String> {
    linear().assign_issue(&id, user_id.as_deref()).await.map_err(err)
}

#[arbor_rpc::handler]
async fn linear_add_comment(
    _ctx: &CorvusState,
    issue_id: String,
    body: String,
) -> Result<IssueComment, String> {
    linear().add_comment(&issue_id, &body).await.map_err(err)
}

#[arbor_rpc::handler]
#[allow(clippy::too_many_arguments)]
async fn linear_create_issue(
    _ctx: &CorvusState,
    title: String,
    description: Option<String>,
    team_id: String,
    status_id: Option<String>,
    assignee_id: Option<String>,
    label_ids: Vec<String>,
    priority: Option<u32>,
    project_id: Option<String>,
    milestone_id: Option<String>,
    due_date: Option<String>,
    estimate: Option<f64>,
) -> Result<Issue, String> {
    let req = linear_new_issue(
        &title,
        description.as_deref(),
        &team_id,
        status_id.as_deref(),
        assignee_id.as_deref(),
        label_ids,
        priority,
        project_id.as_deref(),
        milestone_id.as_deref(),
        due_date.as_deref(),
        estimate,
    );
    linear().create_issue(req).await.map_err(err)
}

// ── Jira ───────────────────────────────────────────────────────────────────────

#[arbor_rpc::handler]
async fn jira_search_issues(_ctx: &CorvusState, filters: IssueFilters) -> Result<Vec<Issue>, String> {
    jira().search_issues(filters).await.map_err(err)
}

#[arbor_rpc::handler]
async fn jira_get_issue(_ctx: &CorvusState, id: String) -> Result<Issue, String> {
    jira().get_issue(&id).await.map_err(err)
}

#[arbor_rpc::handler]
async fn jira_get_filter_options(_ctx: &CorvusState) -> Result<IssueFilterOptions, String> {
    jira().get_filter_options().await.map_err(err)
}

#[arbor_rpc::handler]
async fn jira_transition_issue(
    _ctx: &CorvusState,
    id: String,
    status_id: String,
) -> Result<Issue, String> {
    jira().transition_issue(&id, &status_id).await.map_err(err)
}

#[arbor_rpc::handler]
async fn jira_assign_issue(
    _ctx: &CorvusState,
    id: String,
    user_id: Option<String>,
) -> Result<Issue, String> {
    jira().assign_issue(&id, user_id.as_deref()).await.map_err(err)
}

#[arbor_rpc::handler]
async fn jira_add_comment(
    _ctx: &CorvusState,
    issue_id: String,
    body: String,
) -> Result<IssueComment, String> {
    jira().add_comment(&issue_id, &body).await.map_err(err)
}

#[arbor_rpc::handler]
#[allow(clippy::too_many_arguments)]
async fn jira_create_issue(
    _ctx: &CorvusState,
    title: String,
    description: Option<String>,
    team_id: String,
    status_id: Option<String>,
    assignee_id: Option<String>,
    label_ids: Vec<String>,
    priority: Option<u32>,
    project_id: Option<String>,
    milestone_id: Option<String>,
    due_date: Option<String>,
    estimate: Option<f64>,
    issue_type: Option<String>,
) -> Result<Issue, String> {
    // `project_id` is accepted on the wire (FE sends a uniform shape) but Jira
    // maps it to team/project, so the builder forces it to `None` — same as the
    // shell shim.
    let _ = project_id;
    let req = jira_new_issue(
        &title,
        description.as_deref(),
        &team_id,
        status_id.as_deref(),
        assignee_id.as_deref(),
        label_ids,
        priority,
        milestone_id.as_deref(),
        due_date.as_deref(),
        estimate,
        issue_type.as_deref(),
    );
    jira().create_issue(req).await.map_err(err)
}

/// Download a Jira attachment to `dest_path`. The FE passes an **absolute** path
/// (chosen via the save dialog) — `corvus-be` runs with a different working
/// directory than the shell, so a relative path would resolve elsewhere.
#[arbor_rpc::handler]
async fn jira_download_attachment(
    _ctx: &CorvusState,
    content_url: String,
    dest_path: String,
) -> Result<u64, String> {
    jira()
        .download_attachment(&content_url, Path::new(&dest_path))
        .await
        .map_err(err)
}
