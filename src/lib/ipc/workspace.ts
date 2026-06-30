import { corvus, sitta } from './rpc';
import type {
  WorkspacesSnapshot, WorkspaceDef, WorkspaceGroup, WorkspacePatch, WorkspaceGroupPatch,
  RepoRegistryEntry, RepoRegistryEntryWithRoot, RepoRegistrationResult, TabSnapshot,
  CrossWsTabRef, TabMeta, ExportedWorkspace, ImportPreview, RepoHealth,
  ExportedWorkspaceGroup, ImportGroupPreview, ExportedBundle, ImportBundleResult,
  WorkspaceFetchStartResult,
} from '../types/workspace';

// ── Queries ─────────────────────────────────────────────────────────────────

export const listWorkspaces   = (): Promise<WorkspacesSnapshot>      => corvus('list_workspaces');
export const listRegistryRepos = (): Promise<RepoRegistryEntry[]>    => corvus('list_registry_repos');

// Read-only twins served by sitta-be (the File Explorer's own backend), which
// reads the same repos.json / workspaces.json directly. For OPTIONAL consumers
// that must list projects without the git product running — e.g. the File
// Explorer's Projects sidebar — so they never poke (or depend on) corvus-be.
// Mutations still go through the corvus functions above (a git-product action).
export const listWorkspacesLocal   = (): Promise<WorkspacesSnapshot>   => sitta('list_workspaces');
export const listRegistryReposLocal = (): Promise<RepoRegistryEntry[]> => sitta('list_registry_repos');
export const listRegistryWithRoots = (): Promise<RepoRegistryEntryWithRoot[]> => corvus('list_registry_with_roots');
export const loadWorkspaceSnapshot = (workspaceId: string): Promise<TabSnapshot> =>
  corvus('load_workspace_snapshot', { workspace_id: workspaceId });

// ── Workspace lifecycle ─────────────────────────────────────────────────────

export const createWorkspace = (
  name: string, colorIdx: number, repoIds: string[], groupId: string | null,
): Promise<WorkspaceDef> =>
  corvus('create_workspace', { name, color_idx: colorIdx, repo_ids: repoIds, group_id: groupId });

export const updateWorkspace = (workspaceId: string, patch: WorkspacePatch): Promise<WorkspaceDef> =>
  corvus('update_workspace', { workspace_id: workspaceId, patch });

export const deleteWorkspace = (workspaceId: string): Promise<void> =>
  corvus('delete_workspace', { workspace_id: workspaceId });

export const reorderWorkspaces = (orderedIds: string[]): Promise<void> =>
  corvus('reorder_workspaces', { ordered_ids: orderedIds });

export const setActiveWorkspace = (workspaceId: string): Promise<WorkspaceDef> =>
  corvus('set_active_workspace', { workspace_id: workspaceId });

// ── Groups ──────────────────────────────────────────────────────────────────

export const createWorkspaceGroup = (name: string, colorIdx: number): Promise<WorkspaceGroup> =>
  corvus('create_workspace_group', { name, color_idx: colorIdx });

export const updateWorkspaceGroup = (groupId: string, patch: WorkspaceGroupPatch): Promise<WorkspaceGroup> =>
  corvus('update_workspace_group', { group_id: groupId, patch });

export const deleteWorkspaceGroup = (groupId: string): Promise<void> =>
  corvus('delete_workspace_group', { group_id: groupId });

export const reorderWorkspaceGroups = (orderedIds: string[]): Promise<void> =>
  corvus('reorder_workspace_groups', { ordered_ids: orderedIds });

export const setWorkspaceGroup = (workspaceId: string, groupId: string | null): Promise<void> =>
  corvus('set_workspace_group', { workspace_id: workspaceId, group_id: groupId });

// ── Repo membership ─────────────────────────────────────────────────────────

export const addRepoToWorkspace = (workspaceId: string, repoId: string): Promise<void> =>
  corvus('add_repo_to_workspace', { workspace_id: workspaceId, repo_id: repoId });

export const removeRepoFromWorkspace = (workspaceId: string, repoId: string): Promise<void> =>
  corvus('remove_repo_from_workspace', { workspace_id: workspaceId, repo_id: repoId });

export const moveRepoBetweenWorkspaces = (
  fromWorkspaceId: string, toWorkspaceId: string, repoId: string,
): Promise<void> =>
  corvus('move_repo_between_workspaces', {
    from_workspace_id: fromWorkspaceId,
    to_workspace_id:   toWorkspaceId,
    repo_id:           repoId,
  });

