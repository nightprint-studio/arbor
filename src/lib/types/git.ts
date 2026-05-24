// ============================================================
// Core git types — mirroring the Rust structs 1:1
// ============================================================

export interface AuthorInfo {
  name: string;
  email: string;
}

export interface RefLabel {
  name: string;
  ref_type: 'local_branch' | 'remote_branch' | 'tag';
  is_current: boolean;
}

export type EdgeType = 'straight' | 'fork_left' | 'fork_right' | 'merge_left' | 'merge_right' | 'squash_merge';

export interface CommitNode {
  oid: string;
  short_oid: string;
  summary: string;
  body?: string;
  author: AuthorInfo;
  committer: AuthorInfo;
  timestamp: number;
  row: number;
  lane: number;
  color_index: number;
  refs: RefLabel[];
  is_merge: boolean;
  is_head: boolean;
  active_lanes: number[];
}

export interface GraphEdge {
  from_row: number;
  from_lane: number;
  to_row: number;
  to_lane: number;
  color_index: number;
  edge_type: EdgeType;
  /** Present only on trailing edges (parent outside the loaded page).
   *  appendGraph uses it to repair to_row once the parent's page arrives. */
  to_parent_oid?: string;
}

export interface GraphData {
  nodes: CommitNode[];
  edges: GraphEdge[];
  lane_count: number;
  total_commits: number;
  offset: number;
  /** Stash anchors (if any) — rendered as dashed markers on the parent
   *  commit. Field comes back as `stashes: []` when there are no stashes. */
  stashes?: StashRef[];
}

/** Lightweight projection of a stash used by the graph renderer. */
export interface StashRef {
  index: number;
  oid: string;
  parentOid: string;
  message: string;
}

export interface CommitDetail {
  oid: string;
  short_oid: string;
  summary: string;
  body?: string;
  author: AuthorInfo;
  committer: AuthorInfo;
  timestamp: number;
  committer_timestamp: number;
  parent_oids: string[];
  refs: RefLabel[];
  is_head: boolean;
}

// ---- File tree ----

export interface RepoFileEntry {
  path: string;
  last_commit_oid?: string;
  last_commit_short_oid?: string;
  last_commit_date?: number;       // unix timestamp (seconds)
  last_commit_summary?: string;
}

// ---- Git Blame ----

export interface BlameLine {
  line_no: number;
  content: string;
  commit_oid: string;
  short_oid: string;
  author_name: string;
  author_email: string;
  timestamp: number;    // unix seconds
  summary: string;
  is_group_start: boolean;
}

// ---- Git Notes ----

export type NoteRemoteStatus = 'local_only' | 'in_sync' | 'out_of_sync' | 'unknown';

export interface CommitNote {
  namespace: string;
  content: string;
  /** Unix timestamp (seconds) of when the note was last written. */
  created_at: number;
  remote_status: NoteRemoteStatus;
}

// ---- Diff ----

export type DiffStatus = 'added' | 'modified' | 'deleted' | 'renamed' | 'copied' | 'untracked' | 'binary';
export type LineKind = 'context' | 'added' | 'removed';

export interface DiffLine {
  kind: LineKind;
  old_lineno?: number;
  new_lineno?: number;
  content: string;
}

export interface DiffHunk {
  header: string;
  old_start: number;
  old_lines: number;
  new_start: number;
  new_lines: number;
  lines: DiffLine[];
}

export interface DiffStats {
  additions: number;
  deletions: number;
}

export interface DiffFile {
  path: string;
  old_path?: string;
  status: DiffStatus;
  hunks: DiffHunk[];
  stats: DiffStats;
  is_binary: boolean;
  image_old?: string; // base64
  image_new?: string; // base64
  /**
   * Encoding label used to decode the hunk lines. "UTF-8" for modern
   * files, "windows-1252" for legacy Latin-1 sources. Absent for binary
   * deltas where no decoding happened.
   */
  encoding?: string;
}

// ---- Status ----

export type FileStatus = 'added' | 'modified' | 'deleted' | 'renamed' | 'copied' | 'untracked' | 'ignored' | 'conflicted';

export interface StatusEntry {
  path: string;
  old_path?: string;
  index_status?: FileStatus;
  workdir_status?: FileStatus;
}

