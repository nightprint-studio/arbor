//! `repo` domain — leaf repository queries/metadata routed through the
//! in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name.
//!
//! The repo-lifecycle flow lives here (`open_repo` / `close_repo`). What stays
//! is path validation + the lifecycle: both mutate the open-repo set and mirror
//! into `corvus-be` (`sync_repo_open/close`) but emit through the backend event
//! sink (`state.emit`) and take no `AppHandle`. The pure `git`-identity /
//! metadata probes (`get_git_identity`, `get_repo_info`) and the path / network
//! probes (`check_is_git_repo`, `clone_repo`, `list_remote_branches_for_url`)
//! moved to `corvus-be` (`crate::repo_ops` there).
//!
//! `init_repo` is here as well: it touches the git-provider registry + a host
//! token from the keyring to create the remote, but that all works in-process
//! (`state.lock_git_providers()` / `credential_store::get`), exactly like the
//! already-migrated `fetch`/`push`/`pull`. The M3 credential broker is only
//! needed once `corvus-be` runs out-of-process — not for this in-process seam.
//!
//! The `on_repo_open` / `on_repo_close` (and the orphan-GC `on_repo_deregistered`)
//! hooks are fire-and-forget and fire inline after the repo lock is dropped,
//! with first-hand data.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::error::AppError;
use crate::git::init::InitRepoOptions;
use crate::git::repo::RepoInfo;
use crate::ipc::corvus;
use crate::AppState;

/// Open the repository at `path` under `tab_id` in the repo manager.
///
/// Fires `on_repo_open` inline (after the repo lock is dropped) and calls
/// `sync_repo_open` to mirror the open into `corvus-be`.
#[corvus::handler]
fn open_repo(state: &AppState, path: String, tab_id: String) -> Result<RepoInfo, AppError> {
    let info = {
        let mut mgr = state.lock_repos()?;
        mgr.open(tab_id.clone(), &path)?
    };
    crate::ipc::sync_repo_open(state, &tab_id, &info.path);
    // Fire inline with first-hand data; the repo lock is already dropped above
    // so Lua git ops in the hook can't deadlock against our guard.
    state.fire_hook(
        "on_repo_open",
        json!({ "tab_id": tab_id, "path": info.path, "name": info.name }),
    );
    Ok(info)
}

