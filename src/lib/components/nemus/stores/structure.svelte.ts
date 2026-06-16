/**
 * nemus "file structure" store — drives the floating structure popover (Ctrl+F12).
 *
 * Populated by the editor (Ctrl+F12 / Command Palette → `TabbedEditor.openStructure`):
 * the editor extracts the symbol table (tracks · fn · let · import) from its live
 * Tree-sitter tree and opens a filterable picker centred near the top. Each item is
 * a `NemusSymbol` carrying the UTF-16 offset (exact jump via `nemusStore.requestGoto`)
 * and a 1-based line. Window-local UI state, rune-store pattern (factory + getters).
 */

import type { NemusSymbol } from '../editor/nemus-lang';

function createStructureStore() {
  let open  = $state(false);
  let items = $state<NemusSymbol[]>([]);

  return {
    /** Whether the floating picker is showing. */
    get open()  { return open; },
    /** The file's symbols, in source order (the picker filters them live). */
    get items() { return items; },

    /** Open the picker with a fresh symbol set. */
    openWith(next: NemusSymbol[]) { items = next; open = true; },
    /** Close the picker. */
    close() { open = false; },
  };
}

export const structureStore = createStructureStore();
