/**
 * The editor side of case conversion: which text a range stands for, and putting the result back.
 *
 * The conversion itself is `$lib/utils/case-convert` (pure, and tested there). What belongs here is
 * the CodeMirror half: a selection is converted as it is, an empty range stands for the identifier
 * around the caret, and every range of a multi-cursor selection is converted on its own — in one
 * transaction, so one undo step takes the whole thing back.
 *
 * The converted text is left SELECTED. That is what makes the cycle command usable: a caret in
 * `fooBar` becomes a selection of `FooBar`, and the next press converts that selection onward.
 */
import { EditorSelection, type ChangeSpec, type SelectionRange } from '@codemirror/state';
import type { EditorView } from '@codemirror/view';
import { convertCase, cycleCase, type CaseStyle } from '$lib/utils/case-convert';

/** What counts as part of the identifier under a caret. `-` is not: in code it is a minus, and a
 *  kebab-case name is converted by selecting it. */
const IDENTIFIER_CHAR = /[\p{L}\p{N}_$]/u;

/** The span an empty `range` stands for: the identifier it touches, or `null` on whitespace/punctuation. */
export function identifierAround(view: EditorView, pos: number): { from: number; to: number } | null {
  const line = view.state.doc.lineAt(pos);
  const text = line.text;
  let start = pos - line.from;
  let end = start;
  while (start > 0 && IDENTIFIER_CHAR.test(text[start - 1])) start -= 1;
  while (end < text.length && IDENTIFIER_CHAR.test(text[end])) end += 1;
  return start === end ? null : { from: line.from + start, to: line.from + end };
}

/**
 * Convert the selection (or the identifier at the caret) to `style`, or one step round the cycle when
 * `style` is `'cycle'`. Returns `false` when nothing was there to convert.
 */
export function convertCaseInView(view: EditorView, style: CaseStyle | 'cycle'): boolean {
  const changes: ChangeSpec[] = [];
  const spans: { from: number; to: number; text: string }[] = [];
  for (const range of view.state.selection.ranges) {
    const span = range.empty ? identifierAround(view, range.head) : { from: range.from, to: range.to };
    if (!span) continue;
    const before = view.state.sliceDoc(span.from, span.to);
    const after = style === 'cycle' ? cycleCase(before) : convertCase(before, style);
    spans.push({ ...span, text: after });
    if (after !== before) changes.push({ from: span.from, to: span.to, insert: after });
  }
  if (spans.length === 0) return false;

  // Where each converted span ends up: earlier spans that changed length push the later ones.
  const ranges: SelectionRange[] = [];
  let shift = 0;
  for (const span of spans) {
    const from = span.from + shift;
    ranges.push(EditorSelection.range(from, from + span.text.length));
    shift += span.text.length - (span.to - span.from);
  }
  view.dispatch({
    changes,
    // Ranges with nothing to convert are skipped, so the main index may have no counterpart.
    selection: EditorSelection.create(ranges, Math.min(view.state.selection.mainIndex, ranges.length - 1)),
    userEvent: 'input.case',
    scrollIntoView: true,
  });
  return true;
}
