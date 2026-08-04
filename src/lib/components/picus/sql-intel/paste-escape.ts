/**
 * Pasting into a string literal escapes the quotes.
 *
 * `L'Aquila` pasted between the quotes of `nome = '|'` gives `'L'Aquila'`, which is
 * not a value with an apostrophe in it — it is a closed string, a stray word and a
 * syntax error. Every SQL editor worth using fixes that at the moment of the paste,
 * because the alternative is finding it at the moment of the run.
 *
 * ## The rule, and why it is narrow
 *
 * The escape happens **only** where an unescaped quote could not have been meant:
 * inside a `'…'` literal, and inside a `"…"` delimited identifier. Everywhere else
 * the paste is left exactly as it came, because everywhere else a quote is a quote.
 *
 * Two places are deliberately excluded even though they look like strings:
 *
 *  • a **dollar-quoted body** (`$$ … $$`) — its whole point is that nothing inside
 *    needs escaping, so doubling a quote there would corrupt the value rather than
 *    protect it;
 *  • an Oracle **alternative-quoted** literal (`q'[…]'`) — chosen precisely to hold
 *    apostrophes verbatim, for the same reason.
 *
 * Getting either of those wrong would silently change data on its way into a
 * script, which is the one failure this product exists to prevent. When in doubt
 * this does nothing: an unescaped paste is a visible error, a wrongly escaped one
 * is a value with extra characters in it that nobody notices until it is stored.
 */

import { EditorView } from '@codemirror/view';
import type { Extension } from '@codemirror/state';
import type { Dialect } from '$lib/types/picus';
import { scanSql, type SqlToken } from './tokens';

/** Quote characters this knows how to double. */
type Quote = "'" | '"';

/**
 * The quote enclosing `offset`, or `null` when it is not inside one that should be
 * escaped.
 *
 * The end is exclusive on purpose, matching `inLiteral`: a caret just past the
 * closing quote of `'abc'` is back in code, and pasting there is an ordinary paste.
 */
function quoteAround(src: string, offset: number, dialect: Dialect): Quote | null {
  const { scan } = scanSql(src, dialect);

  // A literal the scan reached the end of the buffer inside — `WHERE nome = '` with
  // the caret after the quote, which is exactly the position this feature is for.
  // Its token ends at the end of the buffer, and so the caret is *at* `to` rather
  // than before it.
  //
  // Gated on `scan.open` and not on the offset alone: a **closed** string can also
  // end at the end of the buffer, and treating `… = 'abc'` with the caret past the
  // closing quote as "inside" would escape a paste that is plainly outside it.
  const openKind = scan.open?.kind;
  const unterminated = openKind === 'string' || openKind === 'quoted';

  let found: SqlToken | null = null;
  for (const token of scan.tokens) {
    if (token.from >= offset) break;
    if (token.to < offset) continue;
    if (offset < token.to || (unterminated && token.to === src.length)) found = token;
  }
  if (!found) return null;

  if (found.kind === 'quoted') return '"';
  if (found.kind !== 'string') return null;

  const head = found.text;
  // A dollar-quoted body holds its text verbatim — that is what it is for.
  if (head.startsWith('$')) return null;
  // `q'[…]'`, `nq'{…}'`: the prefix letter followed by a quote and a bracket is
  // Oracle's alternative quoting, chosen so apostrophes need no escape at all.
  if (/^[a-z]/i.test(head) && head[1] === "'" && /^[[({<!]/.test(head[2] ?? '')) return null;

  return "'";
}

/**
 * The CodeMirror extension. Rides with the SQL language descriptor rather than
 * living in the shared editor: escaping is a property of the language, and the
 * editor has no business knowing what a SQL string is.
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
        // takes the whole insertion back rather than unpicking it character by
        // character.
        userEvent: 'input.paste',
      });
      return true;
    },
  });
}
