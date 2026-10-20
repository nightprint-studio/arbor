/**
 * The working-tree changes of the open Bennu project, for the Changes panel: the staged /
 * unstaged lists and the verbs that move files between them.
 *
 * Corvus does all of it, by path — the project root becomes a headless-repo tab id once, and
 * every staging call is the one Corvus's own panel uses. This store only holds the answer and
 * re-asks after each change, then tells `bennuGitStatusStore` so the tree colours, the tab
 * marks and the status bar follow.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */
import { repoTabForPath } from '$lib/ipc/corvus/changesByPath';
import {
  getStatus, stageAll, unstageAll, discardAll, stagePaths, unstagePaths, discardPaths,
  commitChanges, getGitCommitTemplate,
} from '$lib/ipc/corvus/stage';
import type { RepoStatus } from '$lib/types/corvus/git';
import type { StageTarget } from '$lib/utils/stage/stage-tree';
import { bennuGitStatusStore } from '$lib/stores/bennu/git-status.svelte';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';

function createGitChangesStore() {
  let status = $state.raw<RepoStatus | null>(null);
  let root = '';
  let tabId: string | null = null;
  let token = 0;

  async function tab(forRoot: string): Promise<string> {
    // The id is the normalised workdir, so it only changes with the project.
    if (tabId === null || root !== forRoot) {
      tabId = await repoTabForPath(forRoot);
      root = forRoot;
    }
    return tabId;
  }

  async function refresh(forRoot: string | null | undefined): Promise<void> {
    if (!forRoot) { status = null; return; }
    const mine = ++token;
    try {
      const next = await getStatus(await tab(forRoot));
      if (mine === token) status = next;
    } catch {
      if (mine === token) status = null;
    }
  }

  /** One try / refresh / toast shape for every verb. */
  async function run(op: (id: string) => Promise<unknown>, ok: string | null, failed: string) {
    if (!root) return;
    try {
      await op(await tab(root));
      await refresh(root);
      bennuGitStatusStore.refresh(root);
      if (ok) toastStore.show(ok, 'success');
    } catch (e) {
      toastStore.show(`${failed}: ${e}`, 'error');
    }
  }

  return {
    get status() { return status; },
    get root() { return root; },
    refresh,

    stage: ({ paths, label }: StageTarget) =>
      run((id) => stagePaths(id, paths), `Staged ${label}`, 'Stage failed'),
    unstage: ({ paths, label }: StageTarget) =>
      run((id) => unstagePaths(id, paths), `Unstaged ${label}`, 'Unstage failed'),
    discard: ({ paths, label }: StageTarget) =>
      run((id) => discardPaths(id, paths), `Discarded ${label}`, 'Discard failed'),
    stageAll: () => run(stageAll, null, 'Stage all failed'),
    unstageAll: () => run(unstageAll, null, 'Unstage all failed'),
    discardAll: () => run(discardAll, 'Discarded all unstaged changes', 'Discard all failed'),

    /** The commit template git is configured with, or `''`. */
    async template(): Promise<string> {
      if (!root) return '';
      try {
        const t = await getGitCommitTemplate(await tab(root));
        return t && t.trim() ? t : '';
      } catch {
        return '';
      }
    },

    /** Commit what is staged. Resolves `true` when it landed. */
    async commit(message: string, amend: boolean): Promise<boolean> {
      if (!root) return false;
      try {
        const oid = await commitChanges(await tab(root), message, amend);
        await refresh(root);
        bennuGitStatusStore.refresh(root);
        toastStore.show(`Committed ${oid.slice(0, 7)}`, 'success');
        return true;
      } catch (e) {
        toastStore.show(`Commit failed: ${e}`, 'error');
        return false;
      }
    },
  };
}

export const bennuGitChangesStore = createGitChangesStore();