export interface RepoStatus {
  current_branch?: string;
  head_oid?: string;
  is_detached: boolean;
  ahead: number;
  behind: number;
  staged: StatusEntry[];
  unstaged: StatusEntry[];
  untracked: StatusEntry[];
  conflicted: StatusEntry[];
  is_rebasing: boolean;
  is_merging: boolean;
  is_cherry_picking: boolean;
  is_reverting: boolean;
}

// ---- Repo ----

export interface RepoInfo {
  tab_id: string;
  path: string;
  name: string;
  current_branch?: string;
  is_bare: boolean;
  is_empty: boolean;
}

export interface CloneOptions {
  url: string;
  dest_path: string;
  branch?: string;
  shallow?: boolean;
  recurse_submodules?: boolean;
}

export interface InitRepoOptions {
  /** Default branch name, e.g. "main" */
  default_branch: string;
  /** Optional description stored in .git/description and README */
  description: string;
  /** Whether to create an initial commit */
  initial_commit: boolean;
  /** Initial commit message */
  commit_message: string;
  /** Author name for the initial commit */
  author_name: string;
  /** Author email for the initial commit */
  author_email: string;
  /** .gitignore template name ("rust", "node", "python", …) or "" for none */
  gitignore_template: string;
  /** SPDX license identifier ("mit", "apache-2.0", …) or "" for none */
  license: string;
  /** Create README.md */
  readme: boolean;
  /** Provider: "none" | "github" | "gitlab" */
  provider: string;
  /** Repository visibility: "public" | "private" */
  visibility: string;
  /** GitHub org / GitLab group. Empty = personal account */
  org: string;
  /** Explicit remote URL — non-empty value skips provider API creation */
  remote_url: string;
  /** After init + initial commit, push to origin and set upstream tracking */
  push_initial: boolean;
}

export interface InitRepoResult {
  info: RepoInfo;
  /** URL of origin that was configured (null when provider=none and no URL given) */
  remote_url: string | null;
  /** Whether the initial commit was pushed to origin */
  pushed: boolean;
  /** Push failure message when push was attempted and failed */
  push_error: string | null;
}

// ---- Branches ----

export interface BranchInfo {
  name: string;
  full_ref: string;
  is_head: boolean;
  upstream?: string;
  ahead: number;
  behind: number;
  head_oid: string;
  head_summary: string;
}

export interface TagInfo {
  name: string;
  target_oid: string;
  message?: string;
}

// ---- Stash ----

export interface StashEntry {
  index: number;
  message: string;
  oid: string;
}

export interface StashApplyResult {
  has_conflicts: boolean;
  conflicted_files: string[];
  /** Untracked files that blocked the apply entirely. When non-empty the apply
   *  was NOT performed — the user must confirm deletion before retrying. */
  blocking_untracked: string[];
  /** True when the apply was a true no-op (workdir already matched the stash).
   *  Lets the UI show "No changes" instead of a misleading "Stash applied". */
  no_changes?: boolean;
}

export interface StashBlockingContent {
  path: string;
  /** Current on-disk content, null if missing. */
  current_content: string | null;
  /** Content from the stash commit tree, null if binary or not in stash. */
  stash_content: string | null;
  /**
   * Encoding used to decode both blobs ("UTF-8" / "windows-1252" / …).
   * Absent only when neither side is readable.
   */
  encoding?: string;
}

export interface CheckoutResult {
  stash_conflicts: string[];
  pre_checkout_stash: StashEntry | null;
  /** Set when stash re-apply failed for a non-conflict reason. Stash is preserved at index 0. */
  stash_apply_error?: string;
  /** Populated by `checkoutRemoteAsLocalSafe` — short name of the created/reused
   *  local tracking branch. `null` for branch / commit checkouts. Lets callers
   *  update the tab badge even when stash apply produced conflicts. */
  resolved_local_name?: string | null;
  /** True when the backend stashed before the checkout. Survives the clean-apply
   *  path (where `pre_checkout_stash` is reset to null after the apply drops
   *  the entry) so the success toast can mention the round-trip. */
  did_stash?: boolean;
}

export interface PullResult {
  stash_conflicts: string[];
  pre_pull_stash: StashEntry | null;
  /** Set when the stash re-apply failed for a non-conflict reason (e.g. file lock).
   *  The stash is still at index 0 and can be recovered from the Stash panel. */
  stash_apply_error?: string;
  /** Set when the pull fetch/merge itself failed.  Stash context (if any) is
   *  still present so the user can recover their work. */
  pull_error?: string;
}

