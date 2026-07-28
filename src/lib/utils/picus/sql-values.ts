/**
 * How the value grid *describes* a typed value — display affordances only.
 *
 * Nothing here produces SQL. The emitted statement is always the backend's
 * (`picus-emit`), which owns the golden tests; these two functions exist so the
 * grid can put an "expr" marker next to a value that will pass through unquoted,
 * and say what it becomes on each engine, without a round trip per keystroke.
 *
 * That means they **mirror** `picus-emit`'s `literal.rs` and must be changed with
 * it: the closed list of recognised expressions is deliberately closed there —
 * guessing whether something looks like an expression would eventually let a
 * user's literal text through unquoted — and it is closed here for the same
 * reason. A drift between the two makes the marker lie; it never makes the SQL
 * wrong, which is why the duplication is acceptable and the direction of truth is
 * not in doubt.
 *
 * The engine descriptors served by `picus_providers` already carry
 * `emission.nowFunction`; when the generator reads descriptors (design §4.3) this
 * module is what they replace.
 */

import type { Dialect } from '$lib/types/picus';

/** Values the user means as expressions, not as string literals. */
const EXPRESSION_RE = /^(SYSDATE|CURRENT_TIMESTAMP|CURRENT_DATE|NOW\(\)|NULL)$/i;

export function looksLikeExpression(value: string): boolean {
  return EXPRESSION_RE.test(value.trim());
}

/** The per-dialect "now" function. */
export function nowFunction(dialect: Dialect): string {
  return dialect === 'oracle' ? 'SYSDATE' : 'CURRENT_TIMESTAMP';
}
