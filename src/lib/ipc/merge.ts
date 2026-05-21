import { invoke } from '@tauri-apps/api/core';
import type { ConflictContent, ConflictPresence } from '$lib/types/git';
import { invalidateTabCache } from './cache-invalidate';

/**
 * Return the three-way content (ours / theirs / base / working) for a
 * conflicted file.
 *
 * `encodingOverride` (e.g. "windows-1252") skips auto-detection and forces
 * the chosen encoding. Pass `undefined` to let the backend detect.
 */
export function getConflictContent(
  tabId: string, path: string, encodingOverride?: string,
): Promise<ConflictContent> {
  return invoke<ConflictContent>('get_conflict_content', {
    tabId, path, encodingOverride,
  });
}

/**
 * Write resolved content to disk and stage the file (for merge conflicts).
 *
 * `encoding` is the label returned by `getConflictContent` (e.g. "UTF-8",
 * "windows-1252"). Pass it back so legacy non-UTF-8 files round-trip
 * through their original byte representation. Omit (or pass `undefined`)
 * for plain UTF-8.
 */
export const resolveConflict = async (
  tabId: string, path: string, content: string, encoding?: string,
): Promise<void> => {
  await invoke<void>('resolve_conflict', { tabId, path, content, encoding });
  invalidateTabCache(tabId);
};

/** Write resolved content to disk and reset the index to HEAD (unstaged, for stash conflicts). */
export const resolveStashConflict = async (
  tabId: string, path: string, content: string, encoding?: string,
): Promise<void> => {
  await invoke<void>('resolve_stash_conflict', { tabId, path, content, encoding });
  invalidateTabCache(tabId);
};

/**
 * Resolve a conflict by removing the file outright (workdir + every index
 * stage). Used by the modify/delete and add/modify UI when the user picks
 * "accept deletion".
 */
export const removeConflictFile = async (
  tabId: string, path: string,
): Promise<void> => {
  await invoke<void>('remove_conflict_file', { tabId, path });
  invalidateTabCache(tabId);
};

/** Create the merge commit with the given message. Returns the new commit OID. */
export const completeMerge = async (tabId: string, message: string): Promise<string> => {
  const oid = await invoke<string>('complete_merge', { tabId, message });
  invalidateTabCache(tabId);
  return oid;
};

/** Abort the merge — equivalent to `git merge --abort`. */
export const abortMerge = async (tabId: string): Promise<void> => {
  await invoke<void>('abort_merge', { tabId });
  invalidateTabCache(tabId);
};

/** Read the pre-filled merge commit message from `.git/MERGE_MSG`. */
export function getMergeMessage(tabId: string): Promise<string> {
  return invoke<string>('get_merge_message', { tabId });
}

/**
 * Lightweight per-file presence info for every conflicted entry in the
 * index — drives the "added by them" / "deleted by them" badges in the
 * sidebar without loading every file's three-way content up front.
 */
export function getConflictPresence(tabId: string): Promise<ConflictPresence[]> {
  return invoke<ConflictPresence[]>('get_conflict_presence', { tabId });
}
