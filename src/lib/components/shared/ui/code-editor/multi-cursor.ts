/**
 * Several cursors in one editor.
 *
 * CodeMirror carries the whole mechanism — typing, deleting, moving and pasting act on every range
 * and land as ONE transaction, so one undo — but only once `allowMultipleSelections` is on, and it
 * is off by default: a selection with more than one range is quietly collapsed to its main range.
 * That is why `selectNextOccurrence` (Alt+J) was bound and did nothing visible.
 *
 * ## How a second cursor is made
 *
 * * **Alt+click** adds a caret. CodeMirror's own default is Ctrl/Cmd+click, which is go-to-definition
 *   here.
 * * **Alt+Shift+drag** selects a column (a rectangle of one range per line).
 * * **Alt+J** adds the next occurrence of the selected text; **Ctrl/Cmd+Shift+L** selects them all.
 * * **Esc** drops back to the main cursor (`simplifySelection`, already in the default keymap).
 *
 * ## What stands down while there is more than one
 *
 * The completion popup and Emmet. Both compute their answer from the main cursor — which word is
 * being completed, which abbreviation sits to its left — and apply it there, so with three cursors
 * they would complete one place and leave the others as typed. A proposal that lands in a third of
 * the places is worse than none. Ghost text already declined (it checks the range count itself), and
 * everything that edits per range (typing, auto-close, wrapping a selection) needs no help.
 */

import { EditorState, type Extension } from '@codemirror/state';
import { EditorView, rectangularSelection, type KeyBinding } from '@codemirror/view';
import { selectSelectionMatches } from '@codemirror/search';
import type { CompletionContext, CompletionSource } from '@codemirror/autocomplete';

/** Whether the editor currently has more than one cursor or selection range. */
export function hasMultipleCursors(state: EditorState): boolean {
  return state.selection.ranges.length > 1;
}

/** Let the selection hold several ranges, and say how they are made (see the module notes). */
export function multiCursor(): Extension {
  return [
    EditorState.allowMultipleSelections.of(true),
    EditorView.clickAddsSelectionRange.of((e) => e.altKey && !e.shiftKey && !e.ctrlKey && !e.metaKey),
    rectangularSelection({ eventFilter: (e) => e.altKey && e.shiftKey && e.button === 0 }),
  ];
}

/** The keys of the multi-cursor set that `intellijEditingKeymap` does not already carry. */
export function multiCursorKeymap(): KeyBinding[] {
  return [
    // Every occurrence of the selected text at once — VS Code's key for it. IntelliJ's own is
    // Ctrl+Alt+Shift+J, which is a Ctrl+Alt+letter chord and does not survive an Italian keyboard.
    { key: 'Mod-Shift-l', run: selectSelectionMatches, preventDefault: true },
  ];
}

/** A completion source that answers nothing while the editor has several cursors. */
export function quietWithMultipleCursors(source: CompletionSource): CompletionSource {
  return (context: CompletionContext) => (hasMultipleCursors(context.state) ? null : source(context));
}
