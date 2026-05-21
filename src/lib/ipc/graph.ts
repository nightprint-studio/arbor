import { invoke } from '@tauri-apps/api/core';
import type { GraphData, CommitDetail, RepoInfo, InitRepoOptions, InitRepoResult, CloneOptions, RepoFileEntry } from '../types/git';

export const openRepo = (path: string, tabId: string) =>
  invoke<RepoInfo>('open_repo', { path, tabId });

/** Returns true when `path` is inside a git repository. */
export const checkIsGitRepo = (path: string) =>
  invoke<boolean>('check_is_git_repo', { path });

/** Read user.name / user.email from the global git config. */
export const getGitIdentity = () =>
  invoke<[string, string]>('get_git_identity');

/** Initialise a new git repository with the given options. */
export const initRepo = (path: string, tabId: string, options: InitRepoOptions) =>
  invoke<InitRepoResult>('init_repo', { path, tabId, options });

export const closeRepo = (tabId: string) =>
  invoke<void>('close_repo', { tabId });

/** List branch names available on a remote URL without cloning. */
export const listRemoteBranchesForUrl = (url: string) =>
  invoke<string[]>('list_remote_branches_for_url', { url });

/** Clone a remote repository and open it as a new tab. */
export const cloneRepo = (opts: CloneOptions, tabId: string) =>
  invoke<RepoInfo>('clone_repo', { opts, tabId });

export const getRepoInfo = (tabId: string) =>
  invoke<RepoInfo>('get_repo_info', { tabId });

export const getGraph = (tabId: string, offset = 0, limit = 500) =>
  invoke<GraphData>('get_graph', { tabId, offset, limit });

export const getGraphForFile = (tabId: string, filePath: string, offset = 0, limit = 500) =>
  invoke<GraphData>('get_graph_for_file', { tabId, filePath, offset, limit });

export const getCommitDetail = (tabId: string, oid: string) =>
  invoke<CommitDetail>('get_commit_detail', { tabId, oid });

export const getRepoFileTree = (tabId: string) =>
  invoke<RepoFileEntry[]>('get_repo_file_tree', { tabId });

/** Fast: returns all tracked file paths from the index, no commit walking. */
export const getRepoFiles = (tabId: string) =>
  invoke<string[]>('get_repo_files', { tabId });

/** Lazy: returns the last commit that touched each path in the given list. */
export const getFilesLastCommit = (tabId: string, paths: string[]) =>
  invoke<RepoFileEntry[]>('get_files_last_commit', { tabId, paths });

/** Starts a background scan that emits:
 *  - `arbor://file-meta-batch` {tab_id, entries[]} progressively
 *  - `arbor://file-meta-done`  {tab_id} when complete */
export const startFileMetaScan = (tabId: string) =>
  invoke<void>('start_file_meta_scan', { tabId });

/** Returns a fast fingerprint of the repo's current ref state (HEAD SHA + all refs).
 *  Used by the cache scheduler to detect remote changes without loading the full graph. */
export const getRepoFingerprint = (tabId: string) =>
  invoke<string>('get_repo_fingerprint', { tabId });

/** Kick off a background job that exports the full commit graph as an SVG file.
 *  Pass `themeVars` (the active theme's CSS-vars map) so the export matches
 *  what's on screen — light themes, custom themes, etc.
 *  Returns the job-id immediately; progress is streamed via `arbor://job-output`.
 *  A `plugin:notification` is emitted on completion (success or failure). */
export const exportGraphSvg = (
  tabId: string,
  outputPath: string,
  themeVars?: Record<string, string>,
) =>
  invoke<string>('export_graph_svg', { tabId, outputPath, themeVars });
