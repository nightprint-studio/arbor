/**
 * Typing an opening delimiter over a selection **wraps** it instead of replacing it.
 *
 * CodeMirror's `closeBrackets` already does this, but only for the pairs it also auto-closes — and
 * for some delimiters those are two different decisions. `<` is the case that forced it: wrapping a
 * selected `String` into `<String>` is always what was meant, while auto-closing a bare `<` is wrong
 * half the time, because the same character starts a type argument list and a comparison. So this
 * is the wrap on its own, with no opinion on an empty selection: a caret with nothing selected
 * falls through to whatever else handles the key.
 *
 * All-or-nothing across a multi-selection: if any range is empty the key is left to the other
 * handlers, so a mixed selection never ends up half wrapped and half typed.
 */

import { EditorSelection, type EditorState, type Extension, type Transaction } from '@codemirror/state';
import { EditorView } from '@codemirror/view';

/** Opening delimiter → closing delimiter. The same character on both sides is fine (`"` → `"`). */
export type DelimiterPairs = Readonly<Record<string, string>>;

/** The transaction that wraps every selected range in `open`…`close`, keeping the wrapped text
 *  selected; `null` when some range is empty (there is nothing to wrap). */
export function wrapSelectionWith(state: EditorState, open: string, close: string): Transaction | null {
  if (state.selection.ranges.some((range) => range.empty)) return null;
  const spec = state.changeByRange((range) => ({
    changes: [{ from: range.from, insert: open }, { from: range.to, insert: close }],
    range: EditorSelection.range(range.anchor + open.length, range.head + open.length),
  }));
  return state.update(spec, { scrollIntoView: true, userEvent: 'input.type' });
}

/** An input handler that wraps a non-empty selection when one of `pairs`' openers is typed. */
export function wrapSelectionOnType(pairs: DelimiterPairs): Extension {
  return EditorView.inputHandler.of((view, from, to, insert) => {
    if (!Object.prototype.hasOwnProperty.call(pairs, insert)) return false;
    if (view.composing || view.state.readOnly) return false;
    // A replacement the browser reports for some other range is not a keystroke over the selection.
    const main = view.state.selection.main;
    if (from !== main.from || to !== main.to) return false;
    const tr = wrapSelectionWith(view.state, insert, pairs[insert]);
    if (!tr) return false;
    view.dispatch(tr);
    return true;
  });
}
