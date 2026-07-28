//! Canonical entry point for `picus-rewrite`'s public API.
//!
//! Workspace convention: call sites reach this crate through
//! `picus_rewrite::prelude::...`.

pub use crate::apply::{commit, prepare, prepare_one, Applied, FileChange, PreparedFile};
pub use crate::error::RewriteError;
pub use crate::source::{Eol, SourceText};
pub use crate::splice::{apply_splices, Splice};
