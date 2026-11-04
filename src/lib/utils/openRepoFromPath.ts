/**
 * Take Corvus to a repository given its **local filesystem path** — as opposed to `openRepoFromUrl`, which
 * resolves a remote git URL.
 *
 * Every "go to Corvus" from another product (Bennu's status bar and changes panel, the File Explorer, a
 * recent project, a link) arrives here through the open-intent (`openProjectIn('corvus', path, context)`),
 * after the window manager has brought Corvus forward — as a window or as the container's tab, whichever is
 * open. What this decides is *which repository tab* the user ends up on:
 *
 *  1. not a git repository → an error that says so;
 *  2. already a tab → that tab;
 *  3. registered → open it in the active workspace if that contains it, else switch to the FIRST workspace
 *     that does (Corvus saves the workspace it leaves and restores the other's tabs), else open it without
 *     adding it to any;
 *  4. not registered → ask: add it to a workspace (the active one by default) or just open it. Nothing is
 *     added to a workspace silently.
 *
 * The rules are `planOpen` in `repo-target.ts`. Then `context` (a commit, a branch) is applied.
 */

import { openRepo } from '$lib/ipc/corvus/graph';
import { listLocalBranches } from '$lib/ipc/corvus/branch';
import { validateRepoPath } from '$lib/ipc/corvus/missing';
import type { OpenContext } from '$lib/ipc/recents';
import { graphStore } from '$lib/stores/corvus/graph.svelte';
import { openRepoPrompt } from '$lib/stores/corvus/openRepoPrompt.svelte';
import { tabsStore } from '$lib/stores/corvus/tabs.svelte';
import { workspacesStore } from '$lib/stores/corvus/workspaces.svelte';
import { planOpen, type AddChoice } from '$lib/utils/repo-target';

const baseName = (p: string) => p.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || p;

/** Open a tab for a registered repository (no workspace membership is touched). */
async function openTab(path: string, repoId: string): Promise<void> {
  tabsStore.addTab(await openRepo(path, repoId));
}

/** The tab for `path` now, if there is one. */
const tabFor = (path: string) => planOpen(path, tabsStore.tabs, [], [], null);

async function carryOut(path: string, choice: AddChoice): Promise<void> {
  // Registered, but in no workspace yet: the registry knows it, no membership is created.
  const repoId = await workspacesStore.registerPathTransient(path);
  if (choice.kind === 'add') {
    await workspacesStore.addRepoTo(choice.workspaceId, repoId);
    if (choice.workspaceId !== workspacesStore.activeId) await workspacesStore.setActive(choice.workspaceId);
  }
  const again = tabFor(path);
  if (again.kind === 'activate') tabsStore.setActive(again.tabId);
  else await openTab(path, repoId);
}

/** Land on the commit (or the branch's tip) the request names. Best effort: the repository is already open. */
async function applyContext(path: string, context: OpenContext | undefined): Promise<void> {
  if (!context || (!context.commit && !context.branch)) return;
  const tab = tabFor(path);
  const tabId = tab.kind === 'activate' ? tab.tabId : null;
  if (!tabId) return;
  let oid = context.commit ?? null;
  if (!oid && context.branch) {
    const branches = await listLocalBranches(tabId).catch(() => []);
    oid = branches.find((b) => b.name === context.branch)?.head_oid ?? null;
  }
  if (!oid) return;
  await graphStore.awaitGraphLoaded(tabId).catch(() => {});
  graphStore.selectCommit(oid);
  graphStore.scrollToCommit(oid);
}

/** Activate the repo's tab, or register + open one (see the rules above). */
export async function openRepoFromPath(path: string, context?: OpenContext): Promise<void> {
  const clean = path.replace(/[\\/]+$/, '');
  const verdict = await validateRepoPath(clean);
  if (verdict.status !== 'ok') {
    throw new Error(verdict.status === 'not_a_repo'
      ? `“${clean}” is not a git repository.`
      : verdict.message || `“${clean}” cannot be opened (${verdict.status}).`);
  }

  const plan = planOpen(clean, tabsStore.tabs, workspacesStore.registry, workspacesStore.workspaces, workspacesStore.activeId);
  switch (plan.kind) {
    case 'activate':
      tabsStore.setActive(plan.tabId);
      break;
    case 'open': {
      if (plan.switchWorkspace && plan.workspaceId) await workspacesStore.setActive(plan.workspaceId);
      // Switching restores that workspace's tabs: the repository may already be one of them.
      const again = tabFor(clean);
      if (again.kind === 'activate') tabsStore.setActive(again.tabId);
      else await openTab(clean, plan.repoId);
      break;
    }
    case 'ask': {
      const choice = await openRepoPrompt.ask({ path: clean, name: baseName(clean) });
      if (!choice) return;
      await carryOut(clean, choice);
      break;
    }
  }
  await applyContext(clean, context);
}
