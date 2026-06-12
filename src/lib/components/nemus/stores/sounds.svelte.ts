/**
 * nemus sounds store — the resolvable instrument list (registry introspection
 * via `nemus_sounds`). Reflects what the engine can *actually* play: the
 * built-in synth presets are always present; sampler voices appear once a sample
 * pack is installed. Not a static list — it tracks the live registry.
 *
 * Refresh on mount and whenever the registry can change (a pack install
 * completing). Imports only the IPC seam — nemus stays extractable.
 */

import { nemusSounds, type NemusInstrument } from '$lib/ipc/nemus';

function createSoundsStore() {
  let instruments = $state<NemusInstrument[]>([]);
  let loading = $state(false);
  let loaded  = $state(false);

  return {
    get instruments() { return instruments; },
    get loading()     { return loading; },
    get loaded()      { return loaded; },
    /** Built-in synth presets. */
    get synths()   { return instruments.filter((i) => i.kind === 'synth'); },
    /** Sample/SFZ voices from any installed sample pack. */
    get samplers() { return instruments.filter((i) => i.kind !== 'synth'); },

    /** Re-read the registry. Keeps the last list on failure (engine not ready). */
    async refresh() {
      loading = true;
      try {
        const list = await nemusSounds();
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
