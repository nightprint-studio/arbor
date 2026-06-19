//! `repo_ops` domain — the two **pure read-only** repo handlers that are safe to
//! serve out-of-process by corvus-be.
//!
//! Ported byte-faithfully from the shell's in-process `ipc::corvus::repo`
//! (`get_git_identity`, `get_repo_info`). The rest of that file —
//! `open_repo`/`close_repo`/`clone_repo`/`init_repo`/`check_is_git_repo`/
//! `create_remote_via_provider`/`list_remote_branches_for_url` — stays shell-side
//! because it touches the `AppState` repo manager, file dialogs, provider OAuth,
//! and credentials, none of which live in this backend. These two read-only
//! probes touch none of that, so they migrate cleanly.
//!
//! No hooks fire from either handler.

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::RepoInfo;

use crate::repo::repo_path;

/// Read user.name / user.email from the global git config.
/// Returns ("", "") when the config is unavailable.
#[arbor_rpc::handler]
fn get_git_identity(_state: &CorvusState) -> Result<(String, String), String> {
    Ok(corvus_git::init::get_git_identity())
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
