/**
 * Indentation for `.ptn` — what Enter and `}` should do.
 *
 * Patina is a stream mode with no syntax tree, so CodeMirror's own indenter found no rule and copied the previous
 * line's whitespace: after `{` the next line stayed at the same column. This is the rule, as a pure function of the
 * text before the line (the editor half is `patinaIndent` at the bottom).
 *
 * ## The rule
 *
 * Brackets decide. Walking the text before the line, every unclosed `{`, `(` or `[` is on a stack with the indent of
 * the line that holds it (strings, `//` comments and loop labels `'a` skipped):
 *
 * | the new line | its indent |
 * |---|---|
 * | inside an open bracket | the **opener's line** + one unit |
 * | starts with a closer (`}` `)` `]`) | the opener's line |
 * | after a line ending in `=>` (an arm with no braces) | that line + one unit |
 * | starts with `.method` (a chain going on) | the chain's first line + one unit, then the same |
 * | anything else outside every bracket | 0 — Patina's items start at the left edge |
 *
 * The opener's line, not the opener's column: `foo(a,\n` and `let x = [\n` both indent one unit from where the line
 * began, the block style the language is written in, never aligned under the bracket.
 */

import { EditorState } from '@codemirror/state';
import { getIndentUnit, indentService } from '@codemirror/language';
import type { Extension } from '@codemirror/state';

interface Opener {
  /** Columns of the line that holds the bracket. */
  indent: number;
}

/** Columns of the whitespace a line starts with (a tab goes to the next multiple of `tabSize`). */
export function columnsOf(line: string, tabSize: number): number {
  let col = 0;
  for (const c of line) {
    if (c === ' ') col++;
    else if (c === '\t') col += tabSize - (col % tabSize);
    else break;
  }
  return col;
}

/** The brackets a line opens and closes, in order, with strings and `//` skipped. */
function* bracketsOf(line: string): Generator<string> {
  let inStr = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (inStr) {
      if (c === '\\') i++;
      else if (c === '"') inStr = false;
      continue;
    }
    if (c === '"') inStr = true;
    else if (c === '/' && line[i + 1] === '/') return;
    else if ('{([})]'.includes(c)) yield c;
  }
}

/** A line is code when it is not blank and not only a comment. */
const isCode = (line: string) => {
  const t = line.trim();
  return t !== '' && !t.startsWith('//');
};

/** The code of a line without its trailing comment, trimmed on the right. */
function codeOf(line: string): string {
  let inStr = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (inStr) {
      if (c === '\\') i++;
      else if (c === '"') inStr = false;
    } else if (c === '"') inStr = true;
    else if (c === '/' && line[i + 1] === '/') return line.slice(0, i).trimEnd();
  }
  return line.trimEnd();
}

/**
 * The indent, in columns, of the line that starts right after `before` (which ends where that line starts, so its last
 * line is empty or the whitespace already there). `textAfter` is what follows on that line.
 */
export function patinaIndentFor(before: string, textAfter: string, unit: number, tabSize = 4): number {
  const lines = before.split('\n');
  lines.pop();
  const stack: Opener[] = [];
  let prev: string | null = null;
  for (const line of lines) {
    const indent = columnsOf(line, tabSize);
    for (const b of bracketsOf(line)) {
      if (b === '{' || b === '(' || b === '[') stack.push({ indent });
      else stack.pop();
    }
    if (isCode(line)) prev = line;
  }
  const after = textAfter.trimStart();
  const closer = after.startsWith('}') || after.startsWith(')') || after.startsWith(']');
  const prevIndent = prev === null ? 0 : columnsOf(prev, tabSize);
  const prevCode = prev === null ? '' : codeOf(prev);
  // A chain going on: `.method()` under the line that started it.
  if (after.startsWith('.') && !after.startsWith('..') && prev !== null) {
    return prevIndent + (prev.trimStart().startsWith('.') ? 0 : unit);
  }
  const top = stack[stack.length - 1];
  if (top) {
    if (closer) return top.indent;
    // An arm without braces: its body is one unit in from the arrow's line (unless that line itself opened the bracket).
    if (prev !== null && /=>$/.test(prevCode)) return prevIndent + unit;
    return top.indent + unit;
  }
  if (prev !== null && /=>$/.test(prevCode) && !closer) return prevIndent + unit;
  return 0;
}

const patinaIndentService = indentService.of((cx, pos) => patinaIndentFor(cx.state.sliceDoc(0, pos), cx.textAfterPos(pos), getIndentUnit(cx.state), cx.state.tabSize));

/** Re-indent a line as soon as what starts it decides its level: typing a closer. */
const electricCloser = EditorState.languageData.of(() => [{ indentOnInput: /^\s*[}\])]$/ }]);

/** The editor half: the indentation rule, and the closers that re-indent their line when typed. */
export const patinaIndent: Extension = [patinaIndentService, electricCloser];
