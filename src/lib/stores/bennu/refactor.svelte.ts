/**
 * Bennu refactor store — drives the two caret-based refactor surfaces:
 *   • **Rename** (Shift+F6): the editor captures the caret context (file, current
 *     buffer, byte offset, the identifier under the caret) and opens the rename modal
 *     off it. The modal previews via `bennu_rename_plan` and applies on confirm.
 *   • **Find usages** (Alt+F7): a caret-anchored popover listing the resolved use
 *     sites from `bennu_references`; picking one opens the file + jumps to the line.
 *
 * Window-local session UI state. Rune-store pattern: private `$state`, returned
 * getters + methods (CLAUDE.md · "Store pattern").
 */

import type { UsageHit } from '$lib/ipc/bennu/nav';

/** The caret context a rename is requested against (the buffer is classified at the
 *  BE against `source`/`offset`; `initialName` seeds the modal input). */
export interface RenameRequest {
  /** Absolute path (forward slashes) of the file the caret is in. */
  file: string;
  /** Current (possibly-unsaved) buffer text. */
  source: string;
  /** Caret UTF-8 byte offset. */
  offset: number;
  /** The identifier under the caret (pre-fills the new-name field). */
  initialName: string;
  /**
   * A name to pre-fill instead of `initialName`, when the caller already knows what the rename
   * should produce — the naming-convention fix computes it, so the field opens on the answer and
   * the user only has to confirm. `initialName` stays what the symbol is *called*, which is what
   * the header shows and what "unchanged" is measured against.
   */
  suggestedName?: string;
}

/** Caret-anchored popover position, viewport coords. */
export interface UsagesAnchor {
  x: number;
  y: number;
}

function createBennuRefactorStore() {
  // ── Rename ──────────────────────────────────────────────────────────────────
  let renameOpen = $state(false);
  let renameReq = $state<RenameRequest | null>(null);

  // ── Find usages ─────────────────────────────────────────────────────────────
  let usagesOpen = $state(false);
  let usagesAnchor = $state<UsagesAnchor | null>(null);
  let usagesLoading = $state(false);
  let usagesLabel = $state<string | null>(null);
  let usagesHits = $state<UsageHit[]>([]);
  // The identifier under the caret when usages was invoked (for the empty-state copy).
  let usagesSymbol = $state<string | null>(null);

  return {
    get renameOpen() { return renameOpen; },
    get renameReq() { return renameReq; },

    get usagesOpen() { return usagesOpen; },
    get usagesAnchor() { return usagesAnchor; },
    get usagesLoading() { return usagesLoading; },
    get usagesLabel() { return usagesLabel; },
    get usagesHits() { return usagesHits; },
    get usagesSymbol() { return usagesSymbol; },

    /** Open the rename modal for a caret context. */
    openRename(req: RenameRequest) {
      renameReq = req;
      renameOpen = true;
    },
    closeRename() {
      renameOpen = false;
      renameReq = null;
    },

    /** Open the usages popover in a loading state at `anchor`, for `symbol`. */
    startUsages(anchor: UsagesAnchor | null, symbol: string | null) {
      usagesAnchor = anchor;
      usagesSymbol = symbol;
      usagesLabel = null;
      usagesHits = [];
      usagesLoading = true;
      usagesOpen = true;
    },
    /** Fill the popover with results (or an empty list). */
    setUsages(label: string | null, hits: UsageHit[]) {
      usagesLabel = label;
      usagesHits = hits;
      usagesLoading = false;
    },
    closeUsages() {
      usagesOpen = false;
    },
  };
}

export const bennuRefactorStore = createBennuRefactorStore();
