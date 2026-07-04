/**
 * Emmet abbreviation expansion on **Tab** for markup buffers (HTML / JSP).
 *
 * A thin CodeMirror 6 keymap over the framework-agnostic `emmet` core: on Tab, extract the
 * abbreviation immediately left of a collapsed caret (`ul>li.item*3`, `div#app`, `a[href]`),
 * expand it to markup, and replace it. When there's no abbreviation — or a non-empty selection —
 * the binding returns `false`, so Tab falls through to the editor's normal indent.
 *
 * Markup syntax only (`html`): a JSP is markup at the top level, which is where Emmet earns its
 * keep. CSS-context expansion (inside `<style>`) is deliberately out of scope for this first cut —
 * it needs the syntax tree to know the caret is in a stylesheet region.
 */

import { keymap, type Command } from '@codemirror/view';
import expandAbbreviation, { extract } from 'emmet';

/** Expand the Emmet abbreviation left of the caret, or return false to let Tab indent. */
const expandOnTab: Command = (view) => {
  const { state } = view;
  const sel = state.selection.main;
  // Only on a collapsed caret — a Tab with a selection is "indent the block".
  if (!sel.empty) return false;

  const line = state.doc.lineAt(sel.head);
  const posInLine = sel.head - line.from;
  const extracted = extract(line.text, posInLine, { type: 'markup' });
  if (!extracted || !extracted.abbreviation) return false;

  let output: string;
  try {
    output = expandAbbreviation(extracted.abbreviation, { type: 'markup', syntax: 'html' });
  } catch {
    return false; // not a valid abbreviation → let Tab indent
  }
  if (!output) return false;

  // Re-indent continuation lines to the current line's leading whitespace, so a nested
  // expansion lands under the caret's column instead of at column 0.
  const indent = /^[ \t]*/.exec(line.text)?.[0] ?? '';
  const insert = output
    .split('\n')
    .map((l, i) => (i === 0 ? l : indent + l))
    .join('\n');

  const from = line.from + extracted.start;
  const to = line.from + extracted.end;
  view.dispatch({
    changes: { from, to, insert },
    selection: { anchor: from + insert.length },
    scrollIntoView: true,
    userEvent: 'input.complete',
  });
  return true;
};

/** The Emmet Tab keymap — install it BEFORE the base keymap so its Tab is tried first and,
 *  on no-match, falls through to `indentWithTab`. */
export function emmetKeymap() {
  return keymap.of([{ key: 'Tab', run: expandOnTab }]);
}
