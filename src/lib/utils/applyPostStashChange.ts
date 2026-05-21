/**
 * Light-weight post-stash refresh.
 *
 * Mirrors `applyPostCheckout` but for the stash domain.  Stash operations
 * (save / drop / pop / apply) leave the commit graph topology untouched —
 * only the stash refs change, plus the working-directory state shifts:
 *
 *   * `stash save`  → workdir becomes clean (or partially), one stash added
 *   * `stash drop`  → one stash removed
 *   * `stash apply` → workdir gains the stashed changes (or conflicts)
 *   * `stash pop`   → workdir gains the stashed changes + that stash drops
 *
 * In every case `getGraph` (gitk lane assignment) would be wasted work.
 * This helper pulls just the three pieces that actually move:
 *   * `stash refs` for the in-graph dashed bubble markers
 *   * `stash entries` for the sidebar list
 *   * fresh `status` for the WIP node + dirty/clean indicators
 *
 * and slots them straight into the stores Sidebar.svelte / CommitGraph render
 * from.  No `graphStore.refresh()`, no `loadGraph` round-trip.
 */

import { listStashes, listGraphStashRefs } from '$lib/ipc/branch';
import { getStatus } from '$lib/ipc/stage';
import { graphStore } from '$lib/stores/graph.svelte';
import { repoStore } from '$lib/stores/repo.svelte';
import { tabsStore } from '$lib/stores/tabs.svelte';
import { cacheStore } from '$lib/stores/cache.svelte';

export async function applyPostStashChange(tabId: string): Promise<void> {
  // Drop the cached sidebar snapshot — it embeds the stash list, which is
  // about to differ.  We re-populate `repoStore` directly below so the UI
  // updates instantly; the invalidate just keeps a future loadSidebarData
  // (e.g. on tab switch back) from serving the stale snapshot.
  cacheStore.invalidate(tabId);

  const [graphRefs, entries, status] = await Promise.all([
    listGraphStashRefs(tabId),
    listStashes(tabId),
    getStatus(tabId),
  ]);

  if (tabsStore.activeTabId !== tabId) return;

  graphStore.setStashes(graphRefs);
  repoStore.setStashes(entries);
  repoStore.setStatus(status);
  tabsStore.updateTab(tabId, { status });
}
