/**
 * merula scale catalogue store — the `.scale("root:mode")` modes, loaded once from
 * the backend (`merula_scales`, sourced from `merula-pattern`'s authoritative
 * table). Drives the editor's scale-aware quick-fixes (snap-to-scale, change-
 * scale). Static; a single load on window mount. Rune-store pattern.
 */

import { merulaScales, type MerulaScaleMode } from '$lib/ipc/merula/merula';

function createScalesStore() {
  let modes = $state<MerulaScaleMode[]>([]);
  let loaded = false;

  return {
    /** The mode catalogue (empty until loaded). */
    get modes() { return modes; },

    /** Fetch the catalogue once (idempotent; a failure leaves it retryable). */
    async load() {
      if (loaded) return;
      loaded = true;
      try { modes = await merulaScales(); } catch { loaded = false; }
    },
  };
}

export const scalesStore = createScalesStore();
