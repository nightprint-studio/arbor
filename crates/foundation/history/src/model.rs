//! What a history is made of.
//!
//! Everything here is `serde`-serialisable and crosses a process boundary as JSON, so
//! the field names ARE a contract. New fields must be `#[serde(default)]`: a log written
//! by an older build has to keep reading, and a history that a version bump silently
//! empties is the one failure this whole crate exists to prevent.

use serde::{Deserialize, Serialize};

/// Why a revision was recorded.
///
/// The kind is not decoration — it is what tells you whether to trust the revision
/// without looking. "Saved" is you; "Refactor" is a tool that touched six files at once;
/// "External" is something outside the editor, which is the one you almost always want
/// to read before accepting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevisionKind {
    /// The file's first known content.
    Created,
    /// An ordinary save (explicit or autosave).
    Saved,
    /// The state that arrived from **outside** the editor: a checkout, a generator, another
    /// tool. Recorded because nothing else records it — by the time the editor has adopted
    /// the new content, what the buffer used to hold is gone from everywhere. (What it used
    /// to hold is the revision *before* this one, which is why a file gets a `Created`
    /// baseline the first time history hears about it.)
    External,
    /// The state a **tool** produced: a refactor, a generate, a format. What makes
    /// "undo the refactor" possible is not this revision but the one before it, which is
    /// why every file gets a `Created` baseline the first time history hears about it —
    /// a refactor that lands on a file nobody ever saved would otherwise have nothing
    /// behind it.
    Refactored,
    /// The file moved. `from` carries where it was.
    Renamed,
    /// The file is gone. Carries no content; the revision before it is the last one.
    Deleted,
}

impl RevisionKind {
    /// Whether a revision of this kind carries bytes.
    pub fn has_content(self) -> bool {
        !matches!(self, RevisionKind::Deleted)
    }
}

/// One recorded state of one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revision {
    /// Unique within the file's log. Sortable: the timestamp comes first.
    pub id: String,
    /// Unix milliseconds.
    pub at: i64,
    pub kind: RevisionKind,
    /// Content hash, absent for a deletion. Several revisions naming the same blob is
    /// the normal case and costs one copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
    /// Size of the content in bytes (0 for a deletion). Held here so a listing does not
    /// have to stat every blob.
    #[serde(default)]
    pub size: u64,
    /// A name the user pinned on this moment. A labelled revision never expires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// What the operation was, when a tool did it: `"Rename frame_at → frame_at_ms"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The change set this revision belongs to. Shared by every file one operation
    /// touched, which is what lets six files read as one row in the timeline.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change: Option<String>,
    /// For [`RevisionKind::Renamed`], the project-relative path the file came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
}

/// One file's whole history, newest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileHistory {
    /// Project-relative, forward slashes.
    pub path: String,
    /// `true` when the file is currently gone (the newest revision is a deletion).
    pub deleted: bool,
    pub revisions: Vec<Revision>,
}

/// A file the history knows about and the project no longer has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeletedEntry {
    /// Project-relative, forward slashes.
    pub path: String,
    /// Final path segment — what the row is called.
    pub name: String,
    /// When it went.
    pub at: i64,
    /// How it went: a plain `Deleted`, or a `Renamed` that moved it away.
    pub kind: RevisionKind,
    /// The operation's description, when a tool did it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// The last content it had, if any — what a restore puts back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
    #[serde(default)]
    pub size: u64,
    /// How much history it still has, so a row can say whether there is more to read.
    pub revisions: usize,
}

/// One entry of a directory, as the history knows it.
///
/// Only what history has an opinion about: the caller already has the live directory
/// listing and merges the two. A store that also walked the filesystem would be
/// answering a question it was not asked, and would disagree with the caller's own tree
/// the moment either of them is stale.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderEntry {
    /// Project-relative, forward slashes.
    pub path: String,
    pub name: String,
    /// `true` for an entry that stands for a sub-directory rather than a file.
    pub is_dir: bool,
    /// Whether the entry was gone at the moment being looked at.
    pub deleted: bool,
    /// The newest change at or before that moment.
    pub at: i64,
    pub revisions: usize,
}

/// One operation, as the timeline shows it: a save is a change set of one file, a
/// refactor is a change set of six.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeGroup {
    pub id: String,
    pub at: i64,
    pub kind: RevisionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The files it touched, with the revision each of them got.
    pub files: Vec<ChangeFile>,
}

/// One file inside a [`ChangeGroup`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeFile {
    pub path: String,
    pub revision: String,
    pub kind: RevisionKind,
}

/// What the store is costing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    pub files: usize,
    pub revisions: usize,
    /// Bytes on disk, blobs only — the logs are noise next to them.
    pub bytes: u64,
}

/// What was dropped by a [`purge`](crate::store::HistoryStore::purge).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurgeReport {
    pub revisions_dropped: usize,
    pub blobs_dropped: usize,
    pub bytes_freed: u64,
}

/// How much history to keep, and of what.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryConfig {
    pub enabled: bool,
    /// Age limit in days. A labelled revision, and each file's newest revision, are kept
    /// regardless — the point of a label is that it outlives the window, and a file whose
    /// only revision aged out would silently stop having a history at all.
    pub keep_days: u32,
    /// Ceiling on the blobs of one project. Over it, the oldest go first.
    pub max_bytes: u64,
    /// Files bigger than this are not recorded. A 40 MB binary fills the whole budget
    /// with one revision of one file that a diff cannot show anyway.
    pub max_file_bytes: u64,
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            keep_days: 7,
            max_bytes: 256 * 1024 * 1024,
            max_file_bytes: 4 * 1024 * 1024,
        }
    }
}
