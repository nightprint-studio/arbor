import { invoke } from '@tauri-apps/api/core';
import type { RemoteInfo, FetchResult, PullResult, SearchQuery, SearchResult } from '../../types/corvus/git';
import { invalidateTabCache } from './cache-invalidate';
import { corvus } from '../rpc';

// ── Read-only ─────────────────────────────────────────────────────────────────

export const listRemotes = (tabId: string) =>
  corvus<RemoteInfo[]>('list_remotes', { tab_id: tabId });

export const searchCommits = (tabId: string, query: SearchQuery) =>
  corvus<SearchResult[]>('search_commits', { tab_id: tabId, query });

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
  return corvus<FetchResult>('fetch_remote', { tab_id: tabId, remote });
};

export const pushBranch = async (tabId: string, remote: string, refspec: string, force = false): Promise<void> => {
  await corvus<void>('push_branch', { tab_id: tabId, remote, refspec, force });
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
  const result = await corvus<PullResult>('pull_branch', { tab_id: tabId, remote, op_id: opId ?? null });
  invalidateTabCache(tabId);
  return result;
};
