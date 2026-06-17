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
pub use crate::recovery::{
    RecoveryEntry, RecoveryKind, RestorePreview, SkippedFile, SnapshotPolicy,
    DEFAULT_DENY_EXTENSIONS, DEFAULT_MAX_FILE_SIZE, DEFAULT_RETENTION_DAYS,
};
pub use crate::reset::{create_tag, delete_tag, run_reset, ResetMode};
pub use crate::merge::{
    abort_merge, complete_merge, get_conflict_content, get_conflict_presence, get_merge_message,
    merge_branch, remove_conflict_file, resolve_conflict, resolve_stash_conflict, ConflictContent,
    ConflictPresence, MergeOutcome, MergeStrategy,
};
pub use crate::search::{search_commits, AuthorInfo, SearchQuery, SearchResult};
pub use crate::rebase::{
    get_rebase_todo, rebase_abort, rebase_continue, rebase_skip, start_interactive_rebase,
    RebaseAction, RebaseState, RebaseTodoEntry,
};
pub use crate::worktree::{
    add_worktree, detect_project_type, list_worktrees, remove_worktree, ProjectType, WorktreeInfo,
};
