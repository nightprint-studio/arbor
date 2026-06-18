use serde::{Deserialize, Serialize};
use tauri::State;

use crate::error::AppError;
use crate::git::repo::RepoInfo;
use crate::git::init::InitRepoOptions;
use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitRepoResult {
    pub info: RepoInfo,
    pub remote_url: Option<String>,
    pub pushed: bool,
    pub push_error: Option<String>,
}

/// Initialise a new git repository, create optional files (.gitignore,
/// LICENSE, README), optionally create a remote repo via the provider API,
/// and make an initial commit. Fires the `on_repo_init` plugin hook.
#[tauri::command]
pub async fn init_repo(
    state:   State<'_, AppState>,
    path:    String,
    tab_id:  String,
    options: InitRepoOptions,
) -> std::result::Result<InitRepoResult, AppError> {
    // Step 0 — when the caller asked for a remote provider but didn't supply
    // an explicit URL, create the remote repo through the GitProvider
    // registry so init() only ever sees a fully-formed URL.
    let mut effective = options.clone();
    if effective.remote_url.trim().is_empty()
        && !effective.provider.is_empty()
        && effective.provider != "none"
    {
        let url = create_remote_via_provider(&state, &path, &effective).await?;
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
        serde_json::json!({
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
    use crate::git_provider::types::{RepoCreateRequest, RepoVisibility};

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

// `clone_repo` migrated to the corvus broker (`ipc/corvus/repo.rs`) — it is a
// pure clone-to-disk returning the fresh repo's metadata (no tab opened; the
// frontend opens the tab afterwards via `open_repo`). `init_repo` stays here
// for the credential pass.
