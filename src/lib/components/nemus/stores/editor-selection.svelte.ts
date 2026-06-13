/**
 * nemus editor selection ranges — the other half of the editor↔DAW link.
 *
 * {@link symbolHighlightStore} lights up whole lanes for the symbol under the
 * caret; this store carries the *explicit* selection so the arrangement can box
 * the individual haps whose source span overlaps it. When you select a note, a
 * literal, or a whole block, the matching timeline regions are outlined.
 *
 * It holds an **array** of ranges so a single selection can resolve to several
 * source regions. The common case is one range (the selected text). But selecting
 * a **variable name** also adds its `let` value range — the haps a variable
 * produces carry the spans of the note literals inside its definition, not the use
 * site, so boxing them needs the definition's range too.
 *
 * Offsets are **UTF-16 document offsets** (CodeMirror-native). The arrangement
 * converts each hap's UTF-8 byte span to UTF-16 (via `makeByteToU16`) to test
 * overlap. Empty array = no box. Window-local UI state, rune-store pattern.
 */

export interface SelectionRange {
  /** Start offset (UTF-16, inclusive). */
  from: number;
  /** End offset (UTF-16, exclusive). */
  to: number;
}

function createEditorSelectionStore() {
  let ranges = $state<SelectionRange[]>([]);

  return {
    /** The source regions the current selection maps to (empty when none). */
    get ranges() { return ranges; },
    /** The literally-selected text range (the first region), or null. What
     *  "play / load the selection" acts on, vs the full {@link ranges} the DAW
     *  highlight boxes. */
    get primary() { return ranges[0] ?? null; },
    /** Whether any region is highlighted. */
    get active() { return ranges.length > 0; },
    /** True when `[start, end)` (UTF-16) overlaps any highlighted region. */
    overlaps(start: number, end: number): boolean {
      for (const r of ranges) if (start < r.to && end > r.from) return true;
      return false;
    },

    /** Publish the regions the selection maps to (drops empty/degenerate ones). */
    set(next: SelectionRange[]) {
      ranges = next.filter((r) => r.to > r.from);
    },
    /** Clear the selection (editor blurred / torn down). */
    clear() { ranges = []; },
  };
}

export const editorSelectionStore = createEditorSelectionStore();
