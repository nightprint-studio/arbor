import { corvus } from '../rpc';
import type { RepoStatus, CherryPickResult, StatusEntry } from '../../types/corvus/git';
import { invalidateTabCache } from './cache-invalidate';

// ── Read-only ─────────────────────────────────────────────────────────────────

export const getStatus = (tabId: string) =>
  corvus<RepoStatus>('get_status', { tab_id: tabId });

export const getGitCommitTemplate = (tabId: string) =>
  corvus<string | null>('get_git_commit_template', { tab_id: tabId });

// ── Writes (invalidate cache on success) ─────────────────────────────────────

export const stageAll = async (tabId: string): Promise<void> => {
  await corvus<void>('stage_all', { tab_id: tabId });
  invalidateTabCache(tabId);
};

export const unstageAll = async (tabId: string): Promise<void> => {
  await corvus<void>('unstage_all', { tab_id: tabId });
  invalidateTabCache(tabId);
};

export const discardAll = async (tabId: string): Promise<void> => {
  await corvus<void>('discard_all', { tab_id: tabId });
  invalidateTabCache(tabId);
};

// ── Path lists (ATOMIC — one index write / checkout for the whole group) ─────────────
// There is no single-file variant: a file is a list of length one. Two verbs for one concept
// is how "Stage File" and "Stage Folder" drifted apart in the first place, and fanning out N
// single-file RPCs would race on `.git/index` anyway — each opens its own repo handle and
// rewrites the WHOLE index, so the last writer wins and only a subset of the folder lands.
//
// A **rename is two paths**: git stages a move as the removal of the old path plus the
// addition of the new one. Callers send both halves (`StatusEntry.old_path` alongside
// `path`) — send one and the other side of the move is silently left behind.

/** Every path a status entry occupies — one for an ordinary change, **two for a
 *  rename**. Build the `paths` argument of the three calls below with this, never
 *  with `entry.path` alone: a move is a removal plus an addition, and passing half
 *  of it stages half of it. */
export const pathsOf = (entry: Pick<StatusEntry, 'path' | 'old_path'>): string[] =>
  entry.old_path && entry.old_path !== entry.path ? [entry.path, entry.old_path] : [entry.path];

export const stagePaths = async (tabId: string, paths: string[]): Promise<void> => {
  await corvus<void>('stage_paths', { tab_id: tabId, paths });
  invalidateTabCache(tabId);
};

export const unstagePaths = async (tabId: string, paths: string[]): Promise<void> => {
  await corvus<void>('unstage_paths', { tab_id: tabId, paths });
  invalidateTabCache(tabId);
};

export const discardPaths = async (tabId: string, paths: string[]): Promise<void> => {
  await corvus<void>('discard_paths', { tab_id: tabId, paths });
  invalidateTabCache(tabId);
};

export const stagePatch = async (tabId: string, patch: string): Promise<void> => {
  await corvus<void>('stage_patch', { tab_id: tabId, patch });
  invalidateTabCache(tabId);
};

// `commit` fires the vetoable `on_pre_commit` hook + `on_commit`; the broker
// handler fires them inline (the plugin host is co-located with the handler),
// so a plugin veto comes back as a rejected promise.
export const commitChanges = async (tabId: string, message: string, amend = false): Promise<string> => {
  const oid = await corvus<string>('commit', { tab_id: tabId, message, amend });
  invalidateTabCache(tabId);
  return oid;
};

export const cherryPick = async (tabId: string, oid: string): Promise<CherryPickResult> => {
  const result = await corvus<CherryPickResult>('cherry_pick', { tab_id: tabId, oid });
  invalidateTabCache(tabId);
  return result;
};

export const revertCommit = async (tabId: string, oid: string): Promise<CherryPickResult> => {
  const result = await corvus<CherryPickResult>('revert_commit', { tab_id: tabId, oid });
  invalidateTabCache(tabId);
  return result;
};
