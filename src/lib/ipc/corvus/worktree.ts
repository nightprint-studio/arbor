import { corvus } from '../rpc';
import type { WorktreeInfo, ProjectType, IdeConfig, DetectedIde } from '$lib/types/corvus/git';

export const listWorktrees = (tabId: string) =>
  corvus<WorktreeInfo[]>('list_worktrees', { tab_id: tabId });

export const addWorktree = (
  tabId: string,
  destPath: string,
  branch: string,
  newBranch?: string,
) =>
  corvus<void>('add_worktree', { tab_id: tabId, dest_path: destPath, branch, new_branch: newBranch ?? null });

export const removeWorktree = (tabId: string, worktreePath: string) =>
  corvus<void>('remove_worktree', { tab_id: tabId, worktree_path: worktreePath });

export const detectProjectType = (path: string) =>
  corvus<ProjectType>('detect_project_type', { path });

export const openInIde = (path: string, ideId?: string) =>
  corvus<void>('open_in_ide', { path, ide_id: ideId ?? null });

export const getIdeConfig = () =>
  corvus<IdeConfig>('get_ide_config');

export const setIdeConfig = (config: IdeConfig) =>
  corvus<void>('set_ide_config', { config });

/** Fire IDE detection as a non-cancellable background job.
 *  Returns the job_id. Results arrive via the `arbor://ide-detection-done` event. */
export const startIdeDetection = () =>
  corvus<string>('start_ide_detection');

// ── Per-repo IDE preference (.arbor/config.toml → ide_id) ─────────────────────

/** Read the project-bound IDE for the given tab, or `null` when the
 *  repo defers to the global default. */
export const getRepoIde = (tabId: string) =>
  corvus<string | null>('get_repo_ide', { tab_id: tabId });

/** Persist the project-bound IDE.  Pass `null` to clear the override and
 *  fall back to the global default. */
export const setRepoIde = (tabId: string, ideId: string | null) =>
  corvus<void>('set_repo_ide', { tab_id: tabId, ide_id: ideId });
