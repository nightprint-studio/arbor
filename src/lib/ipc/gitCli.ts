import { invoke } from '@tauri-apps/api/core';

export type GitCliSource = 'config' | 'path' | 'portable' | 'missing';

export interface GitCliStatus {
  path:               string | null;
  version:            string | null;
  source:             GitCliSource | null;
  download_supported: boolean;
  portable_dir:       string;
}

export type GitDownloadStage =
  | 'resolving'
  | 'downloading'
  | 'extracting'
  | 'verifying'
  | 'done'
  | 'error';

export interface GitDownloadProgress {
  stage:   GitDownloadStage;
  message: string;
  bytes:   number;
  total:   number;
}

export const getGitStatus = (): Promise<GitCliStatus> =>
  invoke('get_git_status');

export const redetectGit = (): Promise<GitCliStatus> =>
  invoke('redetect_git');

export const verifyGitPath = (path: string): Promise<string> =>
  invoke('verify_git_path', { path });

/** When `path` is null/empty the override is cleared and detection re-runs
 *  (PATH → portable copy). */
export const setGitPath = (path: string | null): Promise<GitCliStatus> =>
  invoke('set_git_path', { path });

export const downloadPortableGit = (): Promise<GitCliStatus> =>
  invoke('download_portable_git');

/** Signal an in-flight portable-git download to abort.  Cooperative — the
 *  backend stops at the next chunk / 7z entry boundary. */
export const cancelGitDownload = (): Promise<void> =>
  invoke('cancel_git_download');
