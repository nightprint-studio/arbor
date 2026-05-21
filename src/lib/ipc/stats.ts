import { invoke } from '@tauri-apps/api/core';

/** Start a background stats computation for the given tab.
 *  Returns immediately. The result is delivered as a Tauri event:
 *    - `arbor://repo-stats-ready`  { tab_id, stats: RepoStats }
 *    - `arbor://repo-stats-error`  { tab_id, error: string }
 *
 *  If the HEAD hasn't changed since the last computation the cached
 *  result is emitted synchronously (no thread spawned). */
export const computeRepoStats = (tabId: string): Promise<void> =>
  invoke<void>('compute_repo_stats', { tabId });

/** Export repository statistics to a JSON or HTML file.
 *  Returns a job-id; the export runs in the background.
 *  @param format  'json' | 'html' */
export const exportRepoStats = (tabId: string, outputPath: string, format: 'json' | 'html'): Promise<string> =>
  invoke<string>('export_repo_stats', { tabId, outputPath, format });
