//! `reset` / `tags` domain — served **out-of-process** by corvus-be.
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::reset`), but the context is [`CorvusState`]: the
//! repo is opened by the shell-pushed path and the git program comes from
//! [`CorvusState::git_program`]. The pure git work is the shared [`corvus_git`]
//! crate, so behavior + error strings are identical to in-process.
//!
//! **Hooks fire here** (plugin-relocation Wave 0): `on_tag_create` /
//! `on_tag_delete` go to the co-located host after the repo handle is dropped —
//! same payload as in-process, no longer dropped on the OOP path.
//!
//! The hard-reset safety snapshot uses the shell-pushed recovery policy
//! (`crate::repo::snapshot_policy`), falling back to the built-in default when
//! none was pushed — same configured limits as in-process (W0b).

use corvus_core::prelude::CorvusState;
use corvus_git::prelude::{RecoveryKind, ResetMode};
use serde_json::json;

use crate::repo::{git, open, snapshot_policy};

#[arbor_rpc::handler]
fn reset_to_commit(state: &CorvusState, tab_id: String, oid: String, mode: ResetMode) -> Result<(), String> {
    // Validate the OID before spawning a subprocess — same typed-error wire
    // string as in-process (`AppError::CommitNotFound` Displays "Commit not
    // found: …"); over the seam the shell wraps it as `AppError::Other` but the
    // string the FE reads is unchanged.
    let git_oid = git2::Oid::from_str(&oid).map_err(|_| format!("Commit not found: {oid}"))?;
    let g = git(state);

    // Open + (for hard) snapshot while we hold the repo, then drop it before the
    // CLI reset so libgit2 doesn't keep a stale HEAD/refs view across the
    // subprocess — same shape as the in-process handler.
    let workdir = {
        let repo = open(state, &tab_id)?;

        // Confirm the OID resolves to a commit in this repo before shelling out.
        repo.find_object(git_oid, Some(git2::ObjectType::Commit))
            .map_err(|e| format!("Git error: {e}"))?;

        if matches!(mode, ResetMode::Hard) {
            let short = oid.get(..7).unwrap_or(&oid);
            let _ = corvus_git::recovery::snapshot_with_policy(
                &g,
                &repo,
                RecoveryKind::ResetHard,
                &format!("reset --hard to {short}"),
                &snapshot_policy(state),
            );
        }

        repo.workdir()
            .ok_or_else(|| "bare repository has no working directory".to_string())?
            .to_path_buf()
    };

    corvus_git::reset::run_reset(&g, &workdir, &oid, mode).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn create_tag(
    state: &CorvusState,
    tab_id: String,
    name: String,
    oid: String,
    message: Option<String>,
) -> Result<(), String> {
    let annotated = message.is_some();
    {
        let repo = open(state, &tab_id)?;
        corvus_git::reset::create_tag(&repo, &name, &oid, message.as_deref())
            .map_err(|e| e.to_string())?;
    }
    // Repo handle dropped; fire inline so a Lua git op in the hook can't deadlock.
    // Payload mirrors the shell's in-process `create_tag`.
    state.fire_hook(
        "on_tag_create",
        json!({ "tab_id": &tab_id, "name": &name, "oid": &oid, "annotated": annotated }),
    );
    Ok(())
}

#[arbor_rpc::handler]
fn delete_tag(state: &CorvusState, tab_id: String, name: String) -> Result<(), String> {
    {
        let repo = open(state, &tab_id)?;
        corvus_git::reset::delete_tag(&repo, &name).map_err(|e| e.to_string())?;
    }
    state.fire_hook("on_tag_delete", json!({ "tab_id": &tab_id, "name": &name }));
    Ok(())
}
