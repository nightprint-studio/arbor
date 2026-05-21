import { invoke } from '@tauri-apps/api/core';
import type { BlameLine, DiffFile } from '../types/git';
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

const getContextLines = () => {
  if (diffStore.fullFile) return FULL_FILE_CONTEXT;
  return parseInt(localStorage.getItem('arbor:context-lines') ?? '3');
};
const getDiffAlgo = () => localStorage.getItem('arbor:diff-algo') ?? 'myers';

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
  invoke<DiffFile[]>('get_commit_diff', {
    tabId, oid,
    contextLines: getContextLines(), diffAlgo: getDiffAlgo(),
    encodingOverrides: overridesForTab(tabId),
  });

/// Metadata-only commit diff: file list + stats, no hunks. Pair with
/// `getCommitFileDiff(path)` to fetch hunks lazily when the user picks a file.
/// Designed to keep "click on a commit" snappy even with `fullFile=true` —
/// only the file the user actually opens pays the parse cost.
export const getCommitDiffMeta = (tabId: string, oid: string) =>
  invoke<DiffFile[]>('get_commit_diff_meta', {
    tabId, oid, diffAlgo: getDiffAlgo(),
  });

export const getCommitFileDiff = (tabId: string, oid: string, path: string) =>
  invoke<DiffFile>('get_commit_file_diff', {
    tabId, oid, path,
    contextLines: getContextLines(), diffAlgo: getDiffAlgo(),
    encodingOverrides: overridesForTab(tabId),
  });

export const getWorkdirDiff = (tabId: string, staged: boolean) =>
  invoke<DiffFile[]>('get_workdir_diff', {
    tabId, staged,
    contextLines: getContextLines(), diffAlgo: getDiffAlgo(),
    encodingOverrides: overridesForTab(tabId),
  });

/// Start a streaming workdir diff.  Returns a job_id.  The backend emits:
///   arbor://diff-stream-started  { job_id, tab_id, staged, total_files, files }
///   arbor://diff-stream-file     { job_id, tab_id, index, total, file }  (per file)
///   arbor://diff-stream-done     { job_id, tab_id }
///   arbor://diff-stream-error    { job_id, tab_id, error }
export const getWorkdirDiffStream = (tabId: string, staged: boolean) =>
  invoke<string>('get_workdir_diff_stream', {
    tabId, staged,
    contextLines: getContextLines(), diffAlgo: getDiffAlgo(),
    encodingOverrides: overridesForTab(tabId),
  });

export const getFileAtCommit = (tabId: string, oid: string, path: string) => {
  const tab = tabsStore.tabs.find(t => t.id === tabId);
  const encodingOverride = tab ? encodingOverrides.get(tab.path, path) : undefined;
  return invoke<string>('get_file_at_commit', { tabId, oid, path, encodingOverride });
};

export const getFileBlame = (tabId: string, path: string) =>
  invoke<BlameLine[]>('get_file_blame', { tabId, path });

export const getBranchDiff = (tabId: string, fromRef: string, toRef: string) =>
  invoke<DiffFile[]>('get_branch_diff', {
    tabId, fromRef, toRef,
    contextLines: getContextLines(), diffAlgo: getDiffAlgo(),
    encodingOverrides: overridesForTab(tabId),
  });
