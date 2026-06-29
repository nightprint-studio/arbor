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
    http_auth_args_for_credentials, merge_branch, prepare_mr_conflict_resolution,
    remove_conflict_file, resolve_conflict, resolve_stash_conflict, ConflictContent,
    ConflictPresence, MergeOutcome, MergeStrategy, MrPrepEvent, MrPrepOutcome, MrPrepPhase,
};
pub use crate::search::{search_commits, AuthorInfo, SearchQuery, SearchResult};
pub use crate::rebase::{
    get_rebase_todo, rebase_abort, rebase_continue, rebase_skip, start_interactive_rebase,
    RebaseAction, RebaseState, RebaseTodoEntry,
};
pub use crate::worktree::{
    add_worktree, detect_project_type, list_worktrees, remove_worktree, ProjectType, WorktreeInfo,
};
pub use crate::init::{
    get_git_identity, gitignore_content, init, is_git_repo, license_content, InitOutcome,
    InitRepoOptions, PushFn,
};
pub use crate::notes::{
    check_remote_status, delete_note, list_notes, set_note, CommitNote, NoteRemoteStatus,
};
pub use crate::gitflow::{
    feature_finish, feature_finish_or_pr, feature_start, get_gitflow_status, gitflow_init,
    gitflow_init_create_main, hotfix_finish, hotfix_finish_or_pr, hotfix_start, release_finish,
    release_finish_or_pr, release_start, FlowFinishResult, FlowStartResult, GitFlowBranchType,
    GitFlowConfig, GitFlowFinishConfig, GitFlowPrefixes, GitFlowStatus,
};
pub use crate::stats::{
    compute_stats, export_to_file, generate_html, AuthorLineStat, ContributorStat, FileChangeStat,
    RepoStats, StatsExclude, LOGO_SVG,
};
pub use crate::diff::{
    build_workdir_diff, get_branch_diff, get_commit_diff, get_commit_diff_meta,
    get_commit_file_diff, get_commits_range_diff_meta, get_commits_range_file_diff,
    get_file_at_commit, get_file_blame, get_workdir_diff, parse_diff, parse_diff_meta,
    parse_diff_one, run_incremental_blame, BlameLine, BlameProgress, DiffFile, DiffHunk, DiffLine,
    DiffStats, DiffStatus, EncodingOverrides, LineKind,
};
pub use crate::tickets::{
    add_toml_link, check_notes_push_refspec, default_true, links_toml_path, parse_text,
    read_all_toml_links, read_git_notes, remove_toml_link, write_git_notes, LinkSource,
    StorageBackend, TicketLink, TicketLinkCache, TicketLinkConfig, NOTES_REF,
};
// NOTE: graph::AuthorInfo is intentionally NOT re-exported here — the prelude
// already binds `AuthorInfo` to `search::AuthorInfo` (structurally identical,
// distinct type). The shell graph wrapper re-exports graph::AuthorInfo by its
// explicit module path instead.
pub use crate::graph::{
    get_commit_detail, get_files_last_commit, get_repo_file_tree, get_repo_files, load_graph,
    load_graph_for_file, CommitDetail, CommitNode, EdgeType, GraphData, GraphEdge, RefLabel,
    RefType, RepoFileEntry,
};
pub use crate::graph_svg::{generate_svg_to_file, ThemeColors};
pub use crate::status::{get_status, get_status_with, FileStatus, RepoStatus, StatusEntry};
// NOTE: `list_remote_branches` is intentionally NOT re-exported flat here — two
// distinct functions own the name (`branch::list_remote_branches(&Repository)
// -> Vec<BranchInfo>` lists remote-tracking branches of an open repo;
// `repo::list_remote_branches(&GitCli, url, auth) -> Vec<String>` lists heads of
// a remote URL via the CLI). Both shell wrappers reach theirs by module path
// (`corvus_git::branch::…` / `corvus_git::repo::…`), so neither needs the flat
// prelude. Same precedent as `graph::AuthorInfo` above.
pub use crate::branch::{
    create_branch, checkout_branch, checkout_commit_detached, checkout_remote_as_local,
    delete_branch, delete_branches, delete_remote_branches, get_nearest_tag, list_local_branches,
    list_merged_branches, list_merged_remote_branches, list_tags, rename_branch,
    rename_remote_branch, BranchInfo, RemoteRenameResult, TagInfo,
};
pub use crate::repo::{clone_repo, CloneOptions, GitRepo, RepoInfo, RepoManager};
pub use crate::reflog::{get_reflog, ReflogEntry};
pub use crate::remote::{
    fetch, list_remotes, pull, push, CredentialResolver, FetchResult, RemoteInfo,
};
pub use crate::submodule::{
    list_submodules, submodule_checkout, submodule_fetch, submodule_list_branches, submodule_pull,
    submodule_push, update_submodule, update_submodules, AuthArgsResolver, SubmoduleInfo,
};
