//! `reset` / `tags` domain — handlers routed through the in-process broker.
//!
//! Each handler is the body the matching `#[tauri::command]` ran inline;
//! `#[corvus::handler]` self-registers it under its own function name. The pure
//! git work now lives in [`corvus_git::reset`]; this shell layer keeps the OID
//! validation, the hard-reset recovery snapshot (config-loading, stays
//! shell-side), and the `on_tag_*` hooks — so behavior (subprocess shelling,
//! recovery snapshot, hooks fired, errors) is byte-identical.
//!
//! Reset and tag-create/delete were one command module historically; they ride
//! together here for a faithful move. If this grows, splitting tags out is the
//! natural next step.

use corvus_git::prelude::ResetMode;

use crate::error::AppError;
use crate::ipc::corvus;
use crate::AppState;

/// The shell's resolved git program as a `corvus-git` invoker.
fn git() -> corvus_git::prelude::GitCli {
    corvus_git::prelude::GitCli::from_optional(crate::git_cli::snapshot().path)
}

#[corvus::handler]
fn reset_to_commit(state: &AppState, tab_id: String, oid: String, mode: ResetMode) -> Result<(), AppError> {
    // Validate the OID before spawning a subprocess.
    let git_oid = git2::Oid::from_str(&oid).map_err(|_| AppError::CommitNotFound(oid.clone()))?;

    // Extract workdir + run the hard-reset safety snapshot while we still
    // hold the repo, then release the lock before calling the CLI so libgit2
    // does not keep a stale view of HEAD/refs across the subprocess.
    let workdir = {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get_mut(&tab_id)?;
        let r = repo.inner_mut();

        // Confirm the OID resolves to a commit in this repo before shelling
        // out — gives a cleaner error than a cryptic CLI failure.
        let _ = r.find_object(git_oid, Some(git2::ObjectType::Commit))?;

        if matches!(mode, ResetMode::Hard) {
            let short = oid.get(..7).unwrap_or(&oid);
            crate::git::recovery::try_snapshot(
                r,
                crate::git::recovery::RecoveryKind::ResetHard,
                format!("reset --hard to {short}"),
            );
        }

        r.workdir()
            .ok_or_else(|| AppError::Other("bare repository has no working directory".into()))?
            .to_path_buf()
    };

    corvus_git::reset::run_reset(&git(), &workdir, &oid, mode)?;

    Ok(())
}

#[corvus::handler]
fn create_tag(
    state: &AppState,
    tab_id: String,
    name: String,
    oid: String,
    message: Option<String>,
) -> Result<(), AppError> {
    let annotated = message.is_some();
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        corvus_git::reset::create_tag(repo.inner(), &name, &oid, message.as_deref())?;
    }
    state.fire_hook(
        "on_tag_create",
        serde_json::json!({
            "tab_id":    &tab_id,
            "name":      &name,
            "oid":       &oid,
            "annotated": annotated,
        }),
    );
    Ok(())
}

#[corvus::handler]
fn delete_tag(state: &AppState, tab_id: String, name: String) -> Result<(), AppError> {
    {
        let mut mgr = state.lock_repos()?;
        let repo = mgr.get(&tab_id)?;
        corvus_git::reset::delete_tag(repo.inner(), &name)?;
    }
    state.fire_hook(
        "on_tag_delete",
        serde_json::json!({ "tab_id": &tab_id, "name": &name }),
    );
    Ok(())
}
