import { invoke } from '@tauri-apps/api/core';
import type { BranchInfo, TagInfo, StashEntry, StashRef, StashApplyResult, StashBlockingContent, ResetMode, CheckoutResult } from '../types/git';
import { invalidateTabCache } from './cache-invalidate';
import { tabsStore } from '../stores/tabs.svelte';

// ── Read-only ─────────────────────────────────────────────────────────────────

export const listLocalBranches = (tabId: string) =>
  invoke<BranchInfo[]>('list_local_branches', { tabId });

export const listRemoteBranches = (tabId: string) =>
  invoke<BranchInfo[]>('list_remote_branches', { tabId });

export const listTags = (tabId: string) =>
  invoke<TagInfo[]>('list_tags', { tabId });

export const listMergedBranches = (tabId: string, target: string) =>
  invoke<BranchInfo[]>('list_merged_branches', { tabId, target });

export const listMergedRemoteBranches = (tabId: string, target: string) =>
  invoke<BranchInfo[]>('list_merged_remote_branches', { tabId, target });

export const listStashes = (tabId: string) =>
  invoke<StashEntry[]>('list_stashes', { tabId });

/** Same shape that `getGraph` embeds in `GraphData.stashes` — fetched
 *  standalone so post-stash refresh can repaint markers without
 *  re-running the lane assignment over the whole graph. */
export const listGraphStashRefs = (tabId: string) =>
  invoke<StashRef[]>('list_graph_stash_refs', { tabId });

export const getNearestTag = (tabId: string) =>
  invoke<string | null>('get_nearest_tag', { tabId });

// ── Writes (invalidate cache on success) ─────────────────────────────────────

/** Outcome of a clean merge — discriminates the "nothing happened" cases. */
export type MergeOutcome = 'already_up_to_date' | 'fast_forward' | 'merged' | 'squashed';

/** Strategy flag forwarded to `git merge`. */
export type MergeStrategy = 'default' | 'no_ff' | 'ff_only' | 'squash';

/** Merge `branchName` into the current HEAD. Rejects with CONFLICTS:… on conflict. */
export const mergeBranch = async (
  tabId: string,
  branchName: string,
  strategy?: MergeStrategy,
): Promise<MergeOutcome> => {
  const r = await invoke<MergeOutcome>('merge_branch', { tabId, branchName, strategy });
  invalidateTabCache(tabId);
  return r;
};

export const createBranch = async (tabId: string, name: string, fromOid: string): Promise<BranchInfo> => {
  const r = await invoke<BranchInfo>('create_branch', { tabId, name, fromOid });
  invalidateTabCache(tabId);
  return r;
};

export const deleteBranch = async (tabId: string, name: string): Promise<void> => {
  await invoke<void>('delete_branch', { tabId, name });
  invalidateTabCache(tabId);
};

/** Returns names of branches that failed to delete. */
export const deleteBranches = async (tabId: string, names: string[]): Promise<string[]> => {
  const failed = await invoke<string[]>('delete_branches', { tabId, names });
  invalidateTabCache(tabId);
  return failed;
};

/** Push-delete remote branches. Returns names that failed. Names are "remote/branch" format. */
export const deleteRemoteBranches = async (tabId: string, names: string[]): Promise<string[]> => {
  const failed = await invoke<string[]>('delete_remote_branches', { tabId, names });
  invalidateTabCache(tabId);
  return failed;
};

export interface RemoteRenameResult {
  new_full_name: string;
  local_renamed: boolean;
  local_skipped: boolean;
}

/**
 * Rename a remote branch: push old tip to new name + delete old name on remote.
 * If `renameLocal` is true and a local branch with the same short name exists,
 * it is renamed too and its upstream is re-pointed at the new remote ref.
 * `oldFullName` is the "remote/branch" form (e.g. "origin/develop").
 */
export const renameRemoteBranch = async (
  tabId: string,
  oldFullName: string,
  newShortName: string,
  renameLocal: boolean,
): Promise<RemoteRenameResult> => {
  const r = await invoke<RemoteRenameResult>('rename_remote_branch', {
    tabId, oldFullName, newShortName, renameLocal,
  });
  invalidateTabCache(tabId);
  return r;
};

export const renameBranch = async (tabId: string, oldName: string, newName: string): Promise<BranchInfo> => {
  const r = await invoke<BranchInfo>('rename_branch', { tabId, oldName, newName });
  invalidateTabCache(tabId);
  return r;
};

