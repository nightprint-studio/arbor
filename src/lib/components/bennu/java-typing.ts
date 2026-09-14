/**
 * Typing delimiters in Java — the quote rules, and `<` over a selection.
 *
 * The shared editor auto-closes with CodeMirror's `closeBrackets`, which decides whether a `"`
 * starts a string by asking the Lezer syntax tree. A tree-sitter descriptor has no Lezer tree, so
 * for Java that question always came back "no string here": a `"` typed inside a literal or a
 * comment still inserted a pair, and the third quote of `"""` produced `""""`. Java therefore takes
 * the quotes away from `closeBrackets` (see {@link closeBracketsConfig}) and owns them here,
 * answering the question from {@link javaContextAt} instead.
 *
 * `"` and `'` follow the same rules:
 *
 * - over a **selection** they wrap it;
 * - right before the closing quote of the literal the caret is in, they **step over** it;
 * - they **pair** only in code (not in a literal, not in a comment), when the character before is
 *   neither a word character nor a quote, and the one after is whitespace, the end of the line or
 *   a closer (`) ] } , ;`) — anything else would glue a quote to a word;
 * - **Backspace** between the two quotes of an empty literal removes both.
 *
 * A third `"` right after an empty `""` in code opens a **text block**: the delimiter, the caret on
 * an indented line of its own, and the closing delimiter below at the same indentation (so the
 * block's value carries no incidental indentation). Java requires the line break after an opening
 * `"""`, so writing the whole shape at once is what leaves the rest of the file as code rather
 * than swallowed into an unterminated block.
 *
 * `<` wraps a selection and does nothing else: the same character opens a type argument list and a
 * comparison, so pairing a bare `<` would be wrong half the time.
 */

import {
  EditorSelection, EditorState, Prec,
  type ChangeSpec, type Extension, type SelectionRange, type StateCommand,
} from '@codemirror/state';
import { EditorView, keymap } from '@codemirror/view';
import { indentUnit } from '@codemirror/language';
import { wrapSelectionOnType } from '$lib/components/shared/ui/code-editor';
import { javaContextAt, type JavaContext } from './java-lexical';

type Quote = '"' | "'";

/** The literal kind a quote delimits. */
const LITERAL_OF: Record<Quote, 'string' | 'char'> = { '"': 'string', "'": 'char' };

/** A character a pair may be glued to the front of. */
const CLOSERS = new Set([')', ']', '}', ',', ';']);

/** A character that makes a quote a suffix of a word rather than the start of a literal. */
const WORD_CHAR = /[\p{L}\p{N}_$]/u;

function isQuote(text: string): text is Quote {
  return text === '"' || text === "'";
}

/**
 * The brackets `closeBrackets` still owns in Java — the quotes are gone from the list.
 *
 * `,` joins the closers a bracket may pair in front of, so `foo(|, b)` gets its `()` like
 * `foo(|)` does.
 */
const closeBracketsConfig = EditorState.languageData.of(() => [
  { closeBrackets: { brackets: ['(', '[', '{'], before: ')]}:;>,' } },
]);

/**
 * The lexical context at `pos`, scanning only up to the end of its line.
 *
 * Everything this module decides depends on the text before the caret and on the rest of the
 * caret's own line (a closing quote is never past it). The one extra character keeps a caret at a
 * line end inside a block comment reading as "in the comment".
 */
function contextAt(state: EditorState, pos: number): JavaContext {
  const lineEnd = state.doc.lineAt(pos).to;
  return javaContextAt(state.sliceDoc(0, Math.min(state.doc.length, lineEnd + 1)), pos);
}

/** Is `ctx` a terminated `quote` literal whose closing quote sits exactly at `closeAt`? */
function closesAt(ctx: JavaContext, quote: Quote, closeAt: number): boolean {
  return ctx.kind === LITERAL_OF[quote] && ctx.terminated && ctx.to === closeAt + 1;
}

