//! [`SyncRemote`] — the seam itself.
//!
//! The vocabulary here is *reconcile two versions of a folder of notes*, not
//! *run git* (`docs/garrulus-design.md` §4). That is not decoration: the second
//! implementation, [`crate::folder::FolderRemote`], is what proves it — if a
//! plain mirror directory is awkward to write against this trait, the trait is
//! wrong.
//!
//! Capability flags rather than optional methods: a remote that cannot answer
//! `history` says so in its [`RemoteCapabilities`], and the UI hides the history
//! panel instead of showing a broken one.

use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::change::{ChangeBatch, RelPath};
use crate::conflict::Conflict;
use crate::error::SyncResult;
use crate::state::SyncState;

/// What kind of thing is on the other side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RemoteKind {
    /// A git repository (the real one).
    Git,
    /// A mirror directory: a USB stick, a network share, a cloud-synced folder.
    Folder,
}

/// What a remote can actually do, so the UI never offers what it cannot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteCapabilities {
    /// Can it answer [`SyncRemote::history`] / [`SyncRemote::revision`]?
    pub history: bool,
    /// Is a push all-or-nothing?
    pub atomic_batch: bool,
    /// Can it detect concurrent edits, or is it last-writer-wins?
    pub conflicts: bool,
}

/// Identity of one configured remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoteDescriptor {
    /// Stable id (the git remote name, or the mirror path).
    pub id: String,
    /// Which implementation is behind it.
    pub kind: RemoteKind,
    /// What the user sees in the sync dropdown.
    pub display: String,
    /// What it can do.
    pub capabilities: RemoteCapabilities,
}

/// One past version of a note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    /// Opaque id to feed back to [`SyncRemote::revision`].
    pub id: String,
    /// Who wrote it — for Garrulus's own commits, the device (§4.2).
    pub author: String,
    /// Unix seconds.
    pub timestamp: i64,
    /// One-line description.
    pub summary: String,
}

/// What a pull did.
///
/// `conflicts` is not an error path: a conflict is a normal outcome that the
/// user resolves, and the notes in `applied` landed regardless.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullOutcome {
    /// Notes whose content changed on disk as a result.
    pub applied: Vec<RelPath>,
    /// Notes two machines disagree about.
    pub conflicts: Vec<Conflict>,
}

impl PullOutcome {
    /// Did the pull change anything at all?
    pub fn is_empty(&self) -> bool {
        self.applied.is_empty() && self.conflicts.is_empty()
    }
}

/// One place the vault is mirrored to.
///
/// **Only [`probe`](SyncRemote::probe) may run unattended.** Everything else
/// changes bytes and therefore happens because the user pressed the button
/// (§4.2). Implementations are expected to be cheap to clone and to do their
/// blocking work off the runtime's workers.
#[async_trait]
pub trait SyncRemote: Send + Sync {
    /// Identity and capabilities. Never fails, never touches the network.
    fn descriptor(&self) -> RemoteDescriptor;

    /// Read-only: where does the vault stand against the remote?
    ///
    /// The only method the background timer is allowed to call.
    async fn probe(&self) -> SyncResult<SyncState>;

    /// Bring remote changes in, writing the notes to disk and parking whatever
    /// did not auto-merge as conflict side files.
    async fn pull(&self, vault: &Path) -> SyncResult<PullOutcome>;

    /// Send local changes out.
    async fn push(&self, vault: &Path, batch: &ChangeBatch) -> SyncResult<()>;

    /// Past versions of one note, newest first.
    ///
    /// Returns [`crate::error::SyncError::Unsupported`] when
    /// `capabilities.history` is false.
    async fn history(&self, vault: &Path, note: &RelPath) -> SyncResult<Vec<Revision>>;

    /// The text of one note at one revision.
    async fn revision(&self, vault: &Path, note: &RelPath, rev: &str) -> SyncResult<String>;
}
