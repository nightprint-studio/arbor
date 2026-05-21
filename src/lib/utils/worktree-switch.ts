/**
 * Shared "switch the current tab to this worktree" flow.
 *
 * The Sidebar list and the global WorktreeInfoModal (mounted in AppShell so
 * the Command Palette can open it from anywhere) both need this — extracted
 * here so the logic lives in one place.
 */

import type { WorktreeInfo } from '$lib/types/git';
import { tabsStore } from '$lib/stores/tabs.svelte';
import { workspacesStore } from '$lib/stores/workspaces.svelte';
import { worktreeStore } from '$lib/stores/worktree.svelte';
import { uiStore } from '$lib/stores/ui.svelte';
import { openRepo, closeRepo } from '$lib/ipc/graph';

/**
 * Activate (or, when applicable, swap the current tab's context to) `wt`.
 *
 *   1. Same path as the active tab → no-op.
 *   2. Already open in another tab  → focus that tab.
 *   3. No tab at all                → fall back to the global `open-recent`
 *      event so the standard open flow creates a fresh tab.
 *   4. Otherwise                    → swap the active tab's context in-place
 *      (keeps the "1 project = 1 tab" invariant rather than spawning a new
 *      tab per worktree).
 */
export async function switchToWorktree(wt: WorktreeInfo): Promise<void> {
  const currentTab = tabsStore.activeTab;

  if (currentTab && currentTab.path === wt.path) return;

  const existing = tabsStore.tabs.find(t => t.path === wt.path);
  if (existing) { tabsStore.setActive(existing.id); return; }

  if (!currentTab) {
    document.dispatchEvent(new CustomEvent('open-recent', { detail: wt.path }));
    return;
  }

  try {
    // Register the worktree's path so it gets a stable repo_id, but do
    // NOT add it to the active workspace's member list — the user is
    // swapping context, not pinning a new repo into their workspace.
    const repoId = await workspacesStore.registerPathTransient(wt.path);
    const info   = await openRepo(wt.path, repoId);
    await closeRepo(currentTab.id).catch(() => { /* best-effort */ });
    tabsStore.replaceTab(currentTab.id, info, {
      preserveName:     true,
      isLinkedWorktree: !wt.is_main,
    });
    uiStore.addRecentRepo(wt.path);
    await workspacesStore.persistSnapshotNow();
    void worktreeStore.load(info.tab_id);
  } catch (err) {
    uiStore.showToast(`Failed to switch worktree: ${err}`, 'error');
  }
}