export interface CherryPickResult {
  has_conflicts: boolean;
  conflicted_files: string[];
  /** True when cherry-pick / revert produced no diff against HEAD (the
   *  commit's changes are already present on the current branch). */
  no_changes: boolean;
}

// ---- Remote ----

export interface RemoteInfo {
  name: string;
  url: string;
}

export interface FetchResult {
  remote: string;
  received_objects: number;
  indexed_objects: number;
  received_bytes: number;
}

// ---- Rebase ----

export type RebaseAction = 'pick' | 'reword' | 'edit' | 'squash' | 'fixup' | 'drop';

export interface RebaseTodoEntry {
  action: RebaseAction;
  short_oid: string;
  summary: string;
}

export interface RebaseState {
  in_progress: boolean;
  current_step: number;
  total_steps: number;
  conflicted_files: string[];
}

// ---- Search ----

export interface SearchResult {
  oid: string;
  short_oid: string;
  summary: string;
  author: AuthorInfo;
  timestamp: number;
}

export interface SearchQuery {
  text: string;
  include_author: boolean;
  limit: number;
}

// ---- Submodule ----

export interface SubmoduleInfo {
  name: string;
  /** Relative path from parent repo root. */
  path: string;
  /** Absolute filesystem path (for opening as a tab). */
  abs_path: string;
  url: string;
  /** Short 7-char HEAD commit hash; empty string if uninitialised. */
  head_hash: string;
  /** Current branch name, or null when detached HEAD. */
  branch: string | null;
  /** Commits ahead of the remote tracking branch. */
  ahead: number;
  /** Commits behind the remote tracking branch. */
  behind: number;
  /** Whether the working directory has uncommitted changes. */
  is_dirty: boolean;
  /** Whether the submodule has been initialised and cloned. */
  is_initialized: boolean;
}

// ---- Reset ----

export type ResetMode = 'soft' | 'mixed' | 'hard';

// ---- Git Flow ----

export interface GitFlowPrefixes {
  feature: string;
  release: string;
  hotfix:  string;
  bugfix:  string;
  support: string;
}

export interface GitFlowFinishConfig {
  feature_delete_branch: boolean;
  feature_squash:        boolean;
  release_tag:           boolean;
  release_tag_prefix:    string;
  hotfix_tag:            boolean;
  /** Force PR/MR when finishing a feature (mandatory — no local merge option). */
  feature_use_pr:        boolean;
  /** Force PR/MR when finishing a release (mandatory — no local merge option). */
  release_use_pr:        boolean;
  /** Force PR/MR when finishing a hotfix (mandatory — no local merge option). */
  hotfix_use_pr:         boolean;
  /** Default action for the Finish Feature button when not forced: false = merge, true = PR/MR. */
  feature_pr_default:    boolean;
  /** Default action for the Finish Release button when not forced: false = merge, true = PR/MR. */
  release_pr_default:    boolean;
  /** Default action for the Finish Hotfix button when not forced: false = merge, true = PR/MR. */
  hotfix_pr_default:     boolean;
}

export interface GitFlowConfig {
  main_branch:    string;
  develop_branch: string;
  prefixes:       GitFlowPrefixes;
  finish:         GitFlowFinishConfig;
  /** When starting a feature/bugfix, require the name to come from an issue tracker ticket. */
  require_ticket_branch: boolean;
}

/** Result returned by feature/release/hotfix finish commands. */
export type FlowFinishResult =
  | { action: 'merged' }
  | { action: 'create_pr'; source_branch: string; target_branch: string };

/** Result returned by feature/release/hotfix start commands. */
export interface FlowStartResult {
  branch_name: string;
  base_branch: string;
  /** True when develop was missing and the branch was created from main. */
  fell_back_to_main: boolean;
}

export type GitFlowBranchType =
  | 'main'
  | 'develop'
  | 'feature'
  | 'release'
  | 'hotfix'
  | 'bugfix'
  | 'support'
  | 'other';

// ---------------------------------------------------------------------------
// Merge conflict resolution
// ---------------------------------------------------------------------------

/** Per-file presence flags for a conflicted entry — cheap query that scans
 *  the index without reading blobs. */
export interface ConflictPresence {
  path: string;
  ours_present: boolean;
  theirs_present: boolean;
}

