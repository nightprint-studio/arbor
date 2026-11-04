/**
 * The code generators the backend offers (`bennu-generate`'s registry), for the command palette and the
 * context menus. The registry is the backend's, so a new generator appears here without an edit — the
 * store only filters them by what the open file can take.
 */

import { listGenerators, type GeneratorListing } from '$lib/ipc/bennu/generate';

function createGeneratorsStore() {
  let list = $state<GeneratorListing[]>([]);
  let loadedFor: string | null = null;

  /** The generators that fit a Java file, a Rust file, or a folder of the project tree. */
  function offeredFor(kind: 'java' | 'rust' | 'folder' | 'none'): GeneratorListing[] {
    return list.filter((g) =>
      g.targets.some((t) => {
        if (t === 'project') return true;
        if (t === 'endpoint') return kind !== 'none';
        if (t === 'java' || t === 'class') return kind === 'java';
        return t === kind;
      }),
    );
  }

  return {
    get list() { return list; },
    offeredFor,
    /** Load once per project; the list does not change while one is open. */
    async load(root: string) {
      if (loadedFor === root) return;
      loadedFor = root;
      try {
        list = await listGenerators(root);
      } catch {
        loadedFor = null;
        list = [];
      }
    },
  };
}

export const generatorsStore = createGeneratorsStore();
