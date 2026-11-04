/**
 * Bennu Node — the packages and scripts of the Node project the Node tool window is on.
 *
 * One read per folder, kept until asked again: the backend reads `package.json`s only (never the
 * lockfile, never the network), so a re-read is cheap and the window offers a refresh instead of
 * watching. The dependencies themselves are the Dependencies panel's — this is the other half of
 * `package.json`, the part you *run*.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md · Store pattern).
 */

import { nodePackages, type NodeProject } from '$lib/ipc/bennu/npm';

function createNodeStore() {
  let project = $state<NodeProject | null>(null);
  let loading = $state(false);
  let error = $state('');
  let loadedRoot: string | null = null;
  const byRoot = new Map<string, NodeProject>();

  return {
    get project(): NodeProject | null { return project; },
    get loading() { return loading; },
    get error() { return error; },

    /** Read `root`. A repeat call for the same folder is a no-op unless `force`; switching folders
     *  shows the last answer for the target at once and refreshes behind it. */
    async load(root: string, force = false) {
      if (!force && loadedRoot === root && project) return;
      if (loadedRoot !== root) project = byRoot.get(root) ?? null;
      loadedRoot = root;
      loading = true;
      error = '';
      try {
        const fresh = await nodePackages(root);
        byRoot.set(root, fresh);
        if (loadedRoot === root) project = fresh;
      } catch (e) {
        if (loadedRoot === root) {
          project = null;
          error = String(e);
        }
      } finally {
        if (loadedRoot === root) loading = false;
      }
    },

    reset() {
      project = null;
      loadedRoot = null;
      loading = false;
      error = '';
    },
  };
}

export const bennuNodeStore = createNodeStore();
