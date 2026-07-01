import { corvus } from '$lib/ipc/rpc';
import type { MergeRequest, MrDetail, CreateMrParams, MrFileDiff, MrCommit, MergedMrHint, MrCapabilities, MrFeatureStatus } from '$lib/types/corvus/mr';
import { invalidateTabCache } from './cache-invalidate';

export function listMrs(
  tabId:       string,
  stateFilter: 'open' | 'closed' | 'merged' | 'all' = 'open',
): Promise<MergeRequest[]> {
  return corvus('list_mrs', { tab_id: tabId, state_filter: stateFilter });
}

export function getMrDetail(tabId: string, number: number): Promise<MrDetail> {
  return corvus('get_mr_detail', { tab_id: tabId, number });
}

/** Probe per-repo capabilities (currently only auto-merge support).
 *  Never rejects — falls back to permissive defaults on backend errors. */
export function getMrCapabilities(tabId: string): Promise<MrCapabilities> {
  return corvus('get_mr_capabilities', { tab_id: tabId });
}

/** Probe whether the active repo accepts pull/merge requests at all.
 *  Permissive on failure (returns `{ enabled: true, reason: null }`) so
 *  the user can still try when the probe itself errors. */
export function probeMrFeature(tabId: string): Promise<MrFeatureStatus> {
  return corvus('probe_mr_feature', { tab_id: tabId });
}

export async function createMr(tabId: string, params: CreateMrParams): Promise<MergeRequest> {
  const r = await corvus<MergeRequest>('create_mr', { tab_id: tabId, params });
  invalidateTabCache(tabId);
  return r;
}

export async function mergeMr(
  tabId:        string,
  number:       number,
  opts: {
    mergeMethod?:  'merge' | 'squash' | 'rebase';
    squash?:       boolean;
    deleteBranch?: boolean;
    sourceBranch?: string;
  } = {},
): Promise<void> {
  await corvus('merge_mr', {
    tab_id:        tabId,
    number,
    merge_method:  opts.mergeMethod,
    squash:        opts.squash,
    delete_branch: opts.deleteBranch,
    source_branch: opts.sourceBranch,
  });
  invalidateTabCache(tabId);
}

export async function closeMr(tabId: string, number: number): Promise<void> {
  await corvus('close_mr', { tab_id: tabId, number });
  invalidateTabCache(tabId);
}

export async function reopenMr(tabId: string, number: number): Promise<void> {
  await corvus('reopen_mr', { tab_id: tabId, number });
  invalidateTabCache(tabId);
}

export async function markMrReady(tabId: string, number: number): Promise<void> {
  await corvus('mark_mr_ready', { tab_id: tabId, number });
  invalidateTabCache(tabId);
}

/** Cancel an armed auto-merge / merge-when-pipeline-succeeds.
 *  Idempotent — resolves quietly when auto-merge wasn't actually armed. */
export async function disableMrAutoMerge(tabId: string, number: number): Promise<void> {
  await corvus('disable_mr_auto_merge', { tab_id: tabId, number });
  invalidateTabCache(tabId);
}

export async function addMrComment(tabId: string, number: number, body: string): Promise<void> {
  await corvus('add_mr_comment', { tab_id: tabId, number, body });
  invalidateTabCache(tabId);
}

export function getMrFiles(tabId: string, number: number): Promise<MrFileDiff[]> {
  return corvus('get_mr_files', { tab_id: tabId, number });
}

export function getMrCommits(tabId: string, number: number): Promise<MrCommit[]> {
  return corvus('get_mr_commits', { tab_id: tabId, number });
}

export function getCommitDiff(tabId: string, sha: string): Promise<MrFileDiff[]> {
  return corvus('get_mr_commit_diff', { tab_id: tabId, sha });
}

/** Returns merge-commit SHA hints for all merged PRs/MRs.
 *  Never rejects — returns [] if no provider / token is configured. */
export function getMergedMrHints(tabId: string): Promise<MergedMrHint[]> {
  return corvus('get_merged_mr_hints', { tab_id: tabId });
}

/** Phase identifiers emitted by `arbor://mr-conflict-progress`. */
export type MrPrepPhase = 'status' | 'fetch' | 'checkout' | 'merge';

export interface MrConflictProgress {
  job_id:       string;
  phase:        MrPrepPhase;
  phase_index:  number;
  phase_total:  number;
  label:        string;
  detail?:      string | null;
}

export interface MrConflictDone {
  job_id:  string;
  /** `clean`     — merge fast-forwarded / no conflicts; user just needs to push.
   *  `conflicts` — merge produced conflicts; open the resolver modal.
   *  `error`     — prep failed (dirty workdir, fetch error, etc.). */
  status:  'clean' | 'conflicts' | 'error';
  error?:  string | null;
}

/** Start the MR conflict-resolution prep flow as a background job.
 *
 *  Returns the `job_id` immediately; the actual flow runs on a worker thread
 *  and reports progress via two Tauri events:
 *  - `arbor://mr-conflict-progress`  → {@link MrConflictProgress}
 *  - `arbor://mr-conflict-done`      → {@link MrConflictDone}
 *
 *  The job is also visible in the JobsOverlay / Job Output panel.
 *
 *  Note: cache invalidation is the caller's responsibility — invoke
 *  `invalidateTabCache(tabId)` only AFTER the `mr-conflict-done` event arrives,
 *  since the working tree / branch state hasn't been touched yet at the time
 *  this promise resolves. */
export function mrStartConflictResolution(
  tabId:        string,
  sourceBranch: string,
  targetBranch: string,
): Promise<string> {
  return corvus<string>('mr_start_conflict_resolution', {
    tab_id:        tabId,
    source_branch: sourceBranch,
    target_branch: targetBranch,
  });
}
