import { invoke } from '@tauri-apps/api/core';
import type { RemoteInfo, FetchResult, PullResult, SearchQuery, SearchResult } from '../types/git';
import { invalidateTabCache } from './cache-invalidate';

// ── Read-only ─────────────────────────────────────────────────────────────────

export const listRemotes = (tabId: string) =>
  invoke<RemoteInfo[]>('list_remotes', { tabId });

export const searchCommits = (tabId: string, query: SearchQuery) =>
  invoke<SearchResult[]>('search_commits', { tabId, query });

/** Open the repository (or a specific commit/branch/tag) in the default browser.
 *  target: "repo" | "commit:{oid}" | "branch:{name}" | "tag:{name}" */
export const openInBrowser = (tabId: string, target: string) =>
  invoke<void>('open_in_browser', { tabId, target });

// ── Writes (invalidate cache on success) ─────────────────────────────────────

export const fetchRemote = async (tabId: string, remote = 'origin'): Promise<FetchResult> => {
  // NOTE: do not call invalidateTabCache() here. Callers (StatusBar,
  // CommandPalette, MrModal) drive the refresh through
  // cacheStore.refreshIfChanged(), which compares the repo fingerprint
  // against the cached snapshot's fingerprint to decide whether to reload.
  // Invalidating the cache first would drop snap.fingerprint, making
  // refreshIfChanged see an undefined baseline and skip the refresh even
  // when the fetch actually brought in new commits on non-current branches.
  return invoke<FetchResult>('fetch_remote', { tabId, remote });
};

export const pushBranch = async (tabId: string, remote: string, refspec: string, force = false): Promise<void> => {
  await invoke<void>('push_branch', { tabId, remote, refspec, force });
  invalidateTabCache(tabId);
};

/** Pull from `remote` with optional progress reporting in the OperationsOverlay.
 *
 *  When `opId` is provided the backend emits `arbor://pull-progress` (per-phase)
 *  and `arbor://pull-done` events keyed by that id — the frontend bridge
 *  translates them into the floating progress card.  Generate the id with
 *  `nanoid` / `crypto.randomUUID()` BEFORE calling `operationsStore.start(...)`
 *  so the card is mounted by the time the first event arrives. */
export const pullBranch = async (
  tabId:  string,
  remote: string  = 'origin',
  opId?:  string,
): Promise<PullResult> => {
  const result = await invoke<PullResult>('pull_branch', { tabId, remote, opId });
  invalidateTabCache(tabId);
  return result;
};
