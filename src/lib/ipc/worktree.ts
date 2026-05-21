import { invoke } from '@tauri-apps/api/core';
import type { WorktreeInfo, ProjectType, IdeConfig, DetectedIde } from '$lib/types/git';

export const listWorktrees = (tabId: string) =>
  invoke<WorktreeInfo[]>('list_worktrees', { tabId });

export const addWorktree = (
  tabId: string,
  destPath: string,
  branch: string,
  newBranch?: string,
) =>
  invoke<void>('add_worktree', { tabId, destPath, branch, newBranch: newBranch ?? null });

export const removeWorktree = (tabId: string, worktreePath: string) =>
  invoke<void>('remove_worktree', { tabId, worktreePath });

export const detectProjectType = (path: string) =>
  invoke<ProjectType>('detect_project_type', { path });

export const openInIde = (path: string, ideId?: string) =>
  invoke<void>('open_in_ide', { path, ideId: ideId ?? null });

export const getIdeConfig = () =>
  invoke<IdeConfig>('get_ide_config');

export const setIdeConfig = (config: IdeConfig) =>
  invoke<void>('set_ide_config', { config });

/** Fire IDE detection as a non-cancellable background job.
 *  Returns the job_id. Results arrive via the `arbor://ide-detection-done` event. */
export const startIdeDetection = () =>
  invoke<string>('start_ide_detection');

// ── Per-repo IDE preference (.arbor/config.toml → ide_id) ─────────────────────

/** Read the project-bound IDE for the given tab, or `null` when the
 *  repo defers to the global default. */
export const getRepoIde = (tabId: string) =>
  invoke<string | null>('get_repo_ide', { tabId });

/** Persist the project-bound IDE.  Pass `null` to clear the override and
 *  fall back to the global default. */
export const setRepoIde = (tabId: string, ideId: string | null) =>
  invoke<void>('set_repo_ide', { tabId, ideId });
