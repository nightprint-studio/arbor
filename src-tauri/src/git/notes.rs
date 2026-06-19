//! `notes` — shell wrapper over the Tauri-free `corvus-git` crate.
//!
//! The git logic moved into [`corvus_git::notes`] (so the headless `corvus-be`
//! shares it). This module keeps the original shell-facing API — same
//! signatures, `AppError` results — so the in-process consumers (the notes IPC
//! handlers in `ipc/corvus/notes.rs` and the Lua `ns_shell/notes.rs` namespace)
//! are untouched.
//!
//! Unlike `stash`/`merge`, this domain is plain `git2`: there is no git-program
//! global and no recovery snapshot to inject. The credential-coupled push of a
//! notes namespace stays shell-side in the broker handler
//! (`ipc/corvus/notes.rs::push_note_namespace`, via `crate::git::remote::push`)
//! — it is out of scope for the Tauri-free crate.

use git2::Repository;

use crate::error::Result;

// Re-export the data types so existing `crate::git::notes::{CommitNote,
// NoteRemoteStatus}` paths resolve.
pub use corvus_git::prelude::{CommitNote, NoteRemoteStatus};

pub fn list_notes(repo: &Repository, commit_oid_str: &str) -> Result<Vec<CommitNote>> {
    Ok(corvus_git::notes::list_notes(repo, commit_oid_str)?)
}

pub fn check_remote_status(
    repo: &Repository,
    commit_oid_str: &str,
    namespace: &str,
) -> Result<NoteRemoteStatus> {
    Ok(corvus_git::notes::check_remote_status(repo, commit_oid_str, namespace)?)
}

pub fn set_note(
    repo: &Repository,
    commit_oid_str: &str,
    namespace: &str,
    content: &str,
) -> Result<()> {
    Ok(corvus_git::notes::set_note(repo, commit_oid_str, namespace, content)?)
}

pub fn delete_note(
    repo: &Repository,
    commit_oid_str: &str,
    namespace: &str,
) -> Result<()> {
    Ok(corvus_git::notes::delete_note(repo, commit_oid_str, namespace)?)
}
