/**
 * grove DSL reference store — the canonical language catalogue (`grove_lang_reference`).
 *
 * Loaded once per window (the catalogue is static): every combinator, generator,
 * signal, transform, mini-notation operator, keyword and note form, authored in
 * Rust (`arbor-grove-lang`). This single source feeds the editor's autocomplete,
 * hover docs, and the Docs panel — so language intelligence never drifts from the
 * evaluator. Imports only the IPC seam; grove stays extractable.
 */

import { groveLangReference, type GroveDslEntry } from '$lib/ipc/grove';

function createReferenceStore() {
  let entries = $state<GroveDslEntry[]>([]);
  let loaded = $state(false);
  let loading = $state(false);
  /** Name → entry, for hover / completion lookups. Built lazily from `entries`. */
  let byNameMap = $state<Map<string, GroveDslEntry>>(new Map());

  return {
    get entries() { return entries; },
    get loaded() { return loaded; },
    get loading() { return loading; },

    /** Resolve a name to its (first) catalogue entry, or `undefined`. Aliases
     *  that share a name across kinds (e.g. `par`) resolve to the first listed. */
    byName(name: string): GroveDslEntry | undefined {
      return byNameMap.get(name);
    },

    /** Load the catalogue once (idempotent). No-op once loaded; keeps the last
     *  list on failure so the editor still works (just without hints). */
    async load() {
      if (loaded || loading) return;
      loading = true;
      try {
        const list = await groveLangReference();
        entries = list;
        // First-wins map: aliases (`par` combinator vs seq-method) keep the
        // combinator entry, which is what hover-on-an-identifier wants.
        const m = new Map<string, GroveDslEntry>();
        for (const e of list) if (!m.has(e.name)) m.set(e.name, e);
        byNameMap = m;
        loaded = true;
      } catch {
        // Backend not ready — keep what we have; a later call can retry.
      } finally {
        loading = false;
      }
    },
  };
}

export const referenceStore = createReferenceStore();
