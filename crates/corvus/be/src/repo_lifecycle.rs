//! `repo_lifecycle` domain — repository **initialisation**, served out-of-process
//! by corvus-be.
//!
//! Ported byte-faithfully from the shell's in-process `ipc::corvus::repo::init_repo`
//! (`src-tauri/src/ipc/corvus/repo.rs`). The flow is unchanged:
//!  1. optional remote creation via the git-provider registry (`provider::for_host`,
//!     resolved over the **reverse channel** — the keyring stays shell-side);
//!  2. `corvus_git::init::init` (git init + .gitignore/LICENSE/README + initial
//!     commit + optional push), with the credential-coupled push bound to a
//!     reverse-channel resolver;
//!  3. open the result + fire `on_repo_init`.
//!
//! Two couplings the shell owned in-process are marshalled over the reverse
//! channel here, exactly like the `remote` / `repo_ops` domains:
//!  * **git smart-HTTP credentials** for the initial push → `__git_credentials`
//!    (the same [`credential_resolver`] the `remote` domain uses);
//!  * **GitLab namespace-id resolution** (path → numeric id, a keyring-backed REST
//!    call) → the new `__gitlab_namespace_id` host method (the keyring + REST stay
//!    shell-side; corvus-be only sees the resolved id).
//!
//! The `RepoInfo` returned carries the `tab_id` (the FE adds the tab from it
//! directly — it does **not** issue a follow-up `open_repo`, unlike the clone
//! flow). The handler registers the tab in corvus-be's own registry
//! (`state.register_repo`, so its later OOP handlers resolve the path). The
//! launcher keeps no repo registry of its own — its in-process consumers
//! (studio, plugin host, open-in-browser) resolve the tab by asking corvus-be —
//! so there is nothing to mirror shell-side.
//!
//! Hooks: `on_repo_init` fires here through the backend hook broker (the same one
//! the OOP RPC handlers fire onto), with the identical payload shape.

use std::path::Path;
use std::sync::Arc;

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{InitOutcome, InitRepoOptions, RepoInfo};
use corvus_git_provider_api::prelude::{RepoCreateRequest, RepoVisibility};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::remote::credential_resolver;

/// Mirror of the shell's `ipc::corvus::repo::InitRepoResult` — same serde shape,
/// so the FE `InitRepoResult` decodes byte-identically.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitRepoResult {
    pub info: RepoInfo,
    pub remote_url: Option<String>,
    pub pushed: bool,
    pub push_error: Option<String>,
}

/// Initialise a new git repository, create optional files (.gitignore, LICENSE,
/// README), optionally create a remote repo via the provider API, and make an
/// initial commit. Opens the result under `tab_id` (in both registries) and fires
/// the `on_repo_init` plugin hook.
///
/// Async because remote creation + the optional initial push both `.await`; runs
/// on a serve-loop worker, so the dispatch loop stays responsive.
#[arbor_rpc::handler]
async fn init_repo(
    state: &CorvusState,
    path: String,
    tab_id: String,
    options: InitRepoOptions,
) -> Result<InitRepoResult, String> {
    // Step 0 — when the caller asked for a remote provider but didn't supply an
    // explicit URL, create the remote repo through the provider registry so
    // init() only ever sees a fully-formed URL.
    let mut effective = options.clone();
    if effective.remote_url.trim().is_empty()
        && !effective.provider.is_empty()
        && effective.provider != "none"
    {
        let url = create_remote_via_provider(state, &path, &effective).await?;
        effective.remote_url = url;
    }

    // Step 1 — initialise the repository. The credential-coupled push is bound to
    // `corvus_git::remote::push` with a reverse-channel resolver (the keyring is
    // shell-side); `init` returns `Err(String)` from the closure into
    // `InitOutcome.push_error`, byte-identical to the shell.
    let host = state
        .host_caller()
        .ok_or_else(|| "init_repo: no reverse channel".to_string())?;
    let outcome: InitOutcome = {
        let resolver = credential_resolver(Arc::clone(&host));
        let push = move |repo: &git2::Repository, remote: &str, refspec: &str, force: bool| {
            corvus_git::remote::push(repo, remote, refspec, force, &resolver).map_err(|e| e.to_string())
        };
        corvus_git::init::init(&path, &effective, &push)
            .await
            .map_err(|e| e.to_string())?
    };

    // Step 2 — build the opened-repo metadata (tab_id filled) and register the tab
    // in corvus-be's own registry, so its later OOP handlers resolve the path. The
    // launcher keeps no repo registry of its own any more — its in-process
    // consumers (studio, plugin host, open-in-browser) resolve the tab by asking
    // corvus-be (`__repo_tab_path` / `__repo_open_tabs`), so there is no
    // shell-side open to mirror here.
    let mut info = RepoInfo::for_path(&path).map_err(|e| e.to_string())?;
    info.tab_id = tab_id.clone();
    state.register_repo(tab_id.clone(), info.path.clone());

    // Step 3 — fire on_repo_init through the backend hook broker (same broker the
    // OOP RPC handlers fire onto). Payload shape is identical to the shell's.
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

/// Create the remote repo on `opts.provider` via the git-provider registry,
/// returning the HTTPS clone URL. The provider is resolved over the reverse
/// channel ([`crate::provider::for_host`]); the GitLab namespace-id lookup (a
/// keyring-backed REST call) is marshalled to the shell via `__gitlab_namespace_id`.
async fn create_remote_via_provider(
    state: &CorvusState,
    path: &str,
    opts: &InitRepoOptions,
) -> Result<String, String> {
    let provider = crate::provider::for_host(&opts.provider)?;

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
            if opts.org.trim().is_empty() {
                None
            } else {
                Some(opts.org.trim().to_string())
            },
            None,
        ),
        "gitlab" => {
            let ns_id = if opts.org.trim().is_empty() {
                None
            } else {
                resolve_gitlab_namespace_id(state, opts.org.trim())?
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

    let info = provider
        .create_repo(req)
        .await
        .map_err(crate::provider::pe)?;
    Ok(info.clone_url_https)
}

/// Resolve a GitLab namespace path (e.g. "myorg" or "myorg/team") to its numeric
/// `namespace_id` over the reverse channel. The keyring read + the
/// `GET /api/v4/namespaces?search=` REST call stay shell-side (`__gitlab_namespace_id`);
/// corvus-be only sees the resolved id (or `None` when the lookup fails / has no
/// match — the same best-effort shape the shell's `resolve_gitlab_namespace_id`
/// had: a missing namespace falls through to the user's default namespace).
fn resolve_gitlab_namespace_id(state: &CorvusState, path: &str) -> Result<Option<u64>, String> {
    let v = state.host_call("__gitlab_namespace_id", json!({ "path": path }))?;
    Ok(v.as_u64())
}