/// Close the tab `tab_id` in the repo manager.
///
/// Fires `on_repo_close` inline (after the repo lock is dropped), mirrors the
/// close into `corvus-be` via `sync_repo_close`, then runs the orphan GC: a
/// repo with no open tab and no workspace membership is forgotten (registry
/// entry + recent-repos pointer dropped, `on_repo_deregistered` fired), and
/// `arbor://registry-changed` is emitted so the explorer's Projects view
/// refreshes.
#[corvus::handler]
fn close_repo(state: &AppState, tab_id: String) -> Result<(), AppError> {
    let (path, name) = {
        let mut mgr = state.lock_repos()?;
        let info = mgr.get(&tab_id)
            .map(|r| (r.path.clone(), r.name.clone()))
            .unwrap_or_default();
        mgr.close(&tab_id);
        info
    };
    state.fire_hook(
        "on_repo_close",
        json!({ "tab_id": &tab_id, "path": &path, "name": &name }),
    );
    crate::ipc::sync_repo_close(state, &tab_id);

    // The shared GC helper re-checks both orphan conditions (no open tab AND no
    // workspace membership) itself before dropping the registry entry.
    if !path.is_empty() {
        let repo_id = state.lock_repo_registry()
            .ok()
            .and_then(|reg| reg.find_by_path(&path).map(|e| e.id.clone()));
        if let Some(id) = repo_id {
            let forgotten = crate::commands::workspace_commands::forget_repo_if_orphaned(
                state, &id, "tab_closed_when_orphan",
            ).unwrap_or(false);
            // Dropping the registry entry changes the explorer's Projects view.
            // `()` serializes to JSON null — byte-identical to the old `app.emit`.
            if forgotten {
                state.emit("arbor://registry-changed", ());
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Init (with optional remote creation via the git provider)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitRepoResult {
    pub info: RepoInfo,
    pub remote_url: Option<String>,
    pub pushed: bool,
    pub push_error: Option<String>,
}

/// Initialise a new git repository, create optional files (.gitignore,
/// LICENSE, README), optionally create a remote repo via the provider API,
/// and make an initial commit. Opens the result under `tab_id` and fires the
/// `on_repo_init` plugin hook.
#[corvus::handler]
async fn init_repo(
    state:   &AppState,
    path:    String,
    tab_id:  String,
    options: InitRepoOptions,
) -> Result<InitRepoResult, AppError> {
    // Step 0 — when the caller asked for a remote provider but didn't supply
    // an explicit URL, create the remote repo through the GitProvider
    // registry so init() only ever sees a fully-formed URL.
    let mut effective = options.clone();
    if effective.remote_url.trim().is_empty()
        && !effective.provider.is_empty()
        && effective.provider != "none"
    {
        let url = create_remote_via_provider(state, &path, &effective).await?;
        effective.remote_url = url;
    }

    // Step 1 — initialise the repository.
    let outcome = crate::git::init::init(&path, &effective).await?;

    // Step 2 — open it in the repo manager (sync, must not hold lock across await).
    let info = {
        let mut mgr = state.lock_repos()?;
        mgr.open(tab_id, &path)?
    };

    // Step 3 — fire on_repo_init plugin hook.
    state.fire_hook(
        "on_repo_init",
        json!({
            "path":           &info.path,
            "name":           &info.name,
            "default_branch": &options.default_branch,
            "provider":       &options.provider,
            "remote_url":     outcome.remote_url.as_deref().unwrap_or(""),
            "pushed":         outcome.pushed,
            "has_readme":     options.readme,
            "license":        &options.license,
            "gitignore":      &options.gitignore_template,
        }),
    );

    Ok(InitRepoResult {
        info,
        remote_url: outcome.remote_url,
        pushed: outcome.pushed,
        push_error: outcome.push_error,
    })
}

/// Create the remote repo on `opts.provider` via the GitProvider registry,
/// returning the HTTPS clone URL.  Used by `init_repo` to externalise the
/// host-specific REST call so `git::init::init` stays provider-agnostic.
async fn create_remote_via_provider(
    state: &AppState,
    path:  &str,
    opts:  &InitRepoOptions,
) -> Result<String, AppError> {
    use std::path::Path;
    use corvus_git_provider_api::prelude::{RepoCreateRequest, RepoVisibility};

    let host = match opts.provider.as_str() {
        "github" => "github.com",
        "gitlab" => "gitlab.com",
        other => return Err(AppError::Other(
            format!("Unknown remote provider: {other}"),
        )),
    };

    let provider = {
        let registry = state.lock_git_providers()?;
        registry.for_host(host).ok_or_else(|| AppError::Other(
            format!("No GitProvider registered for host '{host}'"),
        ))?
    };

    let name = Path::new(path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".to_string());

    let visibility = if opts.visibility == "public" {
        RepoVisibility::Public
    } else {
        RepoVisibility::Private
    };

    let (org, namespace_id) = match opts.provider.as_str() {
        "github" => (
            if opts.org.trim().is_empty() { None } else { Some(opts.org.trim().to_string()) },
            None,
        ),
        "gitlab" => {
            let ns_id = if opts.org.trim().is_empty() {
                None
            } else {
                resolve_gitlab_namespace_id(opts.org.trim()).await?
            };
            (None, ns_id)
        }
        _ => (None, None),
    };

    let req = RepoCreateRequest {
        name,
        description: if opts.description.trim().is_empty() {
            None
        } else {
            Some(opts.description.trim().to_string())
        },
        visibility,
        org,
        namespace_id,
    };

    let info = provider.create_repo(req).await
        .map_err(|e| AppError::Other(e.to_string()))?;
    Ok(info.clone_url_https)
}

/// Resolve a GitLab namespace path (e.g. "myorg" or "myorg/team") to its
/// numeric `namespace_id` so `RepoCreateRequest` can carry it. GitLab's
/// `/projects` POST requires the numeric id, not the path.
async fn resolve_gitlab_namespace_id(path: &str) -> Result<Option<u64>, AppError> {
    let token = crate::auth::credential_store::get("gitlab.com/arbor", "oauth")?
        .or_else(|| {
            crate::auth::credential_store::get_for_host("gitlab.com")
                .ok()
                .flatten()
                .map(|(_, tok)| tok)
        });
    let Some(token) = token else { return Ok(None); };

    let url = format!("https://gitlab.com/api/v4/namespaces?search={path}");
    let resp = reqwest::Client::new()
        .get(&url)
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .map_err(|e| AppError::Other(format!("GitLab namespace lookup failed: {e}")))?;
    if !resp.status().is_success() { return Ok(None); }
    let arr = resp.json::<serde_json::Value>().await
        .map_err(|e| AppError::Other(format!("GitLab namespace parse error: {e}")))?;
    let id = arr.as_array()
        .and_then(|a| {
            a.iter()
                .find(|n| n["path"].as_str() == Some(path))
                .or_else(|| a.first())
        })
        .and_then(|n| n["id"].as_u64());
    Ok(id)
}
