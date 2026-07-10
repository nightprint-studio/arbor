//! `repo_ops` domain — the pure + network repo handlers safe to serve
//! out-of-process by corvus-be.
//!
//! Ported byte-faithfully from the shell's in-process `ipc::corvus::repo`:
//!  * the read-only identity / metadata probes (`get_git_identity`,
//!    `get_repo_info`), and
//!  * the path / network probes that never touch the shell's `RepoManager`
//!    (`check_is_git_repo`, `clone_repo`, `list_remote_branches_for_url`).
//!
//! The clone / ls-remote pair resolves HTTPS credentials over the **reverse
//! channel** (`__git_credentials`; the keyring stays shell-side), rebuilding the
//! `-c http.extraHeader` argv via [`http_auth_args_for_credentials`] — the same
//! pattern as the `submodule` / `remote` domains. `clone_repo` does not open a
//! tab: [`RepoInfo::for_path`] leaves `tab_id` empty and the frontend opens the
//! tab afterwards via `open_repo`.
//!
//! The repo lifecycle (`open_repo` / `close_repo` / `init_repo`) now lives in
//! [`crate::repo_lifecycle`]; what stays shell-side (file dialogs / the streaming
//! job registry) is the background `spawn_clone_job`.
//!
//! No hooks fire from any handler here.

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{http_auth_args_for_credentials, CloneOptions, RepoInfo};

use crate::remote::credential_resolver;
use crate::repo::{git, origin_url, repo_path};

/// Read user.name / user.email from the global git config.
/// Returns ("", "") when the config is unavailable.
#[arbor_rpc::handler]
fn get_git_identity(_state: &CorvusState) -> Result<(String, String), String> {
    Ok(corvus_git::init::get_git_identity())
}

/// Write user.name / user.email to the global git config (`~/.gitconfig`).
/// Called when a commit failed for lack of a configured identity, so the retry
/// can build the author signature.
#[arbor_rpc::handler]
fn set_git_identity(_state: &CorvusState, name: String, email: String) -> Result<(), String> {
    corvus_git::init::set_git_identity(&name, &email).map_err(|e| e.to_string())
}

/// Read metadata (path, name, current branch, bare/empty flags) from the repo
/// opened for `tab_id`.
///
/// In-process this read the live `RepoManager` entry; here it rebuilds the same
/// `RepoInfo` from the shell-pushed path via [`RepoInfo::for_path`] (identical
/// path/name/current_branch/is_bare/is_empty derivation) and overrides `tab_id`,
/// which `for_path` leaves empty.
#[arbor_rpc::handler]
fn get_repo_info(state: &CorvusState, tab_id: String) -> Result<RepoInfo, String> {
    let path = repo_path(state, &tab_id)?;
    let mut info = RepoInfo::for_path(&path).map_err(|e| e.to_string())?;
    info.tab_id = tab_id;
    Ok(info)
}

/// The `origin` remote URL of the repo open for `tab_id` (libgit2), or `None` when
/// it has none. Used by the launcher's `open_in_browser` (the shell builds the
/// forge URL + opens the system browser, but runs no git of its own).
#[arbor_rpc::handler]
fn repo_origin_url(state: &CorvusState, tab_id: String) -> Result<Option<String>, String> {
    Ok(origin_url(&repo_path(state, &tab_id)?))
}

/// Returns true when `path` is inside a git repository (`Repository::discover`).
#[arbor_rpc::handler]
fn check_is_git_repo(_state: &CorvusState, path: String) -> Result<bool, String> {
    Ok(corvus_git::init::is_git_repo(&path))
}

/// List branch names available on a remote URL (`git ls-remote --heads`), with
/// HTTPS auth resolved over the reverse channel for private remotes.
#[arbor_rpc::handler]
fn list_remote_branches_for_url(state: &CorvusState, url: String) -> Result<Vec<String>, String> {
    let auth = auth_args(state, &url);
    corvus_git::repo::list_remote_branches(&git(state), &url, &auth).map_err(|e| e.to_string())
}

/// Clone a remote repository to disk and return the fresh repo's metadata.
///
/// Does **not** open a tab: the returned [`RepoInfo`] carries an empty `tab_id`
/// and no hook fires — the frontend opens the clone as a tab afterwards via
/// `open_repo`. HTTPS auth is resolved over the reverse channel. Runs the
/// network clone on the dispatch worker thread (the handler is sync), so the
/// serve loop never stalls on it.
#[arbor_rpc::handler]
fn clone_repo(state: &CorvusState, opts: CloneOptions) -> Result<RepoInfo, String> {
    let auth = auth_args(state, &opts.url);
    let dest = corvus_git::repo::clone_repo(&git(state), &opts, &auth).map_err(|e| e.to_string())?;
    RepoInfo::for_path(&dest).map_err(|e| e.to_string())
}

/// Resolve the HTTPS `-c http.extraHeader` auth argv for `url` over the reverse
/// channel (`__git_credentials`; the keyring stays shell-side). Empty when no
/// credential is stored or no channel is wired (public remotes need none) — the
/// same best-effort shape the `submodule` domain uses.
fn auth_args(state: &CorvusState, url: &str) -> Vec<String> {
    let Some(host) = state.host_caller() else { return Vec::new() };
    let resolve = credential_resolver(host);
    resolve(url)
        .ok()
        .flatten()
        .map(|(u, p)| http_auth_args_for_credentials(url, &u, &p))
        .unwrap_or_default()
}
