/**
 * Which editor the keyboard is talking to.
 *
 * Several window-level commands are *about the document in front*: find and
 * replace, go to a table's structure, replace a match. The shell owns the
 * keystrokes — that is where a window-wide binding has to live — but only the view
 * that mounted the editor has the handle, and there is more than one such view
 * (a query tab, a script file).
 *
 * So each view registers its editor under its tab id, and this store answers "the
 * one the user is looking at". Registration rather than prop-drilling because the
 * shell is three levels above the editor and passing a handle up through those
 * levels would put an editor-shaped hole in every component in between.
 *
 * ## The handle is structural
 *
 * A named interface rather than the component's own type: what the shell needs is
 * six methods, and binding to `CodeEditor` as a whole would make every command in
 * this window depend on the shared widget's entire surface.
 */

import { untrack } from 'svelte';

import { picusTabsStore } from './tabs.svelte';

/** What a window-level command may ask of the editor in front. */
export interface PicusEditorHandle {
  focus: () => void;
  /** CodeMirror's own find-and-replace panel. */
  openSearch: () => void;
  getValue: () => string;
  selectionRange: () => { from: number; to: number; head: number; empty: boolean };
  selectRange: (from: number, to: number) => void;
  /** UTF-8 offsets — everything the backend reports is in bytes. */
  selectByteRange: (startByte: number, endByte: number) => void;
  replaceByteRange: (startByte: number, endByte: number, text: string) => void;
  /** Several ranges as one edit — one undo step, and no shifting offsets. */
  replaceByteRanges: (
    edits: readonly { startByte: number; endByte: number; text: string }[],
  ) => number;
  /** The identifier under the caret, for go-to-definition. */
  wordAtCaret: () => string | null;
}

function createEditorStore() {
  let handles = $state.raw<Record<string, PicusEditorHandle>>({});

  return {
    /**
     * The editor of the active tab, or `null`.
     *
     * Keyed on the tab rather than on focus: a command fired from the toolbar or
     * from the palette has taken focus away from the editor by the time it runs,
     * and "the editor of the tab in front" is what the user means in every case.
     */
    get active(): PicusEditorHandle | null {
      const id = picusTabsStore.active?.id;
      return id ? handles[id] ?? null : null;
    },

    /**
     * A view mounted an editor. Pass `null` on unmount.
     *
     * The whole body is untracked, and that is load-bearing rather than tidy: this
     * is called **from an effect** — the one in the view that owns the editor — and
     * it both reads the map (to copy it) and writes it. Reading a signal inside an
     * effect makes it a dependency of that effect, so writing it in the same breath
     * is an effect that re-triggers itself for ever.
     *
     * Untracking here rather than at the call sites because there are two of them
     * and there will be more; a rule that every caller has to remember is a rule
     * that gets forgotten. The write still notifies everyone reading {@link active},
     * which is the point of it.
     */
    bind(tabId: string, handle: PicusEditorHandle | null) {
      untrack(() => {
        if (!handle) {
          if (!(tabId in handles)) return;
          const { [tabId]: _gone, ...rest } = handles;
          handles = rest;
          return;
        }
        handles = { ...handles, [tabId]: handle };
      });
    },
  };
}

export const picusEditorStore = createEditorStore();
