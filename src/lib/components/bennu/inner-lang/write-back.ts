/**
 * Writing an edited value back into the literal(s) it was read from.
 *
 * The value was unescaped on the way in, so it has to be re-escaped on the way out *for that
 * literal*: a Java string needs `\\d`, a text block only needs its backslashes doubled and keeps its
 * indentation, a Rust raw string takes the pattern verbatim, a JS template escapes `${`. A
 * concatenation `"a" + "b"` collapses to one literal holding the whole value — the edit replaces
 * everything between the first literal's contents and the last one's.
 */

import type { InnerSite } from './detect';
import { escapeForLiteral, textBlockIndent } from './units';

export interface SourceEdit { from: number; to: number; text: string }

/** The edit that makes `site` read back as `value`, or null when this literal cannot hold it
 *  (a raw string that would contain its own closing delimiter, or a chain that mixes literal kinds). */
export function rewriteSite(src: string, site: InnerSite, value: string): SourceEdit | null {
  const first = site.literals[0];
  const last = site.literals[site.literals.length - 1];
  if (site.literals.some((l) => l.textBlock !== first.textBlock || l.raw !== first.raw)) return null;
  const indent = first.textBlock ? textBlockIndent(src, first) : '';
  const text = escapeForLiteral(value, site.host, first, {
    quote: src[first.open],
    indent,
    hashes: first.raw ? first.start - first.open - 2 : 0,
  });
  if (text === null) return null;
  // A text block whose value ends in a line break closes on a line of its own, at its indentation.
  const body = first.textBlock && value.endsWith('\n') ? text + indent : text;
  return { from: first.start, to: last.end, text: body };
}
