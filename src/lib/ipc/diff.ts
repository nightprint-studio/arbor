import { invoke, Channel } from '@tauri-apps/api/core';
import { corvus } from './rpc';
import type { BlameLine, BlameProgress, DiffFile } from '../types/git';
import { diffStore } from '$lib/stores/diff.svelte';
import { tabsStore } from '$lib/stores/tabs.svelte';
import { encodingOverrides } from '$lib/stores/encodingOverrides.svelte';

// When "Show full file" is on, request the entire file as context. Picking
// a "large enough" context value is surprisingly delicate:
//   * libgit2's xdiff stores ctxlen as a signed `long`/`int`, so values near
//     u32::MAX underflow to -1 and emit zero context.
//   * libgit2 also decides whether to MERGE two adjacent hunks with the
//     formula `gap < 2*ctxlen + interhunk_lines + 1`. On Windows `long` is
//     32-bit, so passing i32::MAX (0x7FFFFFFF) made `2*ctxlen` overflow to
//     a negative number → the gap was never smaller than it → every change
//     range stayed in its own hunk, each duplicated with the full file as
//     context. Capping at 1M lines fits comfortably in a 32-bit signed long
//     after doubling (2_000_000 < 2^31) while still exceeding the line count
//     of any realistic source file.
const FULL_FILE_CONTEXT = 1_000_000;

const getContextLines = () => diffStore.fullFile ? FULL_FILE_CONTEXT : diffStore.contextLines;
const getDiffAlgo = () => diffStore.algorithm;

/** Per-path encoding pins, e.g. `{ "src/foo.java": "windows-1252" }`. Empty
 *  / `undefined` means "no overrides — auto-detect every file". The pill
 *  in DiffViewer / ConflictResolutionModal pins individual entries. */
export type EncodingOverrides = Record<string, string>;

/**
 * Look up the encoding-override snapshot for the repo backing `tabId`.
 * Returns `undefined` when there are no overrides so we serialise less
 * over IPC and don't trigger a backend re-decode for no reason.
 */
function overridesForTab(tabId: string): EncodingOverrides | undefined {
  const tab = tabsStore.tabs.find(t => t.id === tabId);
  if (!tab) return undefined;
  const snap = encodingOverrides.snapshotForRepo(tab.path);
  return Object.keys(snap).length === 0 ? undefined : snap;
}

export const getCommitDiff = (tabId: string, oid: string) =>
  corvus<DiffFile[]>('get_commit_diff', {
    tab_id: tabId, oid,
    context_lines: getContextLines(), diff_algo: getDiffAlgo(),
    encoding_overrides: overridesForTab(tabId),
  });

/// Metadata-only commit diff: file list + stats, no hunks. Pair with
/// `getCommitFileDiff(path)` to fetch hunks lazily when the user picks a file.
/// Designed to keep "click on a commit" snappy even with `fullFile=true` —
/// only the file the user actually opens pays the parse cost.
export const getCommitDiffMeta = (tabId: string, oid: string) =>
  corvus<DiffFile[]>('get_commit_diff_meta', {
    tab_id: tabId, oid, diff_algo: getDiffAlgo(),
  });

export const getCommitFileDiff = (tabId: string, oid: string, path: string) =>
  corvus<DiffFile>('get_commit_file_diff', {
    tab_id: tabId, oid, path,
    context_lines: getContextLines(), diff_algo: getDiffAlgo(),
    encoding_overrides: overridesForTab(tabId),
  });

/// Cumulative diff across a multi-commit selection: the net tree diff from the
/// FIRST PARENT of `baseOid` (the oldest selected commit) to `targetOid` (the
/// newest). Metadata-only — pair with `getCommitsRangeFileDiff` for lazy hunks,
/// mirroring the single-commit meta/file pair above.
export const getCommitsRangeDiffMeta = (tabId: string, baseOid: string, targetOid: string) =>
  corvus<DiffFile[]>('get_commits_range_diff_meta', {
    tab_id: tabId, base_oid: baseOid, target_oid: targetOid, diff_algo: getDiffAlgo(),
  });

export const getCommitsRangeFileDiff = (tabId: string, baseOid: string, targetOid: string, path: string) =>
  corvus<DiffFile>('get_commits_range_file_diff', {
    tab_id: tabId, base_oid: baseOid, target_oid: targetOid, path,
    context_lines: getContextLines(), diff_algo: getDiffAlgo(),
    encoding_overrides: overridesForTab(tabId),
  });

export const getWorkdirDiff = (tabId: string, staged: boolean) =>
  corvus<DiffFile[]>('get_workdir_diff', {
    tab_id: tabId, staged,
    context_lines: getContextLines(), diff_algo: getDiffAlgo(),
    encoding_overrides: overridesForTab(tabId),
  });

/// Start a streaming workdir diff.  Returns a job_id.  The backend emits:
///   arbor://diff-stream-started  { job_id, tab_id, staged, total_files, files }
///   arbor://diff-stream-file     { job_id, tab_id, index, total, file }  (per file)
///   arbor://diff-stream-done     { job_id, tab_id }
///   arbor://diff-stream-error    { job_id, tab_id, error }
export const getWorkdirDiffStream = (tabId: string, staged: boolean) =>
  corvus<string>('get_workdir_diff_stream', {
    tab_id: tabId, staged,
    context_lines: getContextLines(), diff_algo: getDiffAlgo(),
    encoding_overrides: overridesForTab(tabId),
  });

export const getFileAtCommit = (tabId: string, oid: string, path: string) => {
  const tab = tabsStore.tabs.find(t => t.id === tabId);
  const encodingOverride = tab ? encodingOverrides.get(tab.path, path) : undefined;
  return corvus<string>('get_file_at_commit', { tab_id: tabId, oid, path, encoding_override: encodingOverride });
};

export const getFileBlame = (tabId: string, path: string) =>
  corvus<BlameLine[]>('get_file_blame', { tab_id: tabId, path });

/**
 * Streaming blame. Resolves with the full line list once the history walk
 * finishes; `onProgress` fires with a determinate `done/total` tick at each
 * step so the UI can show a real progress bar on large files. Falls back to a
 * single final result (no ticks) on machines without a `git` binary.
 */
export const getFileBlameStreaming = (
  tabId: string,
  path: string,
  onProgress: (p: BlameProgress) => void,
) => {
  const onEvent = new Channel<BlameProgress>();
  onEvent.onmessage = onProgress;
  return invoke<BlameLine[]>('get_file_blame_streaming', { tabId, path, onEvent });
};

export const getBranchDiff = (tabId: string, fromRef: string, toRef: string) =>
  corvus<DiffFile[]>('get_branch_diff', {
    tab_id: tabId, from_ref: fromRef, to_ref: toRef,
    context_lines: getContextLines(), diff_algo: getDiffAlgo(),
    encoding_overrides: overridesForTab(tabId),
  });
