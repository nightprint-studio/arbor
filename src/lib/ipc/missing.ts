import { invoke } from '@tauri-apps/api/core';

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
  invoke('validate_repo_path', { path });

export const validateRepoPaths = (paths: string[]): Promise<RepoPathValidation[]> =>
  invoke('validate_repo_paths', { paths });

export const relocateRepo = (repoId: string, newPath: string): Promise<RelocateResult> =>
  invoke('relocate_repo', { repoId, newPath });

export const reportRepoMissing = (repoId: string, path: string, reason: RepoPathStatus): Promise<void> =>
  invoke('report_repo_missing', { repoId, path, reason });

export const removeRecentRepo = (path: string): Promise<void> =>
  invoke('remove_recent_repo', { path });

export const cleanupMissingRecentRepos = (): Promise<string[]> =>
  invoke('cleanup_missing_recent_repos');

export const getMissingProjectsConfig = (): Promise<MissingProjectsConfig> =>
  invoke('get_missing_projects_config');

export const setMissingProjectsConfig = (config: MissingProjectsConfig): Promise<void> =>
  invoke('set_missing_projects_config', { config });
