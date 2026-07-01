/**
 * Open (or activate) a repo tab in the main window given a **local filesystem
 * path** — as opposed to `openRepoFromUrl`, which resolves a remote git URL.
 *
 * Used by the built-in File Explorer's "Open in Arbor" delegation: the explorer
 * (possibly in its own dedicated window) asks the backend to focus the main
 * window and emit `arbor://explorer-open-repo`; the main window's AppShell
 * listener funnels the payload here so the heavy git operations happen in
 * Arbor's full UI instead of being reimplemented in the explorer.
 */

import { openRepo } from '$lib/ipc/corvus/graph';
import { tabsStore } from '$lib/stores/corvus/tabs.svelte';
import { workspacesStore } from '$lib/stores/corvus/workspaces.svelte';

function norm(p: string): string {
  return p.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase();
}

/** Activate the existing tab for `path`, or register + open one. */
export async function openRepoFromPath(path: string): Promise<void> {
  const target = norm(path);
  const existing = tabsStore.tabs.find(t => norm(t.path) === target);
  if (existing) {
    tabsStore.setActive(existing.id);
    return;
  }
  const id = await workspacesStore.ensureRepoRegistered(path);
  const info = await openRepo(path, id);
  tabsStore.addTab(info);
}
