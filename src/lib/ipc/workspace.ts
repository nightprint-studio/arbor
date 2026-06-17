import { invoke } from '@tauri-apps/api/core';
import { platform } from './rpc';
import type {
  WorkspacesSnapshot, WorkspaceDef, WorkspaceGroup, WorkspacePatch, WorkspaceGroupPatch,
  RepoRegistryEntry, RepoRegistryEntryWithRoot, RepoRegistrationResult, TabSnapshot,
  CrossWsTabRef, TabMeta, ExportedWorkspace, ImportPreview, RepoHealth,
  ExportedWorkspaceGroup, ImportGroupPreview, ExportedBundle, ImportBundleResult,
  WorkspaceFetchStartResult, MigrationReport,
} from '../types/workspace';

// ── Queries ─────────────────────────────────────────────────────────────────

export const listWorkspaces   = (): Promise<WorkspacesSnapshot>      => platform('list_workspaces');
export const listRegistryRepos = (): Promise<RepoRegistryEntry[]>    => platform('list_registry_repos');
export const listRegistryWithRoots = (): Promise<RepoRegistryEntryWithRoot[]> => platform('list_registry_with_roots');
export const loadWorkspaceSnapshot = (workspaceId: string): Promise<TabSnapshot> =>
  platform('load_workspace_snapshot', { workspace_id: workspaceId });

// ── Workspace lifecycle ─────────────────────────────────────────────────────

export const createWorkspace = (
  name: string, colorIdx: number, repoIds: string[], groupId: string | null,
): Promise<WorkspaceDef> =>
  invoke('create_workspace', { name, colorIdx, repoIds, groupId });

export const updateWorkspace = (workspaceId: string, patch: WorkspacePatch): Promise<WorkspaceDef> =>
  invoke('update_workspace', { workspaceId, patch });

export const deleteWorkspace = (workspaceId: string): Promise<void> =>
  invoke('delete_workspace', { workspaceId });

export const reorderWorkspaces = (orderedIds: string[]): Promise<void> =>
  invoke('reorder_workspaces', { orderedIds });

export const setActiveWorkspace = (workspaceId: string): Promise<WorkspaceDef> =>
  invoke('set_active_workspace', { workspaceId });

// ── Groups ──────────────────────────────────────────────────────────────────

export const createWorkspaceGroup = (name: string, colorIdx: number): Promise<WorkspaceGroup> =>
  platform('create_workspace_group', { name, color_idx: colorIdx });

export const updateWorkspaceGroup = (groupId: string, patch: WorkspaceGroupPatch): Promise<WorkspaceGroup> =>
  platform('update_workspace_group', { group_id: groupId, patch });

export const deleteWorkspaceGroup = (groupId: string): Promise<void> =>
  platform('delete_workspace_group', { group_id: groupId });

export const reorderWorkspaceGroups = (orderedIds: string[]): Promise<void> =>
  platform('reorder_workspace_groups', { ordered_ids: orderedIds });

export const setWorkspaceGroup = (workspaceId: string, groupId: string | null): Promise<void> =>
  platform('set_workspace_group', { workspace_id: workspaceId, group_id: groupId });

// ── Repo membership ─────────────────────────────────────────────────────────

export const addRepoToWorkspace = (workspaceId: string, repoId: string): Promise<void> =>
  invoke('add_repo_to_workspace', { workspaceId, repoId });

export const removeRepoFromWorkspace = (workspaceId: string, repoId: string): Promise<void> =>
  invoke('remove_repo_from_workspace', { workspaceId, repoId });

export const moveRepoBetweenWorkspaces = (
  fromWorkspaceId: string, toWorkspaceId: string, repoId: string,
): Promise<void> =>
  invoke('move_repo_between_workspaces', { fromWorkspaceId, toWorkspaceId, repoId });

// ── Registry ────────────────────────────────────────────────────────────────

export const registerRepoPath = (
  path: string, remoteUrl: string | null, displayName: string | null,
): Promise<RepoRegistrationResult> =>
  invoke('register_repo_path', { path, remoteUrl, displayName });

