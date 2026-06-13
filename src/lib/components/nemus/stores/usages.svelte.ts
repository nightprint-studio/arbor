/**
 * nemus "find usages" store — drives the floating usages popover.
 *
 * Populated by the editor (Alt+F7 / Command Palette → `TabbedEditor.findUsages`):
 * the editor resolves the identifier under the cursor against its live Tree-sitter
 * tree and opens a floating list anchored at the caret. Each item carries the
 * UTF-16 offset (the exact jump target via `nemusStore.requestGoto`), a 1-based
 * line/col, and the trimmed line text for a readable preview. Window-local UI
 * state, rune-store pattern (factory + getters).
 */

/** One occurrence of the searched symbol. */
export interface UsageItem {
  /** UTF-16 offset of the occurrence (exact editor jump target). */
  offset: number;
  /** 1-based line / column, for the row label. */
  line: number;
  col: number;
  /** Trimmed source line, for a readable preview. */
  preview: string;
}

/** Viewport anchor (caret coordinates) the popover positions itself against. */
export interface UsageAnchor { x: number; y: number; }

function createUsagesStore() {
  let open   = $state(false);
  let symbol = $state<string | null>(null);
  let items  = $state<UsageItem[]>([]);
  let anchor = $state<UsageAnchor | null>(null);

  return {
    /** Whether the floating popover is showing. */
    get open()   { return open; },
    /** The symbol the result set is for. */
    get symbol() { return symbol; },
    get items()  { return items; },
    get count()  { return items.length; },
    /** Caret anchor for positioning the popover. */
    get anchor() { return anchor; },

    /** Open the popover with a fresh result set, anchored at the caret. A null
     *  `nextSymbol` means the caret wasn't on a name — the popover shows a hint. */
    openAt(nextSymbol: string | null, nextItems: UsageItem[], nextAnchor: UsageAnchor | null) {
      symbol = nextSymbol;
      items  = nextItems;
      anchor = nextAnchor;
      open   = true;
    },
    /** Close the popover (keeps no result set around). */
    close() { open = false; },
  };
}

export const usagesStore = createUsagesStore();
