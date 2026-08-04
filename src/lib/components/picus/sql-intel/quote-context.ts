/**
 * Which quote, if any, encloses a position — the decision behind escaping a paste.
 *
 * Pure, and in its own file for exactly that reason: it is the part that can be
 * wrong in a way nobody notices, so it has to be runnable without an editor, a DOM
 * or a clipboard. `paste-escape.ts` is the CodeMirror wiring around it and holds no
 * rules of its own.
 */

import type { Dialect } from '$lib/types/picus';
import { scanSql, type SqlToken } from './tokens';

/** Quote characters that are escaped by doubling. */
export type Quote = "'" | '"';

/**
 * The quote enclosing `offset`, or `null` when it is not inside one that should be
 * escaped.
 *
 * The end is exclusive, matching `inLiteral`: a caret just past the closing quote of
 * `'abc'` is back in code, and pasting there is an ordinary paste.
 *
 * Returns `null` for the two constructs that look like strings and must not be
 * touched — a dollar-quoted body and Oracle's `q'[…]'` — because both exist so that
 * what is inside them needs no escaping. Doubling a quote there corrupts the value
 * instead of protecting it, and does so silently.
 */
export function quoteAround(src: string, offset: number, dialect: Dialect): Quote | null {
  const { scan } = scanSql(src, dialect);

  // A literal the scan reached the end of the buffer inside — `WHERE nome = '` with
  // the caret after the quote, which is one of the positions this feature is for.
  // Its token ends at the end of the buffer, so the caret is *at* `to` rather than
  // before it.
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
  // `q'[…]'`, `nq'{…}'`: a prefix letter, a quote and an opening bracket is Oracle's
  // alternative quoting, chosen so apostrophes need no escape at all.
  if (/^[a-z]/i.test(head) && head[1] === "'" && /^[[({<!]/.test(head[2] ?? '')) return null;

  return "'";
}