/** Register a "pending" repo (declared via name + optional remote URL, not yet
 *  on disk). Returns the new registry id. Used by the non-blocking import so a
 *  member can be cloned / located later from Repository Management. */
export const registerPendingRepo = (
  name: string, remoteUrl: string | null,
): Promise<string> =>
  invoke('register_pending_repo', { name, remoteUrl });

export const updateRegistryRepo = (
  repoId: string,
  patch: { display_name?: string; remote_url?: string | null; path?: string },
): Promise<RepoRegistryEntry> =>
  invoke('update_registry_repo', {
    repoId,
    displayName: patch.display_name,
    remoteUrl:   patch.remote_url !== undefined ? patch.remote_url : undefined,
    path:        patch.path,
  });

export const deleteRegistryRepo = (repoId: string): Promise<void> =>
  invoke('delete_registry_repo', { repoId });

// ── Tab snapshots ───────────────────────────────────────────────────────────

export const saveWorkspaceSnapshot = (
  workspaceId: string,
  openTabIds: string[],
  activeTabId: string | null,
  crossWsTabs: CrossWsTabRef[],
  tabMeta: TabMeta[] = [],
): Promise<void> =>
  platform('save_workspace_snapshot', {
    workspace_id:  workspaceId,
    open_tab_ids:  openTabIds,
    active_tab_id: activeTabId,
    cross_ws_tabs: crossWsTabs,
    tab_meta:      tabMeta,
  });

// ── Import / export ─────────────────────────────────────────────────────────

export const exportWorkspace = (workspaceId: string): Promise<ExportedWorkspace> =>
  platform('export_workspace', { workspace_id: workspaceId });

export const importWorkspacePreview = (payload: ExportedWorkspace): Promise<ImportPreview> =>
  platform('import_workspace_preview', { payload });

export const importWorkspaceCommit = (
  name: string, colorIdx: number, repoIds: string[], groupId: string | null,
  mergeInto: string | null = null,
): Promise<WorkspaceDef> =>
  invoke('import_workspace_commit', { name, colorIdx, repoIds, groupId, mergeInto });

export const exportWorkspaceGroup = (groupId: string): Promise<ExportedWorkspaceGroup> =>
  platform('export_workspace_group', { group_id: groupId });

export const importWorkspaceGroupPreview = (payload: ExportedWorkspaceGroup): Promise<ImportGroupPreview> =>
  platform('import_workspace_group_preview', { payload });

export const importWorkspaceGroupCommit = (
  name: string, colorIdx: number, existingGroupId: string | null,
  workspaces: { name: string; color_idx: number; repo_ids: string[]; merge_into: string | null }[],
): Promise<void> =>
  invoke('import_workspace_group_commit', { name, colorIdx, existingGroupId, workspaces });

/** Export every group + top-level workspace into one portable backup bundle. */
export const exportAllWorkspaces = (): Promise<ExportedBundle> =>
  platform('export_all_workspaces');

/** Restore a backup bundle (non-blocking: unknown repos land as "not cloned"). */
export const importBundleCommit = (payload: ExportedBundle): Promise<ImportBundleResult> =>
  invoke('import_bundle_commit', { payload });

// ── Health + fetch-all ──────────────────────────────────────────────────────

export const workspaceHealthScan = (workspaceId: string): Promise<RepoHealth[]> =>
  platform('workspace_health_scan', { workspace_id: workspaceId });

export const workspaceFetchAll = (workspaceId: string): Promise<WorkspaceFetchStartResult> =>
  invoke('workspace_fetch_all', { workspaceId });

export const workspacePullAll = (workspaceId: string): Promise<WorkspaceFetchStartResult> =>
  invoke('workspace_pull_all', { workspaceId });

export const workspaceTagAll = (
  workspaceId: string, tagName: string, message: string | null, push: boolean,
): Promise<WorkspaceFetchStartResult> =>
  invoke('workspace_tag_all', { workspaceId, tagName, message, push });

// ── Migration ───────────────────────────────────────────────────────────────

export const takeMigrationReport = (): Promise<MigrationReport | null> =>
  platform('take_migration_report');
