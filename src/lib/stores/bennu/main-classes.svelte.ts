/**
 * The project's entry points — every class declaring `public static void main(String[])`,
 * and which of them are `@SpringBootApplication`s.
 *
 * Three unrelated places want this: the run-configuration editor's class picker, ▷ deciding
 * what to run when nothing is configured, and a Spring Boot launch resolving its own class.
 * Each of them used to ask the backend directly, which meant a scan of the project's sources
 * per modal opening and per button press — enough, on a large tree, to hold a backend thread
 * while the editor's own per-keystroke requests queued behind it.
 *
 * So: **fetched once per project, shared by all three**. Concurrent callers join the same
 * in-flight promise rather than starting a second scan; the result is kept until the project
 * changes or someone asks for a refresh. The backend caches it too — this layer is what stops
 * the round-trip and the re-render, that one is what stops the walk.
 *
 * Rune store — private `$state`, returned getters + methods (CLAUDE.md · Store pattern).
 */

import { SvelteMap } from 'svelte/reactivity';
import { mainClasses as ipcMainClasses } from '$lib/ipc/bennu';
import type { MainClassEntry } from '$lib/types/bennu';

function createMainClassStore() {
  const byRoot = new SvelteMap<string, MainClassEntry[]>();
  // Which roots are being read right now — drives the picker's "looking…" state.
  const loading = new SvelteMap<string, boolean>();
  // The read in flight per root, so a burst of callers costs one round-trip.
  const inflight = new Map<string, Promise<MainClassEntry[]>>();

  return {
    /** The entry points known for `root` — `[]` until {@link load} resolves. Reactive. */
    forRoot(root: string): MainClassEntry[] {
      return byRoot.get(root) ?? [];
    },

    /** Whether `root` has been read at least once (so "none found" can be told from
     *  "not looked yet", which are very different things to show). */
    isLoaded(root: string): boolean {
      return byRoot.has(root);
    },

    /** Whether a read is in flight for `root`. */
    isLoading(root: string): boolean {
      return loading.get(root) === true;
    },

    /**
     * Read `root`'s entry points, once. A repeat call returns what is already held; a call
     * while one is in flight joins it. `force` re-reads (the backend rescans too).
     *
     * Never rejects: a project that cannot be scanned has no entry points as far as every
     * caller here is concerned, and all three of them have a sensible empty behaviour.
     */
    async load(root: string, force = false): Promise<MainClassEntry[]> {
      if (!root) return [];
      if (!force && byRoot.has(root)) return byRoot.get(root) ?? [];
      const pending = inflight.get(root);
      if (pending) return pending;

      loading.set(root, true);
      const p = ipcMainClasses(root)
        .then((found) => {
          byRoot.set(root, found);
          return found;
        })
        .catch(() => {
          byRoot.set(root, []);
          return [] as MainClassEntry[];
        })
        .finally(() => {
          loading.set(root, false);
          inflight.delete(root);
        });
      inflight.set(root, p);
      return p;
    },

    /** Forget `root`'s entry points (a project closing, or a rescan about to happen). */
    invalidate(root: string) {
      byRoot.delete(root);
      inflight.delete(root);
    },
  };
}

export const bennuMainClassStore = createMainClassStore();
