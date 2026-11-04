/**
 * The display hints a game sent with `#[derive(RemoteInspect)]`: which fields are colours, sliders,
 * angles, dates. Fetched once per connection (and again after a restart); a game without the companion
 * plugin simply has none, and every value is drawn from its JSON alone.
 */

import { brpInspectHints } from '$lib/ipc/bennu/brp';
import type { HintMap } from '$lib/components/bennu/brp/brp-widgets';

function createBrpHintsStore() {
  let hints = $state<HintMap>({});
  let loadedFor = -1;

  /** `generation` identifies the connection; asking again for the same one is free. */
  async function load(generation: number) {
    if (loadedFor === generation) return;
    loadedFor = generation;
    try {
      hints = await brpInspectHints();
    } catch {
      hints = {};
    }
  }

  return {
    get hints() { return hints; },
    load,
    clear() { hints = {}; loadedFor = -1; },
  };
}

export const brpHintsStore = createBrpHintsStore();
