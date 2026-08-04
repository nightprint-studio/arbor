/**
 * What to do about a line the editor stops colouring.
 *
 * CodeMirror's stream highlighter gives up **10 000 characters into a line**
 * (`C.MaxLineLength` in `@codemirror/language`, an inlined constant with no option
 * behind it) and emits no tokens past it. Everything after that point on that line
 * draws with no style at all, which is white text — and, since it is per *line*
 * rather than per file, a script looks perfectly fine until you scroll far enough
 * along the one enormous line.
 *
 * It is a guard rather than a fault. The mode re-reads a line from its start on
 * every keystroke, so lifting the limit would trade a cosmetic problem for an
 * editor that cannot be typed in. What is wrong is only that it happens **in
 * silence**: a screen that goes blank halfway through a value looks like a broken
 * product, and there is nothing on it that says otherwise.
 *
 * So this module does the two things that are actually available:
 *
 *  1. {@link longLineWarnings} — say it, once per line, exactly where the colour
 *     stops, with what to do instead.
 *  2. {@link longLineMarks} — put the colour back for the case that matters. Our own
 *     scanner reads the whole buffer correctly however long the lines are, so the
 *     part of a **string or comment** that runs past the limit can be handed to the
 *     editor as a host mark. One range per token rather than per word: the case this
 *     exists for is a single enormous literal, and painting it is one decoration,
 *     not ten thousand.
 *
 * Ordinary code past the limit stays plain. Marking every keyword and operator on a
 * 200 KB line would cost more than the colour is worth, and unlike a literal — where
 * the missing colour makes data look like code — plain code beyond the limit reads
 * as what it is.
 */

import type { Dialect } from '$lib/types/picus';
import { scanSql } from './tokens';

/**
 * Where CodeMirror stops. Mirrored from `@codemirror/language`, which inlines it as
 * a `const enum` and exports nothing — so if it ever changes upstream this number is
 * the one place to change here, and the tests that name it will say so.
 */
export const HIGHLIGHT_LIMIT = 10_000;

/** A line that runs past the limit: where it starts and where the colour dies. */
interface LongLine {
  /** Offset of the first character of the line. */
  start: number;
  /** Offset the highlighter stops at. */
  cutoff: number;
  /** 1-based line number, for the message. */
  line: number;
  length: number;
}

/** Every line longer than the limit. Cheap: one pass over the newlines. */
function longLines(src: string): LongLine[] {
  const out: LongLine[] = [];
  let start = 0;
  let line = 1;
  for (;;) {
    const nl = src.indexOf('\n', start);
    const end = nl < 0 ? src.length : nl;
    if (end - start > HIGHLIGHT_LIMIT) {
      out.push({ start, cutoff: start + HIGHLIGHT_LIMIT, line, length: end - start });
    }
    if (nl < 0) break;
    start = nl + 1;
    line += 1;
  }
  return out;
}

export interface LongLineWarning {
  /** UTF-16 offset where the colour stops. */
  from: number;
  to: number;
  message: string;
}

/**
 * One warning per over-long line, anchored at the character the colour stops at.
 *
 * Anchored there rather than at the start of the line on purpose: that is the place
 * the user is looking at when they notice, and a marker at column 1 of a line two
 * hundred thousand characters long is a marker nobody will ever scroll to.
 */
export function longLineWarnings(src: string): LongLineWarning[] {
  return longLines(src).map(({ cutoff, line, length }) => ({
    from: cutoff,
    to: Math.min(cutoff + 1, src.length),
    message:
      `Line ${line} is ${length.toLocaleString()} characters long, and the editor stops `
      + `colouring a line after ${HIGHLIGHT_LIMIT.toLocaleString()}. Everything past here `
      + 'is drawn plain — the statement itself is unaffected. To keep the colour, put a '
      + 'value this size in the cell it belongs to (right-click a row: “Load from file…”, '
      + 'or “Replace from file…” on a large object) rather than in the statement, or break '
      + 'the literal across lines with the concatenation operator.',
  }));
}

/** A range the host asks the editor to paint. Matches `CodeEditor`'s `marks` prop. */
export interface HighlightMark {
  from: number;
  to: number;
  className: string;
}

/** However long the buffer, this many ranges is already more than the eye uses. */
const MAX_MARKS = 200;

/**
 * The colour the highlighter gave up on, for strings and comments only.
 *
 * Built from `scanSql`, which reads the whole buffer with no length limit and gets
 * `''` doubling right — so the range handed over is the real extent of the literal,
 * not the highlighter's naive pairing of quotes.
 */
export function longLineMarks(src: string, dialect: Dialect): HighlightMark[] {
  const lines = longLines(src);
  if (lines.length === 0) return [];

  const { scan } = scanSql(src, dialect);
  const out: HighlightMark[] = [];

  for (const token of scan.tokens) {
    if (out.length >= MAX_MARKS) break;
    if (token.kind !== 'string' && token.kind !== 'comment') continue;
    // The cutoff of the line this token ends on — a token may span several lines,
    // and what matters is where the *uncoloured* part of it begins.
    for (const line of lines) {
      if (token.to <= line.cutoff) continue;
      if (token.from >= line.start + line.length) continue;
      const from = Math.max(token.from, line.cutoff);
      const to = Math.min(token.to, line.start + line.length);
      if (to > from) {
        out.push({ from, to, className: token.kind === 'string' ? 'cm-tok-string' : 'cm-tok-comment' });
      }
    }
  }
  return out;
}
