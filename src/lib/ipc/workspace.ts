import { invoke } from '@tauri-apps/api/core';
import type {
  WorkspacesSnapshot, WorkspaceDef, WorkspaceGroup, WorkspacePatch, WorkspaceGroupPatch,
  RepoRegistryEntry, RepoRegistryEntryWithRoot, RepoRegistrationResult, TabSnapshot,
  CrossWsTabRef, TabMeta, ExportedWorkspace, ImportPreview, RepoHealth,
  WorkspaceFetchStartResult, MigrationReport,
} from '../types/workspace';

// ── Queries ─────────────────────────────────────────────────────────────────

export const listWorkspaces   = (): Promise<WorkspacesSnapshot>      => invoke('list_workspaces');
export const listRegistryRepos = (): Promise<RepoRegistryEntry[]>    => invoke('list_registry_repos');
export const listRegistryWithRoots = (): Promise<RepoRegistryEntryWithRoot[]> => invoke('list_registry_with_roots');
export const loadWorkspaceSnapshot = (workspaceId: string): Promise<TabSnapshot> =>
  invoke('load_workspace_snapshot', { workspaceId });

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
  invoke('create_workspace_group', { name, colorIdx });

export const updateWorkspaceGroup = (groupId: string, patch: WorkspaceGroupPatch): Promise<WorkspaceGroup> =>
  invoke('update_workspace_group', { groupId, patch });

export const deleteWorkspaceGroup = (groupId: string): Promise<void> =>
  invoke('delete_workspace_group', { groupId });

export const reorderWorkspaceGroups = (orderedIds: string[]): Promise<void> =>
  invoke('reorder_workspace_groups', { orderedIds });

export const setWorkspaceGroup = (workspaceId: string, groupId: string | null): Promise<void> =>
  invoke('set_workspace_group', { workspaceId, groupId });

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
  invoke('save_workspace_snapshot', { workspaceId, openTabIds, activeTabId, crossWsTabs, tabMeta });

// ── Import / export ─────────────────────────────────────────────────────────

export const exportWorkspace = (workspaceId: string): Promise<ExportedWorkspace> =>
  invoke('export_workspace', { workspaceId });

export const importWorkspacePreview = (payload: ExportedWorkspace): Promise<ImportPreview> =>
  invoke('import_workspace_preview', { payload });

export const importWorkspaceCommit = (
  name: string, colorIdx: number, repoIds: string[], groupId: string | null,
): Promise<WorkspaceDef> =>
  invoke('import_workspace_commit', { name, colorIdx, repoIds, groupId });

// ── Health + fetch-all ──────────────────────────────────────────────────────

export const workspaceHealthScan = (workspaceId: string): Promise<RepoHealth[]> =>
  invoke('workspace_health_scan', { workspaceId });

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
  invoke('take_migration_report');
