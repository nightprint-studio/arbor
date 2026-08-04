/**
 * Pasting into a string literal escapes the quotes.
 *
 * `L'Aquila` pasted between the quotes of `nome = '|'` gives `'L'Aquila'`, which is
 * not a value with an apostrophe in it — it is a closed string, a stray word and a
 * syntax error. Every SQL editor worth using fixes that at the moment of the paste,
 * because the alternative is finding it at the moment of the run.
 *
 * This file is only SQL's half of the rule. Which quote encloses the caret — the
 * part that can be wrong without anyone noticing — is {@link quoteAround}, in
 * `quote-context.ts`, where it can be run without an editor; the CodeMirror wiring
 * is {@link pasteIntoLiteral}, shared with the Java editor.
 *
 * Newlines are left alone on purpose, unlike Java's: a SQL literal may span lines,
 * so a pasted multi-line value is already correct, and breaking it into `||` pieces
 * would be noise — and would not even be portable, since the engines disagree about
 * how strings are concatenated.
 */

import type { Extension } from '@codemirror/state';
import { pasteIntoLiteral } from '$lib/components/shared/ui/code-editor';
import type { Dialect } from '$lib/types/picus';
import { quoteAround } from './quote-context';

/**
 * The CodeMirror extension. Rides with the SQL language descriptor rather than
 * living in the shared editor: escaping is a property of the language, and the
 * editor has no business knowing what a SQL string is.
 */
export function escapeQuotesOnPaste(dialect: Dialect): Extension {
  return pasteIntoLiteral((doc, offset, text) => {
    const quote = quoteAround(doc, offset, dialect);
    if (!quote) return null;
    return text.split(quote).join(quote + quote);
  });
}
