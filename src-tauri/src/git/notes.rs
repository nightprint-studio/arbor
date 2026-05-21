use std::path::Path;
use git2::{Oid, Repository};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, Result};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NoteRemoteStatus {
    /// Remote tracking ref does not exist — notes have never been pushed.
    LocalOnly,
    /// Local and remote blob match — note has been pushed and is in sync.
    InSync,
    /// Both refs exist but the blobs differ — local note is ahead of remote.
    OutOfSync,
    /// Could not determine status (remote ref absent or lookup failed).
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitNote {
    /// Namespace, derived from `refs/notes/<namespace>`.
    /// The default git namespace is `"commits"`.
    pub namespace: String,
    pub content: String,
    /// Unix timestamp (seconds) from the author signature of the notes commit.
    /// Reflects when the note was last written.
    pub created_at: i64,
    /// Populated lazily — always `Unknown` from `list_notes`; updated by
    /// `check_remote_status` when the modal opens.
    pub remote_status: NoteRemoteStatus,
}

// ---------------------------------------------------------------------------
// Queries
// ---------------------------------------------------------------------------

/// List all notes attached to `commit_oid_str` across every `refs/notes/*`
/// namespace present in the repository.
pub fn list_notes(repo: &Repository, commit_oid_str: &str) -> Result<Vec<CommitNote>> {
    let oid = parse_oid(commit_oid_str)?;
    let mut notes = Vec::new();

    let refs = match repo.references_glob("refs/notes/*") {
        Ok(r) => r,
        // No notes refs at all — not an error, just empty.
        Err(_) => return Ok(notes),
    };

    for ref_result in refs {
        let reference = match ref_result {
            Ok(r) => r,
            Err(_) => continue,
        };
        let ref_name = match reference.name() {
            Some(n) => n.to_string(),
            None => continue,
        };
        let namespace = match ref_name.strip_prefix("refs/notes/") {
            Some(ns) => ns.to_string(),
            None => continue,
        };

        if let Ok(note) = repo.find_note(Some(&ref_name), oid) {
            let content    = note.message().unwrap_or("").to_string();
            let created_at = note.author().when().seconds();
            notes.push(CommitNote {
                namespace,
                content,
                created_at,
                remote_status: NoteRemoteStatus::Unknown,
            });
        }
    }

    Ok(notes)
}

/// Check whether a given namespace+commit note is in sync with the remote
/// tracking ref `refs/remotes/origin/notes/<namespace>`.
/// This is called on-demand when the modal opens, never eagerly.
pub fn check_remote_status(
    repo: &Repository,
    commit_oid_str: &str,
    namespace: &str,
) -> Result<NoteRemoteStatus> {
    let oid = parse_oid(commit_oid_str)?;
    let local_ref  = format!("refs/notes/{}", namespace);
    let remote_ref = format!("refs/remotes/origin/notes/{}", namespace);

    let local_blob  = note_blob_oid(repo, &local_ref, oid);
    let remote_blob = note_blob_oid(repo, &remote_ref, oid);

    Ok(match (local_blob, remote_blob) {
        (None, _)                              => NoteRemoteStatus::Unknown,
        (Some(_), None)                        => NoteRemoteStatus::LocalOnly,
        (Some(l), Some(r)) if l == r           => NoteRemoteStatus::InSync,
        _                                      => NoteRemoteStatus::OutOfSync,
    })
}

// ---------------------------------------------------------------------------
// Mutations
// ---------------------------------------------------------------------------

/// Create or overwrite a note for `commit_oid_str` in `refs/notes/<namespace>`.
pub fn set_note(
    repo: &Repository,
    commit_oid_str: &str,
    namespace: &str,
    content: &str,
) -> Result<()> {
    let oid = parse_oid(commit_oid_str)?;
    let sig = repo.signature()?;
    let notes_ref = format!("refs/notes/{}", namespace);
    repo.note(&sig, &sig, Some(&notes_ref), oid, content, true)?;
    Ok(())
}

/// Delete the note for `commit_oid_str` in `refs/notes/<namespace>`.
pub fn delete_note(
    repo: &Repository,
    commit_oid_str: &str,
    namespace: &str,
) -> Result<()> {
    let oid = parse_oid(commit_oid_str)?;
    let sig = repo.signature()?;
    let notes_ref = format!("refs/notes/{}", namespace);
    repo.note_delete(oid, Some(&notes_ref), &sig, &sig)
        .map_err(|e| AppError::Other(format!("delete note: {e}")))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn parse_oid(s: &str) -> Result<Oid> {
    Oid::from_str(s).map_err(|_| AppError::CommitNotFound(s.to_string()))
}

/// Resolve the blob OID of the note for `commit_oid` stored inside the
/// notes tree pointed at by `notes_ref`.  Returns `None` if the ref or
/// the note entry does not exist.
///
/// git stores notes in two layouts:
///   • flat:    `<40-char-hex>` entries directly in the root tree
///   • fan-out: `<2>/<38>` entries (used when there are many notes)
fn note_blob_oid(repo: &Repository, notes_ref: &str, commit_oid: Oid) -> Option<Oid> {
    let reference  = repo.find_reference(notes_ref).ok()?;
    let notes_tree = reference.peel_to_commit().ok()?.tree().ok()?;

    let hex = commit_oid.to_string();

    // Try flat path first (most common).
    if let Ok(entry) = notes_tree.get_path(Path::new(&hex)) {
        return Some(entry.id());
    }

    // Try fan-out path.
    if hex.len() >= 2 {
        let fan  = &hex[..2];
        let rest = &hex[2..];
        if let Ok(entry) = notes_tree.get_path(Path::new(&format!("{fan}/{rest}"))) {
            return Some(entry.id());
        }
    }

    None
}