/** Three-way content of a conflicted file, mirrors `ConflictContent` in Rust. */
export interface ConflictContent {
  path: string;
  /** Short name of the current HEAD branch (e.g. "main"). */
  ours_label: string;
  /** Short name of the incoming branch or short SHA (e.g. "feature/foo"). */
  theirs_label: string;
  /** Clean file content from HEAD (index stage 2 — no conflict markers).
   *  Empty when `!ours_present`. */
  ours_content: string;
  /** Clean file content from incoming branch (index stage 3).
   *  Empty when `!theirs_present`. */
  theirs_content: string;
  /** Common ancestor content (index stage 1), may be null. */
  base_content: string | null;
  /** On-disk content — currently contains raw `<<<<<<<` conflict markers
   *  (or just the THEIRS content for add-by-them conflicts). */
  working_content: string;
  /**
   * Encoding label inferred from the working-tree file (and used for the
   * stage blobs). Pass it back to `resolveConflict` so the file is
   * re-encoded with its original byte representation. "UTF-8" for modern
   * files; legacy Java/PHP sources on Windows are often "windows-1252".
   */
  encoding: string;
  /** True when index stage-2 exists (file is present on our side).  False
   *  for "added by them" conflicts — the file is new on the incoming side. */
  ours_present: boolean;
  /** True when index stage-3 exists (file is present on their side).  False
   *  for "deleted by them" conflicts. */
  theirs_present: boolean;
}

// ---- Ticket Links ----

export type LinkSource = 'auto_message' | 'auto_branch' | 'manual';
export type StorageBackend = 'git_notes' | 'links_toml';

export interface TicketLink {
  ticket_id: string;
  tracker:   string;   // "linear" | "github" | "gitlab"
  source:    LinkSource;
}

export interface TicketLinkConfig {
  storage:         StorageBackend;
  tracker:         string | null;
  auto_parse:      boolean;
  warn_push:       boolean;
  /** Custom regex pattern overriding the tracker default. Must contain
   *  exactly one capture group, e.g. `"\\b(MYCO-\\d+)\\b"`. */
  custom_pattern?: string;
}

export interface TicketLinksRepoConfig {
  storage?:        StorageBackend;
  tracker?:        string;
  auto_parse?:     boolean;
  custom_pattern?: string;
}

export interface CommitQueryItem {
  sha:     string;
  message: string;
  refs:    string[];
}

export interface LinkedCommitRef {
  sha:         string;
  short_oid:   string;
  summary:     string;
  author_name: string;
  timestamp:   number;  // unix seconds
  source:      LinkSource;
}

export interface TicketLinksGlobalConfig {
  enabled:    boolean;
  storage:    StorageBackend;
  auto_parse: boolean;
  warn_push:  boolean;
}

export interface GitFlowStatus {
  initialized:         boolean;
  current_branch:      string;
  current_branch_type: GitFlowBranchType;
  /** Name part only, e.g. "my-feature" from "feature/my-feature". */
  current_flow_name:   string | null;
  active_features:     string[];
  active_releases:     string[];
  active_hotfixes:     string[];
  develop_exists:      boolean;
  main_exists:         boolean;
}

// ---------------------------------------------------------------------------
// Worktrees
// ---------------------------------------------------------------------------

export type ProjectType =
  | 'rust'
  | 'node_js'
  | 'java_maven'
  | 'java_gradle'
  | 'go'
  | 'python'
  | 'dot_net'
  | 'cpp'
  | 'ruby'
  | 'php'
  | 'unknown';

export interface WorktreeInfo {
  path: string;
  branch: string | null;
  head_sha: string | null;
  head_short: string | null;
  is_main: boolean;
  is_locked: boolean;
  is_current: boolean;
  project_type: ProjectType;
  ahead: number;
  behind: number;
  changes_count: number;
}

// ---------------------------------------------------------------------------
// IDE config
// ---------------------------------------------------------------------------

export interface IdeEntry {
  id: string;
  name: string;
  command: string;
  args: string[];
}

export interface IdeConfig {
  default_ide: string;
  custom_ides: IdeEntry[];
  path_overrides: Record<string, string>;
  /** Maps project type (e.g. "rust") → ide_id. Overrides default_ide per language. */
  language_defaults: Record<string, string>;
}

export interface DetectedIde {
  id: string;
  name: string;
  available: boolean;
  detected_path: string | null;
}

// ---------------------------------------------------------------------------
// Reflog
// ---------------------------------------------------------------------------

