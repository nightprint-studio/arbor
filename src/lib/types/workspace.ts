// ---------------------------------------------------------------------------
// Workspace subsystem — TypeScript mirrors of src-tauri/src/workspace/*.rs
// and the DTOs exposed by commands/workspace_commands.rs.
// ---------------------------------------------------------------------------

/** Fixed id of the implicit "Scratch" workspace. */
export const SCRATCH_ID = 'scratch';

/** Reserved for a future per-workspace identity override (unused v1). */
export interface GitIdentity {
  name:  string;
  email: string;
}

/** Optional visual parent — groups do not change workspace behaviour. */
export interface WorkspaceGroup {
  id:        string;
  name:      string;
  order:     number;
  collapsed: boolean;
  color_idx: number;
}

export interface WorkspaceDef {
  id:        string;
  name:      string;
  color_idx: number;
  repo_ids:  string[];
  order:     number;
  group_id:  string | null;
  // Reserved extensibility fields — unused in v1.
  metadata:          unknown;
  settings_override: unknown | null;
  git_identity:      GitIdentity | null;
}

export interface WorkspacesSnapshot {
  workspaces:          WorkspaceDef[];
  groups:              WorkspaceGroup[];
  active_workspace_id: string | null;
}

export interface RepoRegistryEntry {
  id:           string;
  path:         string;
  remote_url:   string | null;
  display_name: string;
}

/** Registry entry augmented at request time with the path to its `.git`
 *  common dir.  Two entries that share `common_dir` are linked worktrees of
 *  the same repository — the UI uses that to group them together. */
export interface RepoRegistryEntryWithRoot extends RepoRegistryEntry {
  common_dir:     string | null;
  current_branch: string | null;
  /** True when this path is a linked worktree (not the main checkout).
   *  Pickers that should only offer "root" repos (workspace creation,
   *  edit) filter on this. */
  is_worktree:    boolean;
}

export interface CrossWsTabRef {
  repo_id:      string;
  source_ws_id: string;
}

/** Per-tab metadata that persists across app restarts but isn't derivable
 *  from the underlying repo. Currently used by the worktree switcher. */
export interface TabMeta {
  repo_id:            string;
  name_override:      string | null;
  is_linked_worktree: boolean;
}

export interface TabSnapshot {
  open_tab_ids:  string[];
  active_tab_id: string | null;
  cross_ws_tabs: CrossWsTabRef[];
  tab_meta?:     TabMeta[];
}

export interface WorkspacePatch {
  name?:      string;
  color_idx?: number;
  /** `null` inside Some → clear group. undefined → leave untouched. */
  group_id?:  string | null;
  repo_ids?:  string[];
}

export interface WorkspaceGroupPatch {
  name?:      string;
  color_idx?: number;
  collapsed?: boolean;
}

export interface RepoRegistrationResult {
  id:          string;
  existed:     boolean;
  added_to_ws: boolean;
}

export interface ExportedRepo {
  name:       string;
  remote_url: string | null;
}

export interface ExportedWorkspace {
  arbor_workspace_version: number;
  name:                    string;
  color_idx:               number;
  repos:                   ExportedRepo[];
}

export interface ImportPreviewRepo {
  name:          string;
  remote_url:    string | null;
  existing_id:   string | null;
  existing_path: string | null;
}

export interface ImportPreview {
  name:      string;
  color_idx: number;
  repos:     ImportPreviewRepo[];
}

export interface RepoHealth {
  repo_id:      string;
  path:         string;
  missing:      boolean;
  branch:       string | null;
  ahead:        number;
  behind:       number;
  has_upstream: boolean;
  dirty:        boolean;
  conflicted:   boolean;
  detached:     boolean;
  /** True when the repo path is a linked worktree (lives under
   *  `.git/worktrees/<name>`), not the main checkout. */
  is_worktree:  boolean;
  error:        string | null;
}

export interface WorkspaceFetchStartResult {
  job_id: string;
  total:  number;
}

export interface WorkspaceFetchProgressEvent {
  job_id:       string;
  workspace_id: string;
  repo_id:      string;
  index:        number;
  total:        number;
  /** 'start' | 'ok' | 'error' */
  phase:        string;
  error?:       string;
}

export interface WorkspacePullProgressEvent {
  job_id:       string;
  workspace_id: string;
  repo_id:      string;
  index:        number;
  total:        number;
  /** 'start' | 'ok' | 'error' | 'conflict' */
  phase:        string;
  error?:       string;
}

export interface WorkspacePullDoneEvent {
  job_id:       string;
  workspace_id: string;
  ok:           number;
  failed:       number;
  conflict:     number;
}

export interface WorkspaceTagProgressEvent {
  job_id:       string;
  workspace_id: string;
  repo_id:      string;
  index:        number;
  total:        number;
  /** 'start' | 'ok' | 'skipped' | 'error' */
  phase:        string;
  error?:       string;
}

export interface WorkspaceTagDoneEvent {
  job_id:       string;
  workspace_id: string;
  tag_name:     string;
  ok:           number;
  failed:       number;
  skipped:      number;
}

export interface MigrationReport {
  already_migrated:  boolean;
  added_repo_ids:    string[];
  existing_repo_ids: string[];
  active_repo_id:    string | null;
}

// ---------------------------------------------------------------------------
// Palette tokens — mirrored against the CSS variables defined in theme vars.
// The index is 0..11; CSS vars are named --ws-color-0 .. --ws-color-11 with
// light/dark variants living in the theme files.
// ---------------------------------------------------------------------------

export const WS_COLOR_COUNT = 12;

/** Derive short initials (<=2 uppercase letters) for the workspace dot. */
export function workspaceInitials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return '?';
  if (parts.length === 1) {
    const p = parts[0];
    const first = p.charAt(0).toUpperCase();
    const second = p.length > 1 ? p.charAt(1).toUpperCase() : '';
    return (first + second).trim() || first;
  }
  return (parts[0].charAt(0) + parts[1].charAt(0)).toUpperCase();
}

/** CSS var reference for a given palette index; falls back to index 0 on OOR. */
export function workspaceColorVar(idx: number): string {
  const i = Number.isFinite(idx) ? Math.max(0, Math.min(WS_COLOR_COUNT - 1, Math.floor(idx))) : 0;
  return `var(--ws-color-${i})`;
}
