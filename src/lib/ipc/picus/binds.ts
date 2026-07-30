/**
 * Running a statement with its values bound.
 *
 * A door of its own rather than an argument on {@link execute}, because the two
 * differ in what they hand back: a cursor cannot take parameters, so a bound read
 * opens **no scrollable result** — `resultId` is null and `endOfResult` says
 * whether anything was left behind. Everything else about the reply is the same
 * `ExecuteResult` the rest of the studio already reads.
 *
 * The values never enter the SQL text. That is the entire point of the feature and
 * it is enforced in the driver, not here: the statement crosses the wire with its
 * `$1` intact and the value travels in the protocol's own field, where nothing can
 * read it as syntax.
 */

import { picus } from '../rpc';
import type { ExecuteResult } from './db';

/**
 * One value on the wire.
 *
 * `null` is a real SQL NULL and **not** the empty string — the two are different
 * rows on a text column, and the modal asks for them separately for that reason.
 * Picus sends strings: what the user typed, parsed by the server with the input
 * function of the type it inferred for that placeholder, which is what makes its
 * complaint about a bad value the server's own words.
 */
export type BindValue = string | number | boolean | null;

/**
 * Run one statement with `binds` bound to its placeholders.
 *
 * `binds` is **positional**: entry 0 is `$1` on PostgreSQL, and the first
 * placeholder in order of appearance on Oracle. Build it with `toBindList` from
 * `sql-intel/binds`, which knows both orders.
 *
 * Only for a connection whose engine reports `capabilities.bindParameters` — an
 * engine without it must not be offered the flow at all, rather than offered it and
 * refused.
 */
export function executeBound(
  connectionId: string,
  sql: string,
  binds: BindValue[],
  window?: number,
): Promise<ExecuteResult> {
  return picus('picus_execute_bound', { connectionId, sql, binds, window });
}
