/**
 * Enter and indentation in Java — the editor half of `java-indent` and `java-comment-break`.
 *
 * A tree-sitter descriptor has no Lezer tree, so CodeMirror's own `insertNewlineAndIndent` found no
 * indentation rule for Java and copied the previous line's whitespace. This supplies the rule three
 * ways:
 *
 * - an `indentService`, which every CodeMirror indentation action consults (the electric `}` below,
 *   Insert blank line, Indent selection);
 * - `indentOnInput` language data, so typing `}`, `)`, `]` or finishing a `case …:` label at the start
 *   of a line re-indents it;
 * - Enter itself, which also trims the whitespace around the break, splits `{|}` onto three lines and
 *   continues comments — shapes an indentation number alone cannot express.
 */

import {
  EditorSelection, EditorState, Prec, Text, countColumn,
  type Extension, type SelectionRange, type StateCommand,
} from '@codemirror/state';
import { keymap } from '@codemirror/view';
import { getIndentUnit, indentService, indentString } from '@codemirror/language';
import { bennuSettingsStore } from '$lib/stores/bennu/settings.svelte';
import { javaContextAt } from './java-lexical';
import { javaIndentFor, javaLineBreak, type JavaIndentStyle } from './java-indent';
import { commentBreak, isBlockCommentClosed } from './java-comment-break';

function styleOf(state: EditorState, lineIndent?: (lineFrom: number) => number): JavaIndentStyle {
  return {
    unit: getIndentUnit(state),
    tabSize: state.tabSize,
    indentCaseBody: bennuSettingsStore.javaIndentCaseBody,
    lineIndent,
  };
}

const javaIndentService = indentService.of((cx, pos) =>
  javaIndentFor(cx.state.sliceDoc(0, pos), cx.textAfterPos(pos), styleOf(cx.state, (from) => cx.lineIndent(from))));

/** Re-indent a line once what starts it decides its level: a closer, or a complete `case` label. */
const electricInput = EditorState.languageData.of(() => [
  { indentOnInput: /^\s*(?:[}\])]|(?:case\b.*|default\s*)(?::|->))$/ },
]);

/** The change one range makes: `lines` replace `from..to`, the caret `caretCol` characters into the
 *  second (by default at its end). */
function breakWith(from: number, to: number, lines: string[], caretCol = lines[1].length) {
  return {
    changes: { from, to, insert: Text.of(lines) },
    range: EditorSelection.cursor(from + 1 + caretCol),
  };
}

function breakInComment(state: EditorState, range: SelectionRange) {
  if (!range.empty) return null;
  const line = state.doc.lineAt(range.head);
  // The newline being typed ends the scan, so a comment opened on the last line of the file still
  // reads as a comment rather than as code.
  const ctx = javaContextAt(`${state.sliceDoc(0, line.to)}\n`, range.head);
  if (ctx.kind !== 'comment') return null;

  const lineAfter = state.sliceDoc(range.head, line.to);
  const brk = commentBreak({
    lineBefore: state.sliceDoc(line.from, range.head),
    lineAfter,
    openerAt: ctx.from >= line.from ? ctx.from - line.from : -1,
    block: ctx.block,
    closed: ctx.block && isBlockCommentClosed(state.sliceDoc(range.head)),
  });
  if (!brk) return null;

  let from = range.head;
  while (from > line.from && /[ \t]/.test(state.sliceDoc(from - 1, from))) from--;
  const moved = lineAfter.trimStart();
  if (brk.close !== null) return breakWith(from, line.to, ['', brk.lead + moved, brk.close], brk.lead.length);
  return breakWith(from, line.to - moved.length, ['', brk.lead]);
}

function breakInCode(state: EditorState, range: SelectionRange) {
  const line = state.doc.lineAt(range.from);
  const toLine = state.doc.lineAt(range.to);
  const lineAfter = state.sliceDoc(range.to, toLine.to);
  const brk = javaLineBreak(state.sliceDoc(0, range.from), lineAfter, styleOf(state));
  if (!brk) {
    // Inside a literal or a comment Enter keeps the line's own indentation and every character.
    const own = /^[ \t]*/.exec(line.text)![0];
    return breakWith(range.from, range.to, ['', indentString(state, countColumn(own, state.tabSize))]);
  }
  const from = range.from - brk.trimBefore;
  const to = range.to + brk.trimAfter;
  const lines = ['', indentString(state, brk.caretIndent)];
  if (brk.closeIndent !== null) lines.push(indentString(state, brk.closeIndent));
  return breakWith(from, to, lines);
}

const javaEnter: StateCommand = ({ state, dispatch }) => {
  if (state.readOnly) return false;
  const spec = state.changeByRange((range) => breakInComment(state, range) ?? breakInCode(state, range));
  dispatch(state.update(spec, { scrollIntoView: true, userEvent: 'input' }));
  return true;
};

/** The Java newline behaviours, for the descriptor's `editing` slot. `Prec.high` puts Enter above the
 *  default keymap; the completion popup's Enter sits higher still, so accepting an item wins. */
export const javaNewline: Extension = [
  javaIndentService,
  electricInput,
  Prec.high(keymap.of([{ key: 'Enter', run: javaEnter }])),
];
