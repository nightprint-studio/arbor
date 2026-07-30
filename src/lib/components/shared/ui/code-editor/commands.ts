/**
 * Editor commands CodeMirror does not ship, and that an IDE user misses within the
 * first minute.
 *
 * Arbor's editors are aimed at people who are IntelliJ-comfortable, so the gap that
 * matters is not "what does CodeMirror bind" but "what does a hand trained on an IDE
 * reach for and find nothing". `defaultKeymap` already covers moving a line
 * (`Alt+↑/↓`), deleting one (`Ctrl+Y`, added by the host) and folding. What it has
 * no command for at all is **duplicating** — which is the single most-used editing
 * verb after copy and paste.
 */

import { EditorSelection, type ChangeSpec } from '@codemirror/state';
import type { Command } from '@codemirror/view';

/**
 * `Ctrl+D` — duplicate the selection, or the whole line when there is none.
 *
 * Two behaviours in one key because they are the same intention at two scales, and
 * every IDE that has this key does both:
 *
 *  * **With a selection** the copy goes immediately after it and *becomes* the
 *    selection, so a second press duplicates again and typing replaces the copy —
 *    which is what makes it useful for building a list of similar values.
 *  * **Without one** the line is copied below and the caret keeps its column on the
 *    new line, so `Ctrl+D` then editing is one gesture.
 *
 * Multiple cursors are handled by construction: every range contributes its own
 * insertion, and the ranges are mapped through the combined change rather than
 * recomputed — offsets after the first insertion have all moved.
 */
export const duplicateSelection: Command = (view) => {
  const { state } = view;
  const changes: ChangeSpec[] = [];
  /** Where each range's new selection should land, in pre-change coordinates. */
  const anchors: { from: number; to: number }[] = [];

  for (const range of state.selection.ranges) {
    if (range.empty) {
      const line = state.doc.lineAt(range.head);
      // The newline goes in FRONT of the copy, at the end of the line: appending it
      // behind instead would put the caret on a fresh empty line at the bottom of
      // the document when the line duplicated is the last one.
      changes.push({ from: line.to, insert: `\n${line.text}` });
      const column = range.head - line.from;
      anchors.push({ from: line.to + 1 + column, to: line.to + 1 + column });
    } else {
      const text = state.doc.sliceString(range.from, range.to);
      changes.push({ from: range.to, insert: text });
      anchors.push({ from: range.to, to: range.to + text.length });
    }
  }

  if (!changes.length) return false;
  const transaction = state.update({ changes, scrollIntoView: true, userEvent: 'input.duplicate' });
  // Mapped through the transaction: with several cursors, every position after the
  // first insertion has shifted by the length of the ones before it.
  const selection = EditorSelection.create(
    anchors.map((a) =>
      EditorSelection.range(
        transaction.changes.mapPos(a.from, 1),
        transaction.changes.mapPos(a.to, 1),
      ),
    ),
  );
  view.dispatch(transaction, { selection });
  return true;
};
