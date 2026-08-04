/**
 * What kind of type each `.java` file declares — class / interface / enum / record /
 * annotation — for the icon that marks it.
 *
 * A store and not a component-local map because two places need the same answer: the project
 * tree and the editor's tab strip. They were about to hold one fetch of the class index each,
 * of the same data, refreshed on the same signal — and two copies of a cache drift the moment
 * one of them forgets to invalidate.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md).
 */

import { SvelteMap } from 'svelte/reactivity';
import { bennuIndexStore } from './index.svelte';

function createJavaKindStore() {
  // Forward-slashed absolute path → kind.
  const kinds = new SvelteMap<string, string>();
  let loadedRoot: string | null = null;
  /** The read in flight, so a burst of callers costs one round-trip. */
  let inFlight: Promise<void> | null = null;

  return {
    /** The kind `path` declares. `class` when unknown — the overwhelmingly common answer,
     *  and what an un-indexed file settles to the moment the index lands. A caller wanting
     *  to distinguish "not known yet" from "a class" should ask {@link isKnown}. */
    kindOf(path: string): string {
      return kinds.get(path.replace(/\\/g, '/')) ?? 'class';
    },

    isKnown(path: string): boolean {
      return kinds.has(path.replace(/\\/g, '/'));
    },

    /** Load (or reload) the map for `root`. Repeats for the same project are a no-op unless
     *  `force` — the callers pass `force` when the index has rebuilt. */
    async load(root: string, force = false): Promise<void> {
      if (!force && loadedRoot === root && kinds.size > 0) return;
      if (inFlight) return inFlight;
      inFlight = (async () => {
        let classes;
        try {
          classes = await bennuIndexStore.classesForRoot(root);
        } catch {
          return; // index not ready — keep whatever we had rather than blanking every icon
        }
        kinds.clear();
        for (const c of classes) {
          const key = c.file.replace(/\\/g, '/');
          const stem = key.split('/').pop()?.replace(/\.java$/, '') ?? '';
          // A file may declare several types; the one NAMED like the file is the one its
          // icon should show. Otherwise keep the first seen.
          if (!kinds.has(key) || c.simple === stem) kinds.set(key, c.kind);
        }
        loadedRoot = root;
      })();
      try {
        await inFlight;
      } finally {
        inFlight = null;
      }
    },

    reset() {
      kinds.clear();
      loadedRoot = null;
      inFlight = null;
    },
  };
}

export const javaKindStore = createJavaKindStore();
