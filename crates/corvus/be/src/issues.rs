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
//! The whole issue-tracker domain now lives here, including the connect surface
//! relocated from the shell (descriptors, provider auth-status, field
//! validation, disconnect, `jira_get_auth_status`, the inline-image proxy) — see
//! the "Provider connect" section below. The launcher keeps only the OAuth engine
//! and the keyring vault: keyring WRITES (save / clear) and the Jira
//! keyring-derived metadata (domain + auth method) cross back over the reverse
//! channel (`__save_credential` / `__delete_credential` / `__jira_auth_meta`).
//! The shell's `SplitBroker` routes per-method, so this is transparent to the FE.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, OnceLock};

use arbor_ipc::prelude::{ChildSessionProvider, HostCaller, SessionProvider};
use corvus_core::prelude::CorvusState;
use corvus_issues::prelude::{
    build_registry, jira_new_issue, linear_new_issue, validate_token, AuthStatus, Issue,
    IssueComment, IssueFilterOptions, IssueFilters, IssueTracker, IssueTrackerError,
    IssueTrackerRegistry, JiraAuthStatus, JiraTracker, LinearAuthStatus, LINEAR_GQL,
};
// FE-facing provider-connect types (distinct from the tracker's `AuthStatus`
// above): the shared shape the generic settings UI renders.
use corvus_provider_descriptor::prelude::{
    AuthStatus as ProviderAuthStatus, ProviderDescriptor, ProviderUserInfo,
};
use serde_json::{json, Value};

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

// ── Provider connect / metadata (relocated from the shell) ───────────────────────
//
// These used to live in the shell (`integrations/*` + `provider_connect/issue.rs`),
// keyring-coupled. They now run here over the reverse channel: validation/auth
// resolve credentials through the `ChildSessionProvider`, and the keyring WRITES
// (save / delete) plus the Jira keyring-derived metadata (domain + auth method)
// cross back to the shell's vault via `host_call("__save_credential" / …)`. Only
// the OAuth engine (`*_start_oauth`) stays shell-side.

/// Map the issue-tracker-domain [`AuthStatus`] onto the shared FE-facing
/// [`ProviderAuthStatus`] (byte-identical to the shell's old `map_issue_auth_status`).
fn map_issue_auth_status(s: AuthStatus) -> ProviderAuthStatus {
    ProviderAuthStatus {
        authenticated: s.authenticated,
        user: s.user.map(|u| ProviderUserInfo {
            display_name: u.display_name,
            email:        u.email,
            avatar_url:   u.avatar_url,
        }),
        account_label: s.domain,
        method:        s.auth_method,
    }
}

/// List the registered issue-tracker providers with their self-describing connect
/// forms (id, icon, auth methods + fields). Drives the generic settings UI.
#[arbor_rpc::handler]
fn list_issue_providers(_ctx: &CorvusState) -> Result<Vec<ProviderDescriptor>, String> {
    Ok(registry().descriptors())
}

/// Suggest a git branch name for an issue (`{lower-identifier}-{slugified-title}`).
#[arbor_rpc::handler]
fn branch_name_for_issue(_ctx: &CorvusState, issue: Issue) -> Result<String, String> {
    Ok(corvus_issues::prelude::branch_name_for_issue(&issue))
}

/// Current auth state of an issue-tracker provider, mapped onto the shared shape.
#[arbor_rpc::handler]
async fn issue_provider_auth_status(_ctx: &CorvusState, id: String) -> Result<ProviderAuthStatus, String> {
    let unauth = || ProviderAuthStatus {
        authenticated: false,
        user:          None,
        account_label: None,
        method:        None,
    };
    let Some(tracker) = registry().get(&id) else { return Ok(unauth()) };
    match tracker.auth_status().await {
        Ok(s) => Ok(map_issue_auth_status(s)),
        Err(_) => Ok(unauth()),
    }
}

/// Save `Fields`-method credentials for an issue-tracker provider. Validates the
/// supplied credentials, then asks the shell's vault to persist them
/// (`__save_credential`). Linear validates the raw token first; Jira's tracker
/// validates by reading the just-saved keyring config, so the save precedes the
/// `/myself` probe (identical ordering to the former in-process path).
#[arbor_rpc::handler]
async fn issue_provider_connect_fields(
    ctx: &CorvusState,
    id: String,
    method_id: String,
    fields: HashMap<String, String>,
) -> Result<(), String> {
    let f = |k: &str| fields.get(k).map(|s| s.as_str()).unwrap_or("");
    match (id.as_str(), method_id.as_str()) {
        ("linear", "pat") => {
            let token = f("token");
            validate_token(token, LINEAR_GQL).await.map_err(err)?;
            ctx.host_call(
                "__save_credential",
                json!({ "provider": "linear", "fields": { "token": token } }),
            )?;
            Ok(())
        }
        ("jira", "basic") => {
            ctx.host_call(
                "__save_credential",
                json!({ "provider": "jira", "fields": {
                    "email": f("email"), "api_token": f("api_token"), "domain": f("domain"),
                } }),
            )?;
            jira()
                .current_user()
                .await
                .map_err(|e| format!("Authentication failed: Jira /myself failed: {}", err(e)))?;
            Ok(())
        }
        (p, "pat") | (p, "basic") => Err(format!("{p}: unsupported fields method")),
        (p, m) => Err(format!("{p}: unknown fields method '{m}'")),
    }
}

/// Remove all stored credentials for an issue-tracker provider (via the vault).
#[arbor_rpc::handler]
async fn issue_provider_disconnect(ctx: &CorvusState, id: String) -> Result<(), String> {
    ctx.host_call("__delete_credential", json!({ "provider": id }))?;
    Ok(())
}

/// Jira auth status (authenticated flag + user + domain + method). The domain +
/// auth method are keyring-derived, so they cross from the shell's vault
/// (`__jira_auth_meta`); the user comes from a `/myself` probe.
#[arbor_rpc::handler]
async fn jira_get_auth_status(ctx: &CorvusState) -> Result<JiraAuthStatus, String> {
    let unauth = || JiraAuthStatus { authenticated: false, user: None, domain: None, auth_method: None };
    let meta = ctx.host_call("__jira_auth_meta", Value::Null)?;
    if meta.is_null() {
        return Ok(unauth());
    }
    let status = jira().auth_status().await.map_err(err)?;
    if status.authenticated {
        Ok(JiraAuthStatus {
            authenticated: true,
            user:          status.user,
            domain:        meta.get("domain").and_then(|v| v.as_str()).map(String::from),
            auth_method:   meta.get("auth_method").and_then(|v| v.as_str()).map(String::from),
        })
    } else {
        Ok(unauth())
    }
}

/// Authenticated inline-image proxy for an issue-tracker provider (Jira / Linear).
#[arbor_rpc::handler]
async fn issue_fetch_image(
    _ctx: &CorvusState,
    provider: String,
    url: String,
) -> Result<(Vec<u8>, Option<String>), String> {
    let tracker = registry()
        .get(&provider)
        .ok_or_else(|| format!("unknown issue provider '{provider}'"))?;
    tracker.fetch_image_bytes(&url).await.map_err(err)
}
