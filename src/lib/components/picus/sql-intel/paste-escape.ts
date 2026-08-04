/**
 * Pasting into a string literal escapes the quotes.
 *
 * `L'Aquila` pasted between the quotes of `nome = '|'` gives `'L'Aquila'`, which is
 * not a value with an apostrophe in it — it is a closed string, a stray word and a
 * syntax error. Every SQL editor worth using fixes that at the moment of the paste,
 * because the alternative is finding it at the moment of the run.
 *
 * This file is only the wiring. Which quote encloses the caret — the part that can
 * be wrong without anyone noticing — is {@link quoteAround}, in `quote-context.ts`,
 * where it can be run without an editor.
 */

import { EditorView } from '@codemirror/view';
import type { Extension } from '@codemirror/state';
import type { Dialect } from '$lib/types/picus';
import { quoteAround } from './quote-context';

/**
 * The CodeMirror extension. Rides with the SQL language descriptor rather than
 * living in the shared editor: escaping is a property of the language, and the
 * editor has no business knowing what a SQL string is.
 *
 * Registered through `domEventHandlers`, whose handlers CodeMirror runs **before**
 * its own — so returning `true` here is what stops the default paste from also
 * inserting the unescaped text.
 */
export function escapeQuotesOnPaste(dialect: Dialect): Extension {
  return EditorView.domEventHandlers({
    paste(event, view) {
      const text = event.clipboardData?.getData('text/plain') ?? '';
      if (!text) return false;

      // The paste lands where the selection starts, so that is the position whose
      // context decides — not the caret's head, which for a backwards selection is
      // the other end.
      const { from, to } = view.state.selection.main;
      const quote = quoteAround(view.state.doc.toString(), from, dialect);
      if (!quote || !text.includes(quote)) return false;

      const escaped = text.split(quote).join(quote + quote);
      event.preventDefault();
      view.dispatch({
        changes: { from, to, insert: escaped },
        selection: { anchor: from + escaped.length },
        // Named as a paste so it joins the undo history as one, and so `Ctrl+Z`
        // takes the whole insertion back rather than unpicking it piecemeal.
        userEvent: 'input.paste',
      });
      return true;
    },
  });
}
