//! `repo_browser` domain — remote-repository browser, served **out-of-process**
//! by corvus-be.
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::repo_browser`), but the context is [`CorvusState`]
//! and the provider comes from the reverse-channel registry
//! ([`crate::provider`]) instead of the shell's `GitProviderRegistry`. The trait
//! work is the shared `corvus-git-provider-{api,github,gitlab}` crates, so the
//! results and the `ProviderError` wire strings are identical to in-process. The
//! browser resolves providers by **host string** (`"github"` / `"gitlab"`) — no
//! tab / `RepoManager` involvement — which is why it leads the REST cohort OOP.
//! No hooks fire in this domain.

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use corvus_core::prelude::CorvusState;
use corvus_git_provider_api::prelude::{
    ListReposOpts, RemoteAccount, RemoteFileContent, RemoteRepo, RemoteTreeEntry, RepoRef,
};

use crate::provider::{for_host, hosted, maybe_refresh, pe};

/// Build a repo-scoped `RepoRef` from the browser's `full_name`. GitHub splits
/// `owner/repo`; GitLab takes the full project path verbatim. Same error wire
/// strings as the in-process copy.
fn repo_ref_for(provider: &str, full_name: &str) -> Result<RepoRef, String> {
    match provider {
        "github" => {
            let (owner, name) = full_name
                .split_once('/')
                .ok_or_else(|| "Invalid repo full_name (no slash)".to_string())?;
            Ok(RepoRef::github(owner, name))
        }
        "gitlab" => Ok(RepoRef::gitlab(full_name)),
        other => Err(format!("Unknown provider: {other}")),
    }
}

/// List all connected remote accounts (GitHub + GitLab) via each provider's
/// `current_user`. Providers without a stored token are skipped; a failed lookup
/// drops that account silently (the picker just shows fewer accounts).
#[arbor_rpc::handler]
async fn rb_list_accounts(_ctx: &CorvusState) -> Result<Vec<RemoteAccount>, String> {
    let mut accounts = Vec::new();
    for (provider, p) in hosted() {
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
/// `provider`: "github" | "gitlab".
#[arbor_rpc::handler]
async fn rb_list_repos(_ctx: &CorvusState, provider: String) -> Result<Vec<RemoteRepo>, String> {
    let p = for_host(&provider)?;
    maybe_refresh(&provider);
    p.list_user_repos(ListReposOpts::default()).await.map_err(pe)
}

/// List files and directories at `path` within a remote repository.
#[arbor_rpc::handler]
async fn rb_browse_tree(
    _ctx:      &CorvusState,
    provider:  String,
    full_name: String,
    path:      String,
    branch:    String,
) -> Result<Vec<RemoteTreeEntry>, String> {
    let p = for_host(&provider)?;
    let repo = repo_ref_for(&provider, &full_name)?;
    maybe_refresh(&provider);
    p.browse_tree(&repo, &path, &branch).await.map_err(pe)
}

/// Fetch the content of a single file for inline preview.
#[arbor_rpc::handler]
async fn rb_get_file_content(
    _ctx:      &CorvusState,
    provider:  String,
    full_name: String,
    path:      String,
    branch:    String,
) -> Result<RemoteFileContent, String> {
    let p = for_host(&provider)?;
    let repo = repo_ref_for(&provider, &full_name)?;
    maybe_refresh(&provider);
    p.get_file_content(&repo, &path, &branch).await.map_err(pe)
}

/// Download a remote file to a local path. The FE passes an **absolute**
/// `dest_path` (chosen via the save dialog) — corvus-be runs with a different
/// working directory than the shell, so a relative path would resolve elsewhere.
#[arbor_rpc::handler]
async fn rb_download_file(
    _ctx:      &CorvusState,
    provider:  String,
    full_name: String,
    path:      String,
    branch:    String,
    dest_path: String,
) -> Result<(), String> {
    let p = for_host(&provider)?;
    let repo = repo_ref_for(&provider, &full_name)?;
    maybe_refresh(&provider);
    let file = p.get_file_content(&repo, &path, &branch).await.map_err(pe)?;

    // For images we have base64 data; for text, UTF-8 content. Either way we
    // write the original bytes back to disk.
    if let Some(data_url) = file.image_data {
        if let Some(b64) = data_url.split(',').nth(1) {
            let bytes = BASE64
                .decode(b64)
                .map_err(|e| format!("Base64 decode: {e}"))?;
            std::fs::write(&dest_path, bytes).map_err(|e| format!("Write file: {e}"))?;
            return Ok(());
        }
    }
    if !file.content.is_empty() {
        std::fs::write(&dest_path, file.content.as_bytes())
            .map_err(|e| format!("Write file: {e}"))?;
    }
    Ok(())
}
