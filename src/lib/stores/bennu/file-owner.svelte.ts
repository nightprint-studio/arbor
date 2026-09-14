/**
 * Which indexed project owns the file on screen — and, when none does, which project to open.
 *
 * A Java file outside every indexed project gets syntax checks only, and without saying so that looks
 * exactly like a file whose semantics were checked and found clean. The status bar reads this store
 * to show **Not indexed**, with the enclosing reactor root as the one-step fix.
 *
 * Re-asked when the file changes and whenever the caller's `generation` does (the workspace's roots,
 * the index starting or finishing) — an answer is only as current as the set of open projects.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import { fileOwner, type FileOwnership } from '$lib/ipc/bennu/project-health';
import { projectStore } from './project.svelte';

function baseName(path: string): string {
  return path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || path;
}

function createFileOwnerStore() {
  let file = $state<string | null>(null);
  let ownership = $state<FileOwnership | null>(null);
  /** `file` + generation the current answer was asked for, so a repeat is free and a late reply for
   *  an older question is dropped. */
  let askedKey = '';

  const notIndexed = $derived(!!file && !!ownership && !ownership.owner);
  const suggestedRoot = $derived(notIndexed ? ownership?.suggested_root ?? null : null);
  const suggestionIsMember = $derived(
    !!suggestedRoot && projectStore.workspaceRoots.includes(suggestedRoot),
  );

  return {
    /** The file on screen belongs to no indexed project. */
    get notIndexed() { return notIndexed; },
    /** The reactor root enclosing it, when there is one to open. */
    get suggestedRoot() { return suggestedRoot; },
    /** Display name of {@link suggestedRoot}. */
    get suggestedName() { return suggestedRoot ? baseName(suggestedRoot) : null; },
    /** The suggestion is already a workspace member — switching to it (which builds its index) is
     *  the fix, not opening it again. */
    get suggestionIsMember() { return suggestionIsMember; },

    /** Ask about `path` (a Java file, or `null` to clear). Never throws. */
    async load(path: string | null, generation: string) {
      const key = `${path ?? ''}|${generation}`;
      if (key === askedKey) return;
      askedKey = key;
      if (path !== file) ownership = null;
      file = path;
      if (!path) return;
      try {
        const answer = await fileOwner(path);
        if (askedKey === key) ownership = answer;
      } catch {
        if (askedKey === key) ownership = null;
      }
    },

    /** Open (or switch to) the suggested project — the status bar's and the palette's action. */
    async openSuggested() {
      const root = suggestedRoot;
      if (!root) return;
      if (suggestionIsMember) await projectStore.switchProject(root);
      else await projectStore.addProject(root);
    },

    reset() {
      file = null;
      ownership = null;
      askedKey = '';
    },
  };
}

export const fileOwnerStore = createFileOwnerStore();
