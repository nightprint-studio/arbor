/**
 * Workspace-aware "open this repo" entrypoint used by the deep-link
 * dispatcher.  Given a remote git URL, resolve a registry match (if any),
 * apply the configured cross-workspace strategy, and either activate /
 * open the right tab or report that a clone is needed.
 *
 * This is the single source of truth for "I have a `arbor://…` URL and a
 * repo to find" — every deep-link action handler funnels through here
 * before doing its own follow-up (jump to commit, open MR detail, …).
 */

import { findRepoByRemoteUrl, getDeepLinkConfig } from '$lib/ipc/deep-link';
import { validateRepoPath } from '$lib/ipc/missing';
import { openRepo } from '$lib/ipc/graph';
import { tabsStore } from '$lib/stores/tabs.svelte';
import { workspacesStore } from '$lib/stores/workspaces.svelte';

export type OpenRepoOutcome =
  /** A tab for this repo is now open AND active. `repoId` == tab id. */
  | { kind: 'opened'; repoId: string }
  /** Registry has no match for this URL — caller should prompt clone.
   *  `reason` distinguishes never-seen-before from once-known-now-missing-on-disk
   *  so the modal can word the message accurately. */
  | { kind: 'needs_clone'; url: string; reason: 'unknown' | 'missing_on_disk' }
  /** Surface error path (validation IPC failed, openRepo crashed, …). */
  | { kind: 'error'; message: string };

/**
 * Resolve `url` and bring its tab to the foreground.
 *
 * Behaviour matrix:
 *   1. URL not in registry                       → `needs_clone` (unknown)
 *   2. URL in registry, path missing on disk     → `needs_clone` (missing_on_disk)
 *   3. URL in registry, repo in active workspace → activate tab here
 *      (open it first if not already a tab)
 *   4. URL in registry, repo in OTHER workspace(s) only:
 *        a. strategy = 'switch'    → switch workspace, then activate tab
 *        b. strategy = 'open_here' → add as cross-workspace tab in current ws
 *   5. URL in registry, repo in NO workspace     → register into active ws,
 *      open tab.  (Edge case: registry entry exists but every membership was
 *      removed.  We re-add to the active workspace per the user's intent.)
 */
export async function openRepoFromUrl(url: string): Promise<OpenRepoOutcome> {
  let lookup;
  try {
    lookup = await findRepoByRemoteUrl(url);
  } catch (e) {
    return { kind: 'error', message: `Lookup failed: ${e}` };
  }

  // Case 1 — never seen this URL.
  if (!lookup.repo_id || !lookup.repo_path) {
    return { kind: 'needs_clone', url, reason: 'unknown' };
  }

  // Case 2 — registry knows the path but the directory is gone.  Re-cloning
  // is the simplest user-facing recovery (the alternative — a relocate
  // dialog — is overkill for the deep-link entry point).
  try {
    const validation = await validateRepoPath(lookup.repo_path);
    if (validation.status === 'missing' || validation.status === 'unreachable' || validation.status === 'not_a_repo') {
      return { kind: 'needs_clone', url, reason: 'missing_on_disk' };
    }
  } catch {
    // Validation IPC errors are non-fatal — we'll let openRepo surface the
    // real reason if the path is genuinely broken.
  }

  const repoId   = lookup.repo_id;
  const repoPath = lookup.repo_path;

  // Case 3 — already in this workspace.
  if (lookup.in_active_workspace) {
    return await activateOrOpenLocal(repoId, repoPath);
  }

  // Case 5 — no workspace owns this repo.  The user invoked a deep-link, so
  // surface it in the workspace they're currently looking at.
  if (lookup.workspace_ids.length === 0) {
    return await activateOrOpenLocal(repoId, repoPath);
  }

  // Case 4 — repo lives in other workspaces only.  Apply the configured
  // cross-workspace strategy.
  let strategy: 'switch' | 'open_here' = 'switch';
  try {
    strategy = (await getDeepLinkConfig()).cross_workspace_strategy;
  } catch {
    // Fall through to 'switch' default.
  }

  if (strategy === 'switch') {
    const targetWs = lookup.workspace_ids[0]!;
    try {
      await workspacesStore.setActive(targetWs);
    } catch (e) {
      return { kind: 'error', message: `Workspace switch failed: ${e}` };
    }
    // After the switch, the tab might be open from the workspace's snapshot
    // — or not, if the user closed it before last quitting that workspace.
    return await activateOrOpenLocal(repoId, repoPath, { sourceWsId: targetWs });
  }

  // strategy === 'open_here' — add as cross-workspace tab.
  return await activateOrOpenLocal(repoId, repoPath, {
    crossWs: true,
    sourceWsId: lookup.workspace_ids[0]!,
  });
}

/**
 * Either activate an already-open tab for this repo, or open one.  When
 * `crossWs` is set, the new tab is registered without claiming workspace
 * membership and gets the cross-workspace dot in the UI.
 */
async function activateOrOpenLocal(
  repoId: string,
  repoPath: string,
  options: { crossWs?: boolean; sourceWsId?: string } = {},
): Promise<OpenRepoOutcome> {
  // Already open as a tab? Just activate it.
  const existing = tabsStore.tabs.find(t => t.id === repoId || t.path === repoPath);
  if (existing) {
    tabsStore.setActive(existing.id);
    return { kind: 'opened', repoId: existing.id };
  }

  try {
    // ensureRepoRegistered handles cross-WS undo + dot marker, and is a
    // no-op for an already-registered repo (no duplicate registry entry).
    const id = await workspacesStore.ensureRepoRegistered(
      repoPath,
      null,
      null,
      options.crossWs && options.sourceWsId
        ? { allowCrossWs: true, sourceWsId: options.sourceWsId }
        : {},
    );
    const info = await openRepo(repoPath, id);
    tabsStore.addTab(info);
    return { kind: 'opened', repoId: id };
  } catch (e) {
    return { kind: 'error', message: `Failed to open repo: ${e}` };
  }
}
