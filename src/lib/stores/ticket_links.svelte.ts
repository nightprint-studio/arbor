/**
 * ticket_links.svelte.ts
 *
 * Frontend cache + reactive store for commit ↔ ticket associations.
 *
 * Design:
 *  - `links` is a Map<sha, TicketLink[]> that accumulates across scrolls.
 *  - `fetchLinks(tabId, commits)` sends only the SHAs not yet in the map
 *    to the backend, then merges the results in.
 *  - After add/remove, the affected SHA is invalidated so the next fetch
 *    re-reads from the backend.
 */

import { tabsStore } from '$lib/stores/tabs.svelte';
import {
  getCommitTicketLinks,
  addTicketLink   as ipcAdd,
  removeTicketLink as ipcRemove,
  getTicketLinkConfig,
  checkNotesPushConfig,
} from '$lib/ipc/ticket_links';
import type { CommitQueryItem, TicketLink, TicketLinkConfig } from '$lib/types/git';

// ── localStorage key for the global enabled toggle ───────────────────────────
const ENABLED_KEY = 'arbor:ticket-links-enabled';

function createTicketLinksStore() {
  // ── Reactive state ────────────────────────────────────────────────────────

  /** SHA → links.  Grows as the user scrolls through the graph. */
  let links = $state<Map<string, TicketLink[]>>(new Map());
  /** SHAs that have been sent to (and answered by) the backend this session. */
  let fetched = $state<Set<string>>(new Set());
  /** Effective config for the current tab. */
  let config  = $state<TicketLinkConfig | null>(null);

  // ── Helpers ───────────────────────────────────────────────────────────────

  function isEnabled(): boolean {
    try { return (localStorage.getItem(ENABLED_KEY) ?? 'true') === 'true'; }
    catch { return true; }
  }

  /** Reset when the active tab changes. */
  function clearForTab() {
    links   = new Map();
    fetched = new Set();
    config  = null;
  }

  // ── Public API ────────────────────────────────────────────────────────────

  /**
   * Fetch links for the given visible commits, skipping those already cached.
   * No-ops when the feature is disabled or no tab is active.
   */
  async function fetchLinks(tabId: string, commits: CommitQueryItem[]) {
    if (!isEnabled() || commits.length === 0) return;

    const needed = commits.filter(c => !fetched.has(c.sha));
    if (needed.length === 0) return;

    // Mark as "in-flight" before awaiting to avoid duplicate parallel requests.
    for (const c of needed) fetched.add(c.sha);

    try {
      const result = await getCommitTicketLinks(tabId, needed);
      const next = new Map(links);
      for (const [sha, ls] of Object.entries(result)) {
        next.set(sha, ls);
      }
      links = next;
    } catch {
      // Non-fatal: revert the "in-flight" marks so they can be retried.
      for (const c of needed) fetched.delete(c.sha);
    }
  }

  /** Load (or refresh) the effective config for the current tab. */
  async function loadConfig(tabId: string) {
    try { config = await getTicketLinkConfig(tabId); }
    catch { config = null; }
  }

  /** Add a manual link.  Invalidates the cache entry for `sha`. */
  async function addLink(tabId: string, sha: string, ticketId: string, tracker: string) {
    await ipcAdd(tabId, sha, ticketId, tracker);
    invalidate(sha);
    // Re-fetch just this one commit (no refs needed for manual links)
    const tab = tabsStore.activeTab;
    if (tab) await fetchLinks(tab.id, [{ sha, message: '', refs: [] }]);
  }

  /** Remove a manual link.  Invalidates the cache entry for `sha`. */
  async function removeLink(tabId: string, sha: string, ticketId: string) {
    await ipcRemove(tabId, sha, ticketId);
    invalidate(sha);
    const tab = tabsStore.activeTab;
    if (tab) await fetchLinks(tab.id, [{ sha, message: '', refs: [] }]);
  }

  /** Return links for a single SHA (already-fetched), deduplicated by ticket_id. */
  function getLinks(sha: string): TicketLink[] {
    const raw = links.get(sha) ?? [];
    const seen = new Set<string>();
    return raw.filter(l => {
      if (seen.has(l.ticket_id)) return false;
      seen.add(l.ticket_id);
      return true;
    });
  }

  /** Check whether the git-notes push refspec is configured. */
  async function checkPushConfig(tabId: string): Promise<boolean> {
    return checkNotesPushConfig(tabId);
  }

  // ── Internal helpers ──────────────────────────────────────────────────────

  function invalidate(sha: string) {
    fetched.delete(sha);
    const next = new Map(links);
    next.delete(sha);
    links = next;
  }

  return {
    get links()   { return links; },
    get config()  { return config; },
    isEnabled,
    clearForTab,
    fetchLinks,
    loadConfig,
    addLink,
    removeLink,
    getLinks,
    checkPushConfig,
  };
}

export const ticketLinksStore = createTicketLinksStore();
