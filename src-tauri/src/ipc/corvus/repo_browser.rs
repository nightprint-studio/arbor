//! `repo_browser` domain — remote-repository browser handlers routed through the
//! in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` used to run inline;
//! `#[corvus::handler]` self-registers it under its own function name. They are
//! `async` (provider HTTP) so the generic `rpc` command awaits them on the
//! runtime. The git-provider trait work stays in `crate::git_provider`; these are
//! the thin `AppState` shell that resolves the provider + maps `ProviderError`.
//! No hooks fire in this domain.

use std::sync::Arc;

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};

use crate::error::{AppError, Result};
use crate::git_provider::repo_impl::{
    ListReposOpts, RemoteAccount, RemoteFileContent, RemoteRepo, RemoteTreeEntry, RepoRef,
};
use crate::git_provider::GitProvider;
use crate::ipc::corvus;
use crate::AppState;

fn pe(e: crate::git_provider::types::error::ProviderError) -> AppError {
    AppError::Other(e.to_string())
}

/// Resolve the registry provider for a browser `provider` string. The remote
/// browser targets the hosted instances (`github.com` / `gitlab.com`);
/// self-hosted browsing is not exposed here.
fn provider_for(state: &AppState, provider: &str) -> Result<Arc<dyn GitProvider>> {
    let host = match provider {
        "github" => "github.com",
        "gitlab" => "gitlab.com",
        other    => return Err(AppError::Other(format!("Unknown provider: {other}"))),
    };
    let reg = state.lock_git_providers()?;
    reg.for_host(host)
        .ok_or_else(|| AppError::Other(format!("No provider registered for {host}")))
}

/// Build a repo-scoped `RepoRef` from the browser's `full_name`.
/// GitHub splits `owner/repo`; GitLab takes the full project path verbatim.
fn repo_ref_for(provider: &str, full_name: &str) -> Result<RepoRef> {
    match provider {
        "github" => {
            let (owner, name) = full_name
                .split_once('/')
                .ok_or_else(|| AppError::Other("Invalid repo full_name (no slash)".into()))?;
            Ok(RepoRef::github(owner, name))
        }
        "gitlab" => Ok(RepoRef::gitlab(full_name)),
        other => Err(AppError::Other(format!("Unknown provider: {other}"))),
    }
}

/// List all connected remote accounts (GitHub + GitLab) via each provider's
/// `current_user`. Providers without a stored token are skipped; a failed
/// lookup drops that account silently (the picker just shows fewer accounts).
#[corvus::handler]
async fn rb_list_accounts(state: &AppState) -> Result<Vec<RemoteAccount>> {
    // `current_user` is async → pull the Arcs out from under the lock first.
    let providers: Vec<(String, Arc<dyn GitProvider>)> = match state.lock_git_providers() {
        Ok(reg) => [("github", "github.com"), ("gitlab", "gitlab.com")]
            .iter()
            .filter_map(|(p, h)| reg.for_host(h).map(|prov| (p.to_string(), prov)))
            .collect(),
        Err(_) => return Ok(Vec::new()),
    };

    let mut accounts = Vec::new();
    for (provider, p) in providers {
        if !p.has_token() {
            continue;
        }
        if let Ok(u) = p.current_user().await {
            accounts.push(RemoteAccount {
                provider,
                username:     u.login,
                display_name: u.name,
                avatar_url:   u.avatar_url,
            });
        }
    }
    Ok(accounts)
}

/// Fetch all repositories accessible to the given provider account.
/// `provider`: "github" | "gitlab"
#[corvus::handler]
async fn rb_list_repos(state: &AppState, provider: String) -> Result<Vec<RemoteRepo>> {
    let p = provider_for(state, &provider)?;
    crate::auth::maybe_refresh_for_provider(&provider).await;
    p.list_user_repos(ListReposOpts::default()).await.map_err(pe)
}

/// List files and directories at `path` within a remote repository.
/// `path`: relative path inside the repo ("" for root).
/// `branch`: branch/tag/sha to browse.
#[corvus::handler]
async fn rb_browse_tree(
    state:      &AppState,
    provider:   String,
    full_name:  String,
    path:       String,
    branch:     String,
) -> Result<Vec<RemoteTreeEntry>> {
    let p = provider_for(state, &provider)?;
    let repo = repo_ref_for(&provider, &full_name)?;
    crate::auth::maybe_refresh_for_provider(&provider).await;
    p.browse_tree(&repo, &path, &branch).await.map_err(pe)
}

/// Fetch the content of a single file for inline preview.
#[corvus::handler]
async fn rb_get_file_content(
    state:     &AppState,
    provider:  String,
    full_name: String,
    path:      String,
    branch:    String,
) -> Result<RemoteFileContent> {
    let p = provider_for(state, &provider)?;
    let repo = repo_ref_for(&provider, &full_name)?;
    crate::auth::maybe_refresh_for_provider(&provider).await;
    p.get_file_content(&repo, &path, &branch).await.map_err(pe)
}

/// Download a remote file to a local path.
#[corvus::handler]
async fn rb_download_file(
    state:     &AppState,
    provider:  String,
    full_name: String,
    path:      String,
    branch:    String,
    dest_path: String,
) -> Result<()> {
    let p = provider_for(state, &provider)?;
    let repo = repo_ref_for(&provider, &full_name)?;
    crate::auth::maybe_refresh_for_provider(&provider).await;
    let file = p.get_file_content(&repo, &path, &branch).await.map_err(pe)?;

    // For images we have base64 data; for text, UTF-8 content. Either way we
    // write the original bytes back to disk.
    if let Some(data_url) = file.image_data {
        if let Some(b64) = data_url.split(',').nth(1) {
            let bytes = BASE64
                .decode(b64)
                .map_err(|e| AppError::Other(format!("Base64 decode: {e}")))?;
            std::fs::write(&dest_path, bytes)
                .map_err(|e| AppError::Other(format!("Write file: {e}")))?;
            return Ok(());
        }
    }
    if !file.content.is_empty() {
        std::fs::write(&dest_path, file.content.as_bytes())
            .map_err(|e| AppError::Other(format!("Write file: {e}")))?;
    }
    Ok(())
}