// ── Registry ────────────────────────────────────────────────────────────────

export const registerRepoPath = (
  path: string, remoteUrl: string | null, displayName: string | null,
): Promise<RepoRegistrationResult> =>
  corvus('register_repo_path', { path, remote_url: remoteUrl, display_name: displayName });

/** Register a "pending" repo (declared via name + optional remote URL, not yet
 *  on disk). Returns the new registry id. Used by the non-blocking import so a
 *  member can be cloned / located later from Repository Management. */
export const registerPendingRepo = (
  name: string, remoteUrl: string | null,
): Promise<string> =>
  corvus('register_pending_repo', { name, remote_url: remoteUrl });

export const updateRegistryRepo = (
  repoId: string,
  patch: { display_name?: string; remote_url?: string | null; path?: string },
): Promise<RepoRegistryEntry> =>
  corvus('update_registry_repo', {
    repo_id:      repoId,
    display_name: patch.display_name,
    remote_url:   patch.remote_url !== undefined ? patch.remote_url : undefined,
    path:         patch.path,
  });

export const deleteRegistryRepo = (repoId: string): Promise<void> =>
  corvus('delete_registry_repo', { repo_id: repoId });

// ── Tab snapshots ───────────────────────────────────────────────────────────

export const saveWorkspaceSnapshot = (
  workspaceId: string,
  openTabIds: string[],
  activeTabId: string | null,
  crossWsTabs: CrossWsTabRef[],
  tabMeta: TabMeta[] = [],
): Promise<void> =>
  corvus('save_workspace_snapshot', {
    workspace_id:  workspaceId,
    open_tab_ids:  openTabIds,
    active_tab_id: activeTabId,
    cross_ws_tabs: crossWsTabs,
    tab_meta:      tabMeta,
  });

// ── Import / export ─────────────────────────────────────────────────────────

export const exportWorkspace = (workspaceId: string): Promise<ExportedWorkspace> =>
  corvus('export_workspace', { workspace_id: workspaceId });

export const importWorkspacePreview = (payload: ExportedWorkspace): Promise<ImportPreview> =>
  corvus('import_workspace_preview', { payload });

export const importWorkspaceCommit = (
  name: string, colorIdx: number, repoIds: string[], groupId: string | null,
  mergeInto: string | null = null,
): Promise<WorkspaceDef> =>
  corvus('import_workspace_commit', {
    name, color_idx: colorIdx, repo_ids: repoIds, group_id: groupId, merge_into: mergeInto,
  });

export const exportWorkspaceGroup = (groupId: string): Promise<ExportedWorkspaceGroup> =>
  corvus('export_workspace_group', { group_id: groupId });

export const importWorkspaceGroupPreview = (payload: ExportedWorkspaceGroup): Promise<ImportGroupPreview> =>
  corvus('import_workspace_group_preview', { payload });

export const importWorkspaceGroupCommit = (
  name: string, colorIdx: number, existingGroupId: string | null,
  workspaces: { name: string; color_idx: number; repo_ids: string[]; merge_into: string | null }[],
): Promise<void> =>
  corvus('import_workspace_group_commit', {
    name, color_idx: colorIdx, existing_group_id: existingGroupId, workspaces,
  });

/** Export every group + top-level workspace into one portable backup bundle. */
export const exportAllWorkspaces = (): Promise<ExportedBundle> =>
  corvus('export_all_workspaces');

/** Restore a backup bundle (non-blocking: unknown repos land as "not cloned"). */
export const importBundleCommit = (payload: ExportedBundle): Promise<ImportBundleResult> =>
  corvus('import_bundle_commit', { payload });

// ── Health + fetch-all ──────────────────────────────────────────────────────

export const workspaceHealthScan = (workspaceId: string): Promise<RepoHealth[]> =>
  corvus('workspace_health_scan', { workspace_id: workspaceId });

export const workspaceFetchAll = (workspaceId: string): Promise<WorkspaceFetchStartResult> =>
  corvus('workspace_fetch_all', { workspace_id: workspaceId });

export const workspacePullAll = (workspaceId: string): Promise<WorkspaceFetchStartResult> =>
  corvus('workspace_pull_all', { workspace_id: workspaceId });

export const workspaceTagAll = (
  workspaceId: string, tagName: string, message: string | null, push: boolean,
): Promise<WorkspaceFetchStartResult> =>
  corvus('workspace_tag_all', {
    workspace_id: workspaceId, tag_name: tagName, message, push,
  });
