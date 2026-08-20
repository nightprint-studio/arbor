//! Canonical entry point for `arbor-history`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `arbor_history::prelude::...`. The submodules stay `pub` for rustdoc navigation, but
//! the prelude is the canonical call-site path.

pub use crate::diff::{compare as compare_text, DiffHunk, DiffLine, DiffLineKind, TextDelta};
pub use crate::error::{HistoryError, HistoryResult};
pub use crate::model::{
    ChangeFile, ChangeGroup, DeletedEntry, FileHistory, FolderEntry, HistoryConfig, PurgeReport,
    Revision, RevisionKind, Usage,
};
pub use crate::store::{HistoryStore, RecordCtx};
