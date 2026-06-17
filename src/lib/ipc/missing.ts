import { invoke } from '@tauri-apps/api/core';
import { corvus, platform } from './rpc';

export type RepoPathStatus = 'ok' | 'missing' | 'unreachable' | 'not_a_repo';

export interface RepoPathValidation {
  status:  RepoPathStatus;
  message: string;
  ancestor_exists: boolean;
}

export interface RelocateResult {
  repo_id:  string;
  old_path: string;
  new_path: string;
  validation: RepoPathValidation;
}

export interface MissingProjectsConfig {
  auto_prune_recents:    boolean;
  confirm_before_remove: boolean;
  revalidate_on_focus:   boolean;
}

export const validateRepoPath  = (path: string): Promise<RepoPathValidation> =>
  corvus('validate_repo_path', { path });

export const validateRepoPaths = (paths: string[]): Promise<RepoPathValidation[]> =>
  corvus('validate_repo_paths', { paths });

export const relocateRepo = (repoId: string, newPath: string): Promise<RelocateResult> =>
  invoke('relocate_repo', { repoId, newPath });

export const reportRepoMissing = (repoId: string, path: string, reason: RepoPathStatus): Promise<void> =>
  corvus('report_repo_missing', { repo_id: repoId, path, reason });

export const removeRecentRepo = (path: string): Promise<void> =>
  corvus('remove_recent_repo', { path });

export const cleanupMissingRecentRepos = (): Promise<string[]> =>
  corvus('cleanup_missing_recent_repos');

export const getMissingProjectsConfig = (): Promise<MissingProjectsConfig> =>
  platform('get_missing_projects_config');

export const setMissingProjectsConfig = (config: MissingProjectsConfig): Promise<void> =>
  platform('set_missing_projects_config', { config });
