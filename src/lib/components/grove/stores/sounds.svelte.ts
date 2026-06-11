/**
 * grove sounds store — the resolvable instrument list (registry introspection
 * via `grove_sounds`). Reflects what the engine can *actually* play: the
 * built-in synth presets are always present; the VSCO 2 samplers appear once
 * the bank is installed. Not a static list — it tracks the live registry.
 *
 * Refresh on mount and whenever the registry can change (a VSCO install
 * completing). Imports only the IPC seam — grove stays extractable.
 */

import { groveSounds, type GroveInstrument } from '$lib/ipc/grove';

function createSoundsStore() {
  let instruments = $state<GroveInstrument[]>([]);
  let loading = $state(false);
  let loaded  = $state(false);

  return {
    get instruments() { return instruments; },
    get loading()     { return loading; },
    get loaded()      { return loaded; },
    /** Built-in synth presets. */
    get synths()   { return instruments.filter((i) => i.kind === 'synth'); },
    /** Sample/SFZ voices (the VSCO bank + any manifest entries). */
    get samplers() { return instruments.filter((i) => i.kind !== 'synth'); },

    /** Re-read the registry. Keeps the last list on failure (engine not ready). */
    async refresh() {
      loading = true;
      try {
        const list = await groveSounds();
        instruments = list.instruments;
        loaded = true;
      } catch {
        // Engine/registry not ready — keep what we have; the next call retries.
      } finally {
        loading = false;
      }
    },
  };
}

export const soundsStore = createSoundsStore();
