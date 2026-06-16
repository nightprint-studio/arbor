//! Canonical entry point for `arbor-fs`'s public API.
//!
//! Workspace convention: call sites reach this crate's public surface through
//! `arbor_fs::prelude::...` (or a single `use arbor_fs::prelude::*;`), not the
//! per-feature submodules. The submodules stay `pub` for rustdoc navigation.

pub use crate::copy::{CancelToken, NoopSink, ProgressSink};
pub use crate::entry::{DirSize, DriveUsage, FsEntry, FsRoot, OverviewStats, TrashEntry};
pub use crate::error::{FsError, Result};
pub use crate::mutate::RenamePair;

// Operations are also reachable as `arbor_fs::prelude::<module>::fn` so call
// sites read as `arbor_fs::prelude::read::read_dir(...)`, keeping the verb
// grouped with its domain.
pub use crate::{copy, mutate, pathexpand, read, roots, size, trash, zip};
