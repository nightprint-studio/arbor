/**
 * nemus "symbol under caret" highlight — links the editor to the arrangement.
 *
 * When the caret rests on an identifier (a `let` / `fn` / `import` name, or a
 * `$splice` reference), the editor highlights every occurrence in the buffer AND
 * publishes here the set of arrangement **track indices** whose pattern references
 * that name (computed from the live Tree-sitter tree). {@link ArrangementView}
 * tints those lanes, so you can see at a glance which tracks a phrase feeds.
 *
 * Index identity matches the mixer / arrangement convention (declaration order of
 * the `track(...)` calls). Window-local UI state, rune-store pattern.
 */

function createSymbolHighlightStore() {
  let name   = $state<string | null>(null);
  let tracks = $state<number[]>([]);

  return {
    /** The symbol currently under the caret (null when not on a name). */
    get name()   { return name; },
    /** Track indices whose pattern references the symbol. */
    get tracks() { return tracks; },
    /** Whether any lane should be tinted. */
    get active() { return tracks.length > 0; },
    /** True if `track` (index) references the highlighted symbol. */
    has(track: number) { return tracks.includes(track); },

    /** Publish the symbol + the tracks that reference it. */
    set(nextName: string | null, nextTracks: number[]) {
      name = nextName;
      tracks = nextTracks;
    },
    /** Clear the highlight (caret left a name, or the editor was torn down). */
    clear() { name = null; tracks = []; },
  };
}

export const symbolHighlightStore = createSymbolHighlightStore();
