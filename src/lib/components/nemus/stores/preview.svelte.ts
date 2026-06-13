/**
 * nemus instrument-preview store — the instrument shown in the docked
 * **Preview** bottom panel.
 *
 * Set from the Sound bank (the per-instrument preview button) or the editor
 * (Ctrl/Cmd+click on an instrument name); both also reveal the Preview panel.
 * When opened by bare name (the editor path) the metadata is resolved against the
 * live `soundsStore`, falling back to a minimal stub so an unknown name still
 * previews (the engine decodes / falls back). Rune-store pattern (factory +
 * getters); window-local UI state.
 */

import type { NemusInstrument } from '$lib/ipc/nemus';
import { soundsStore } from './sounds.svelte';
import { nemusStore } from '../nemus-store.svelte';

function createPreviewStore() {
  let inst = $state<NemusInstrument | null>(null);

  return {
    /** The instrument currently loaded in the Preview panel (null = none yet). */
    get inst() { return inst; },

    /** Load a resolved instrument into the Preview panel and reveal it (the
     *  Sound-bank path). */
    show(next: NemusInstrument) {
      inst = next;
      nemusStore.showBottom('preview');
    },

    /** Load by registry name (the editor Ctrl-click path): resolve the metadata
     *  from the live registry, or fall back to a minimal stub so an unknown /
     *  not-yet-loaded name still previews. Reveals the Preview panel. */
    showByName(name: string) {
      const found = soundsStore.instruments.find((i) => i.name === name);
      inst = found ?? {
        name,
        kind: 'synth',
        articulations: [],
        description: '',
        pack: null,
        pack_name: null,
      };
      nemusStore.showBottom('preview');
    },
  };
}

export const previewStore = createPreviewStore();