/** What typing `quote` at an empty `range` does, or `null` for an ordinary insertion. */
function quoteAction(
  state: EditorState,
  range: SelectionRange,
  quote: Quote,
): { changes?: ChangeSpec; range: SelectionRange } | null {
  const pos = range.head;
  const prev = state.sliceDoc(pos - 1, pos);
  const next = state.sliceDoc(pos, pos + 1);
  const ctx = contextAt(state, pos);

  if (next === quote && closesAt(ctx, quote, pos)) {
    return { range: EditorSelection.cursor(pos + 1) };
  }

  if (quote === '"' && ctx.kind === 'code' && opensTextBlock(state, pos)) {
    const line = state.doc.lineAt(pos);
    const indent = /^[ \t]*/.exec(line.text)![0] + state.facet(indentUnit);
    return {
      changes: { from: pos, insert: `"\n${indent}\n${indent}"""` },
      range: EditorSelection.cursor(pos + 2 + indent.length),
    };
  }

  const gluedToWord = WORD_CHAR.test(prev) || isQuote(prev);
  const roomAfter = next === '' || /\s/.test(next) || CLOSERS.has(next);
  if (ctx.kind === 'code' && !gluedToWord && roomAfter) {
    return {
      changes: { from: pos, insert: quote + quote },
      range: EditorSelection.cursor(pos + 1),
    };
  }
  return null;
}

/** Is `pos` right after an empty `""` that starts in code (and is not itself part of a longer
 *  run of quotes)? Typing the third `"` there opens a text block. */
function opensTextBlock(state: EditorState, pos: number): boolean {
  if (pos < 2 || state.sliceDoc(pos - 2, pos) !== '""') return false;
  if (state.sliceDoc(pos - 3, pos - 2) === '"') return false;
  return contextAt(state, pos - 2).kind === 'code';
}

/** The input handler for `"` / `'` at a caret. A selection is wrapped earlier by
 *  `wrapSelectionOnType`; a caret that none of the rules applies to is left to the default input. */
const quoteInput = EditorView.inputHandler.of((view, from, to, insert) => {
  if (!isQuote(insert) || view.composing || view.state.readOnly) return false;
  const { state } = view;
  const main = state.selection.main;
  if (from !== main.from || to !== main.to) return false;

  let special = false;
  const spec = state.changeByRange((range) => {
    const action = range.empty ? quoteAction(state, range, insert) : null;
    if (action) {
      special = true;
      return action;
    }
    return {
      changes: { from: range.from, to: range.to, insert },
      range: EditorSelection.cursor(range.from + insert.length),
    };
  });
  if (!special) return false;
  view.dispatch(state.update(spec, { scrollIntoView: true, userEvent: 'input.type' }));
  return true;
});

/** Is `range` a caret between the two quotes of an empty `""` / `''`? Asked of the lexer, not of
 *  the two characters alone: in `"a"|"b"` the neighbours are quotes too, and they belong to two
 *  different literals. */
function inEmptyLiteral(state: EditorState, range: SelectionRange): boolean {
  if (!range.empty) return false;
  const pos = range.head;
  const quote = state.sliceDoc(pos - 1, pos);
  if (!isQuote(quote) || state.sliceDoc(pos, pos + 1) !== quote) return false;
  const ctx = contextAt(state, pos);
  return closesAt(ctx, quote, pos) && 'from' in ctx && ctx.from === pos - 1;
}

/** Backspace between the two quotes of an empty literal deletes both. Every caret must be in that
 *  position, or the key falls through to an ordinary delete. */
const deleteEmptyLiteral: StateCommand = ({ state, dispatch }) => {
  if (state.readOnly) return false;
  let refused = false;
  const spec = state.changeByRange((range) => {
    const pos = range.head;
    if (!inEmptyLiteral(state, range)) {
      refused = true;
      return { range };
    }
    return {
      changes: { from: pos - 1, to: pos + 1 },
      range: EditorSelection.cursor(pos - 1),
    };
  });
  if (refused) return false;
  dispatch(state.update(spec, { scrollIntoView: true, userEvent: 'delete.backward' }));
  return true;
};

/** The Java typing behaviours, for the descriptor's `editing` slot. */
export const javaTyping: Extension = [
  closeBracketsConfig,
  // Above the shared editor's `closeBrackets` handler and its Backspace binding, so these rules
  // are asked first. The wrap comes before the quote handler: a selection is never a caret.
  Prec.high([
    wrapSelectionOnType({ '<': '>', '"': '"', "'": "'" }),
    quoteInput,
    keymap.of([{ key: 'Backspace', run: deleteEmptyLiteral }]),
  ]),
];
