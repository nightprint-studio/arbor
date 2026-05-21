/**
 * Light-weight post-checkout state refresh.
 *
 * After a `git checkout` the graph topology (commits + edges + lanes) is
 * unchanged — only the HEAD marker moves and the sidebar lists may be
 * slightly different (new tracking branch on first checkout, dropped stash
 * after a pop, etc.).  The full `graphStore.refresh()` would re-run
 * `getGraph` (gitk-style lane assignment over the entire history) which is
 * the most expensive single IPC in the app on big repos.
 *
 * This helper does the minimum work needed to bring the UI in sync:
 *   * Invalidates the cached snapshot so subsequent reads are fresh.
 *   * Pulls fresh `status` + sidebar lists in a single round-trip.
 *   * Slots them into the stores Sidebar.svelte renders from (mirrors the
 *     loop inside Sidebar.svelte's $effect — kept here so we don't have to
 *     bump a global tick that would also drag CommitGraph into a reload).
 *   * Mutates the in-memory graph nodes to move `is_head` to the new HEAD
 *     commit, in place.  No backend graph fetch.
 *
 * Use after: `checkoutBranchSafe`, `checkoutBranch`, `checkoutCommit` and
 * any other operation that moves HEAD without changing the commit list.
 * For operations that CREATE refs (e.g. `checkoutRemoteAsLocal` which can
 * spawn a brand-new local tracking branch that needs to appear on graph
 * nodes' `refs`) prefer a full `graphStore.refresh()` instead.
 */

import { getStatus } from '$lib/ipc/stage';
import { cacheStore } from '$lib/stores/cache.svelte';
import { graphStore } from '$lib/stores/graph.svelte';
import { repoStore } from '$lib/stores/repo.svelte';
import { tabsStore } from '$lib/stores/tabs.svelte';
import { worktreeStore } from '$lib/stores/worktree.svelte';
import { localTagTracker } from '$lib/stores/local-tags.svelte';

export async function applyPostCheckout(tabId: string): Promise<void> {
  // Drop the stale snapshot so the next loadSidebarData() hits the backend.
  cacheStore.invalidate(tabId);

  const [sidebar, status] = await Promise.all([
    cacheStore.loadSidebarData(tabId),
    getStatus(tabId),
  ]);

  // Bail if the user switched tab while the IPC was in flight — writing
  // these stores would clobber the now-active tab's view.
  if (tabsStore.activeTabId !== tabId) return;

  repoStore.setLocalBranches(sidebar.localBranches);
  repoStore.setRemoteBranches(sidebar.remoteBranches);
  repoStore.setStashes(sidebar.stashes);
  repoStore.setStatus(status);
  repoStore.setSubmodules(sidebar.submodules);
  repoStore.setTags(sidebar.tags);
  repoStore.setNearestTag(sidebar.nearestTag);
  tabsStore.updateTab(tabId, { status });
  worktreeStore.load(tabId);
  localTagTracker.load(tabId).catch(() => { /* non-critical */ });

  // Move the HEAD marker on the existing graph nodes.  No-op when the new
  // HEAD oid is outside the loaded page (paginated graph) — the user can
  // still trigger a manual refresh in that edge case.
  if (status.head_oid) graphStore.applyHeadMove(status.head_oid);
}
