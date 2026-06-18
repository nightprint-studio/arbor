import { invoke } from '@tauri-apps/api/core';
import { corvus } from './rpc';
import type { GraphData, CommitDetail, RepoInfo, InitRepoOptions, InitRepoResult, CloneOptions, RepoFileEntry } from '../types/git';

export const openRepo = (path: string, tabId: string) =>
  corvus<RepoInfo>('open_repo', { path, tab_id: tabId });

/** Returns true when `path` is inside a git repository. */
export const checkIsGitRepo = (path: string) =>
  corvus<boolean>('check_is_git_repo', { path });

/** Read user.name / user.email from the global git config. */
export const getGitIdentity = () =>
  corvus<[string, string]>('get_git_identity');

/** Initialise a new git repository with the given options. */
export const initRepo = (path: string, tabId: string, options: InitRepoOptions) =>
  invoke<InitRepoResult>('init_repo', { path, tabId, options });

export const closeRepo = (tabId: string) =>
  corvus<void>('close_repo', { tab_id: tabId });

/** List branch names available on a remote URL without cloning. */
export const listRemoteBranchesForUrl = (url: string) =>
  corvus<string[]>('list_remote_branches_for_url', { url });

/** Clone a remote repository to disk and return the fresh repo's metadata.
 *  Does not open a tab (the returned `tab_id` is empty) — the caller opens it
 *  via {@link openRepo} keyed by the workspace-registry id. */
export const cloneRepo = (opts: CloneOptions) =>
  corvus<RepoInfo>('clone_repo', { opts });

export const getRepoInfo = (tabId: string) =>
  corvus<RepoInfo>('get_repo_info', { tab_id: tabId });

export const getGraph = (tabId: string, offset = 0, limit = 500) =>
  corvus<GraphData>('get_graph', { tab_id: tabId, offset, limit });

export const getGraphForFile = (tabId: string, filePath: string, offset = 0, limit = 500) =>
  corvus<GraphData>('get_graph_for_file', { tab_id: tabId, file_path: filePath, offset, limit });

export const getCommitDetail = (tabId: string, oid: string) =>
  corvus<CommitDetail>('get_commit_detail', { tab_id: tabId, oid });

export const getRepoFileTree = (tabId: string) =>
  corvus<RepoFileEntry[]>('get_repo_file_tree', { tab_id: tabId });

/** Fast: returns all tracked file paths from the index, no commit walking. */
export const getRepoFiles = (tabId: string) =>
  corvus<string[]>('get_repo_files', { tab_id: tabId });

/** Lazy: returns the last commit that touched each path in the given list. */
export const getFilesLastCommit = (tabId: string, paths: string[]) =>
  corvus<RepoFileEntry[]>('get_files_last_commit', { tab_id: tabId, paths });

/** Starts a background scan that emits:
 *  - `arbor://file-meta-batch` {tab_id, entries[]} progressively
 *  - `arbor://file-meta-done`  {tab_id} when complete */
export const startFileMetaScan = (tabId: string) =>
  corvus<void>('start_file_meta_scan', { tab_id: tabId });

/** Returns a fast fingerprint of the repo's current ref state (HEAD SHA + all refs).
 *  Used by the cache scheduler to detect remote changes without loading the full graph. */
export const getRepoFingerprint = (tabId: string) =>
  corvus<string>('get_repo_fingerprint', { tab_id: tabId });

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
  corvus<string>('export_graph_svg', { tab_id: tabId, output_path: outputPath, theme_vars: themeVars });