export interface ReflogEntry {
  index:          number;
  id:             string;
  id_old:         string;
  message:        string;
  committer_name: string;
  committer_time: number;
}

// ---------------------------------------------------------------------------
// Recovery journal — pre-destructive-operation snapshots
// ---------------------------------------------------------------------------

export type RecoveryKind =
  | 'reset_hard'
  | 'checkout'
  | 'discard'
  | 'stash_force_apply'
  | 'stash_drop'
  | 'pull'
  | 'other';

export interface SkippedFile {
  path:    string;
  size:    number;
  reason:  string;
  tracked: boolean;
}

export interface RecoveryEntry {
  id:             number;
  created_at:     number;
  kind:           RecoveryKind;
  summary:        string;
  snapshot_oid:   string;
  head_oid:       string | null;
  head_branch:    string | null;
  ref_name:       string;
  consumed:       boolean;
  skipped_files:  SkippedFile[];
}

export interface RecoveryRestorePreview {
  changed_files:    string[];
  workdir_is_dirty: boolean;
}

export interface RecoveryConfig {
  max_file_size:   number;
  deny_extensions: string[];
  /** Days of snapshots to keep; 0 disables time-based expiry. */
  retention_days:  number;
}

// ---------------------------------------------------------------------------
// Git Bisect
// ---------------------------------------------------------------------------

export interface BisectState {
  active: boolean;
  /** The commit currently being tested (HEAD during bisect). */
  current_hash: string | null;
  /** All commits marked as bad during this session. */
  bad_hashes: string[];
  good_hashes: string[];
  /** Approximate remaining steps, available while session is in progress. */
  steps_remaining: number | null;
  /** Set when bisect has identified the first bad commit. */
  result_hash: string | null;
  result_message: string | null;
  /** True when the log has at least one mark that can be undone. */
  can_undo: boolean;
}

// ---------------------------------------------------------------------------
// Repository Statistics
// ---------------------------------------------------------------------------

export interface ContributorStat {
  name: string;
  email: string;
  commit_count: number;
  percentage: number;
}

export interface FileChangeStat {
  path: string;
  change_count: number;
}

export interface AuthorLineStat {
  name: string;
  email: string;
  lines_added: number;
  lines_deleted: number;
  total_changes: number;
}

export interface RepoStats {
  total_commits: number;
  total_contributors: number;
  /** Unix seconds of the oldest commit. */
  first_commit_time: number;
  /** Unix seconds of the newest commit. */
  last_commit_time: number;
  /** Number of unique calendar days that had at least one commit. */
  active_days: number;
  /** Last 365 days: [YYYY-MM-DD, count] pairs for days with ≥1 commit. */
  commits_by_day: [string, number][];
  /** Top 10 contributors by commit count. */
  top_contributors: ContributorStat[];
  /** Commit count per hour of day, index 0–23. */
  commits_by_hour: number[];
  /** Commit count per weekday: 0=Mon … 6=Sun. */
  commits_by_weekday: number[];
  /** Most changed files (scanned from the first 500 commits). */
  most_changed_files: FileChangeStat[];
  /** Top 10 file extensions by cumulative change count: [".ext", count]. */
  file_type_breakdown: [string, number][];
  /** Top contributor in the last 7 days. */
  top_contributor_week: ContributorStat | null;
  /** Top contributor in the last 30 days. */
  top_contributor_month: ContributorStat | null;
  /** Author with most total lines changed (first 500 commits). */
  top_changer: AuthorLineStat | null;
  /** Top 10 authors by lines changed, sorted desc. */
  top_changers: AuthorLineStat[];
  /** Date with the highest single-day commit count: [YYYY-MM-DD, count]. */
  busiest_day: [string, number] | null;
  /** Average commits per calendar week over the full project lifetime. */
  avg_commits_per_week: number;
  /** Longest streak of consecutive days with at least one commit. */
  longest_streak: number;
  /** Average lines changed per commit (first 500 commits). */
  avg_commit_size: number;
}

export type BisectSessionStatus = 'paused' | 'completed';

export interface BisectSession {
  id: string;
  name: string;
  created_at: number;  // Unix ms
  updated_at: number;  // Unix ms
  status: BisectSessionStatus;
  bad_hashes: string[];
  good_hashes: string[];
  result_hash: string | null;
  result_message: string | null;
}
