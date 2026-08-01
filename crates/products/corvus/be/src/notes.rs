//! `notes` domain — git-notes read/write + namespace push, served
//! **out-of-process** by corvus-be.
//!
//! Same handler set (function names → method names) as the shell's in-process
//! copy (`crate::ipc::corvus::notes`), but the context is [`CorvusState`]: the
//! repo is opened by the shell-pushed path. The git work is the shared
//! [`corvus_git::notes`] crate (plain `git2`, no git-program / recovery), so
//! results + error strings are identical (`GitError` `Display` mirrors
//! `AppError`'s).
//!
//! **Hooks fire here** (plugin-relocation Wave 0): `save_commit_note` →
//! `corvus:note_saved`, `delete_commit_note` → `corvus:note_deleted`, fired inline after
//! the write, same payload as in-process.
//!
//! `push_note_namespace` pushes `refs/notes/<ns>` to origin — its git smart-HTTP
//! credentials cross the reverse channel via the shared
//! [`crate::remote::credential_resolver`] (`__git_credentials`). It runs on the
//! dispatch worker thread; the credential callback blocks on the shell's reply,
//! delivered by the serve loop's reader thread (the reverse-channel reentrancy).

use corvus_core::prelude::{hooks, CorvusState};
use corvus_git::prelude::{CommitNote, NoteRemoteStatus};
use serde_json::json;

use crate::remote::credential_resolver;
use crate::repo::open;

#[arbor_rpc::handler]
fn list_commit_notes(
    state: &CorvusState,
    tab_id: String,
    commit_oid: String,
) -> Result<Vec<CommitNote>, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::notes::list_notes(&repo, &commit_oid).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn check_note_remote_status(
    state: &CorvusState,
    tab_id: String,
    commit_oid: String,
    namespace: String,
) -> Result<NoteRemoteStatus, String> {
    let repo = open(state, &tab_id)?;
    corvus_git::notes::check_remote_status(&repo, &commit_oid, &namespace).map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn save_commit_note(
    state: &CorvusState,
    tab_id: String,
    commit_oid: String,
    namespace: String,
    content: String,
) -> Result<(), String> {
    {
        let repo = open(state, &tab_id)?;
        corvus_git::notes::set_note(&repo, &commit_oid, &namespace, &content)
            .map_err(|e| e.to_string())?;
    }
    state.fire_hook(
        hooks::NOTE_SAVED,
        json!({ "tab_id": &tab_id, "commit_oid": &commit_oid, "namespace": &namespace }),
    );
    Ok(())
}

/// Push `refs/notes/<namespace>` to origin so others can fetch it.
#[arbor_rpc::handler]
fn push_note_namespace(state: &CorvusState, tab_id: String, namespace: String) -> Result<(), String> {
    let repo = open(state, &tab_id)?;
    let host = state
        .host_caller()
        .ok_or_else(|| "push_note_namespace: no reverse channel".to_string())?;
    let refspec = format!("refs/notes/{ns}:refs/notes/{ns}", ns = namespace);
    corvus_git::remote::push(&repo, "origin", &refspec, false, &credential_resolver(host))
        .map_err(|e| e.to_string())
}

#[arbor_rpc::handler]
fn delete_commit_note(
    state: &CorvusState,
    tab_id: String,
    commit_oid: String,
    namespace: String,
) -> Result<(), String> {
    {
        let repo = open(state, &tab_id)?;
        corvus_git::notes::delete_note(&repo, &commit_oid, &namespace)
            .map_err(|e| e.to_string())?;
    }
    state.fire_hook(
        hooks::NOTE_DELETED,
        json!({ "tab_id": &tab_id, "commit_oid": &commit_oid, "namespace": &namespace }),
    );
    Ok(())
}