export const checkoutBranch = async (tabId: string, name: string): Promise<void> => {
  await invoke<void>('checkout_branch', { tabId, name });
  invalidateTabCache(tabId);
  // Keep the tab badge in sync — historic bug: the TabBar reads
  // `tab.currentBranch` which was set at open time and never updated after
  // a checkout, so the chip always lagged.
  tabsStore.updateTab(tabId, { currentBranch: name });
};

/** Stash-safe checkout: stash → checkout → stash apply. Returns conflict info if re-apply had issues. */
export const checkoutBranchSafe = async (tabId: string, name: string): Promise<CheckoutResult> => {
  const result = await invoke<CheckoutResult>('checkout_branch_safe', { tabId, name });
  invalidateTabCache(tabId);
  // Only update the tab's branch chip when the checkout actually settled
  // cleanly — on a stash-conflict the workdir is on the new branch but the
  // user is still in a recovery flow, the chip stays accurate either way.
  if (!result.stash_apply_error && result.stash_conflicts.length === 0) {
    tabsStore.updateTab(tabId, { currentBranch: name });
  }
  return result;
};

/**
 * Checkout a remote-tracking branch by creating (if needed) a local tracking
 * branch. `remoteName` is the form `origin/patch/4.14`. Returns the resolved
 * local short name (e.g. `patch/4.14`).
 */
export const checkoutRemoteAsLocal = async (tabId: string, remoteName: string): Promise<string> => {
  const localName = await invoke<string>('checkout_remote_as_local', { tabId, remoteName });
  invalidateTabCache(tabId);
  tabsStore.updateTab(tabId, { currentBranch: localName });
  return localName;
};

export const checkoutCommit = async (tabId: string, oid: string): Promise<void> => {
  await invoke<void>('checkout_commit', { tabId, oid });
  invalidateTabCache(tabId);
  // Detached HEAD: clear the branch chip.
  tabsStore.updateTab(tabId, { currentBranch: null });
};

export const stashSave = async (tabId: string, message?: string, includeUntracked = true): Promise<StashEntry> => {
  const r = await invoke<StashEntry>('stash_save', { tabId, message, includeUntracked });
  invalidateTabCache(tabId);
  return r;
};

export const stashApply = async (tabId: string, index: number): Promise<StashApplyResult> => {
  const result = await invoke<StashApplyResult>('stash_apply', { tabId, index });
  invalidateTabCache(tabId);
  return result;
};

export const stashPop = async (tabId: string, index: number): Promise<StashApplyResult> => {
  const result = await invoke<StashApplyResult>('stash_pop', { tabId, index });
  invalidateTabCache(tabId);
  return result;
};

export const forceStashApply = async (
  tabId: string,
  index: number,
  filesToDelete: string[],
  filesToKeep: string[],
  dropOnSuccess: boolean,
): Promise<StashApplyResult> => {
  const result = await invoke<StashApplyResult>('force_stash_apply', {
    tabId, index, filesToDelete, filesToKeep, dropOnSuccess,
  });
  invalidateTabCache(tabId);
  return result;
};

export const abortStashApply = async (tabId: string): Promise<void> => {
  await invoke<void>('abort_stash_apply', { tabId });
  invalidateTabCache(tabId);
};

export const writeWorkdirFile = async (
  tabId: string, path: string, content: string, encoding?: string,
): Promise<void> => {
  await invoke<void>('write_workdir_file', { tabId, path, content, encoding });
};

export const getStashFileContent = async (
  tabId: string,
  index: number,
  path: string,
  encodingOverride?: string,
): Promise<StashBlockingContent> => {
  return await invoke<StashBlockingContent>('get_stash_file_content', {
    tabId, index, path, encodingOverride,
  });
};

export const stashDrop = async (tabId: string, index: number): Promise<void> => {
  await invoke<void>('stash_drop', { tabId, index });
  invalidateTabCache(tabId);
};

export const stashRename = async (tabId: string, index: number, newMessage: string): Promise<StashEntry> => {
  const entry = await invoke<StashEntry>('stash_rename', { tabId, index, newMessage });
  invalidateTabCache(tabId);
  return entry;
};

export const resetToCommit = async (tabId: string, oid: string, mode: ResetMode): Promise<void> => {
  await invoke<void>('reset_to_commit', { tabId, oid, mode });
  invalidateTabCache(tabId);
};

export const createTag = async (tabId: string, name: string, oid: string, message?: string): Promise<void> => {
  await invoke<void>('create_tag', { tabId, name, oid, message });
  invalidateTabCache(tabId);
};

export const deleteTag = async (tabId: string, name: string): Promise<void> => {
  await invoke<void>('delete_tag', { tabId, name });
  invalidateTabCache(tabId);
};
