//! Canonical entry point for `corvus-git`'s public API.
//!
//! Workspace convention: call sites reach this crate's surface through
//! `corvus_git::prelude::...`. The submodules stay `pub` for rustdoc navigation.

pub use crate::cli::GitCli;
pub use crate::error::GitError;

pub use crate::bisect::{
    bisect_mark, bisect_reset, bisect_start, bisect_undo_last_mark, get_bisect_state, BisectMark,
    BisectState,
};
pub use crate::bisect_sessions::{
    delete_session, list_sessions, rename_session, resume_session, save_and_pause, save_result,
    BisectSession, BisectSessionStatus,
};
pub use crate::stash::{
    StashApplyResult, StashBlockingContent, StashEntry, StashRef,
};
