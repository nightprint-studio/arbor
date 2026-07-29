/**
 * How the value grid *describes* a typed value — display affordances only.
 *
 * Nothing here produces SQL. The emitted statement is always the backend's
 * (`picus-emit`), which owns the golden tests; these functions exist so the grid
 * can mark a cell that will pass through unquoted, and say what it becomes on each
 * engine, without a round trip per keystroke.
 *
 * They mirror `picus-emit`'s `literal.rs` and must be changed with it — but the
 * mirroring is now trivial, and that is the point of the prefix. Reading "is this
 * an expression" used to mean keeping a closed list of recognised spellings in
 * step across two languages; now it is one character, and there is nothing left
 * to drift.
 */

import type { Dialect } from '$lib/types/picus';

/** How a cell was written. */
export type Written =
  /** Nothing typed. Not `NULL`: an unsupplied column is left OUT of the
   *  statement, so its default still applies. */
  | { kind: 'nothing' }
  /** A value. Quoted, or bare when the column is numeric. */
  | { kind: 'value'; text: string }
  /** SQL, passed through. */
  | { kind: 'expression'; sql: string };

/**
 * Read a cell as the user wrote it.
 *
 * A leading `=` means "this is SQL". `==` escapes it, for the value that really
 * does start with an equals sign — the same doubling rule SQL uses for a quote
 * inside a literal.
 */
export function readValue(value: string): Written {
  const raw = value.trim();
  if (!raw) return { kind: 'nothing' };
  if (raw.startsWith('==')) return { kind: 'value', text: raw.slice(1) };
  if (raw.startsWith('=')) return { kind: 'expression', sql: raw.slice(1).trim() };
  return { kind: 'value', text: raw };
}

/** Will this cell be emitted as SQL rather than as a quoted value? */
export function isExpression(value: string): boolean {
  return readValue(value).kind === 'expression';
}

/** The spellings of "now" that both engines understand between them. */
const NOW_RE = /^(SYSDATE|CURRENT_TIMESTAMP|CURRENT_DATE|NOW\(\))$/i;

/** Is this expression the one thing Picus translates per dialect? */
export function isNow(sql: string): boolean {
  return NOW_RE.test(sql.trim());
}

/** The per-dialect "now" function. */
export function nowFunction(dialect: Dialect): string {
  return dialect === 'oracle' ? 'SYSDATE' : 'CURRENT_TIMESTAMP';
}
