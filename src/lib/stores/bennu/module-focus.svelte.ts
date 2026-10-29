/**
 * Which module the tool windows act on.
 *
 * The Maven, Cargo, Dependencies and Tests windows each show the modules of their ecosystem across
 * the WHOLE workspace, and their commands (build, test, run, outdated, reload…) need one folder to
 * run in. That choice lives here so the four windows and the command palette agree on it:
 *
 * 1. the node the user selected in a module tree, else
 * 2. the module containing the active file, else
 * 3. the first module of the project on screen, else
 * 4. the first module anywhere.
 *
 * (The rule itself is `resolveTarget`, pure and tested.) With a single module the selection is
 * never needed and no selector is drawn — the answer is always that module.
 *
 * Session state only: a selection is a pointer into the current set of projects, not a preference.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md · Store pattern).
 */

import { projectStore } from './project.svelte';
import { resolveTarget, type ModuleRef } from '$lib/components/bennu/project-modules';
import type { ModuleKind } from '$lib/ipc/bennu/modules';

function createModuleFocusStore() {
  let selectedId = $state<string | null>(null);

  return {
    /** The node picked in a module tree, or `null` while the active file decides. */
    get selectedId() { return selectedId; },

    select(id: string | null) { selectedId = id; },

    /** Pick `id`, or let go of it when it is already the pick (back to "follow the active file"). */
    toggle(id: string) { selectedId = selectedId === id ? null : id; },

    /**
     * The module a command of the given ecosystems acts on, or `undefined` when the workspace has
     * none. Reads reactive state, so call it from a `$derived` / `$effect` to follow the workspace.
     */
    targetFor(...kinds: ModuleKind[]): ModuleRef | undefined {
      return resolveTarget({
        refs: projectStore.modulesOf(...kinds),
        selectedId,
        activeFile: projectStore.activeFilePath,
        activeRoot: projectStore.project?.root,
      });
    },
  };
}

export const moduleFocusStore = createModuleFocusStore();
