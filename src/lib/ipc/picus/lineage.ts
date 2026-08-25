/**
 * Where a column comes from, when the answer is several views deep.
 *
 * ## Not the same thing as `columnSources`
 *
 * A result's `columnSources` is the **server's own statement** about which relation
 * each column is read from. It cannot be wrong, it costs nothing, and it names the
 * relation the statement asked for — which, for a query over a view, is the view.
 *
 * This is a **deduction**: the backend reads the views' defining SQL and follows
 * each column back through them until it reaches a table. It answers the question
 * the other one cannot — *which table is this datum actually in, three views down* —
 * and it can be wrong in ways the other cannot. So it is asked for explicitly, never
 * computed behind a query, and anything showing both has to keep them visibly apart.
 *
 * ## Every answer carries its own certainty
 *
 * There is no "unknown" bucket. A column is `resolved` (follow the hops to a table),
 * `derived` (computed from the named ingredients, so there is no one table and
 * nothing to write back through), or `unresolved` (with the sentence saying where
 * the trail stopped). Collapsing the last two would throw away the useful half of
 * the answer.
 */

import { picus } from '../rpc';

/** One step of the journey, outermost first. */
export interface Hop {
  /** The relation this hop reads from, folded. */
  relation: string;
  /** What that relation calls the column — how a rename becomes visible. */
  column: string;
  /** This relation is a view, so the trail continues through it. */
  isView: boolean;
}

/** A column a computed value is made of. */
export interface Ingredient {
  /** Empty when the reference could not be attributed to one relation. */
  relation: string;
  column: string;
}

/**
 * How a trace ended.
 *
 * `split` is the one worth reading twice: a set operation whose arms read different
 * tables. The value **is** a real column — of one table for some rows and another
 * for the rest — which is a different answer from `derived`, and collapsing the two
 * would tell the reader "nothing to write back through" when in fact there are two
 * writable tables rather than none.
 */
export type Verdict = 'resolved' | 'derived' | 'split' | 'unresolved';

/** Where one column comes from. */
export interface Trace {
  output: string;
  verdict: Verdict;
  hops: Hop[];
  /** For `derived`: what the value is computed from. Not traced onward — each is a
   *  lineage of its own, and following them all turns one question into a forest. */
  reads: Ingredient[];
  /** For `unresolved`: why the walk stopped, in the user's terms. */
  stopped: string;
}

/** Everything one relation's or one statement's columns trace back to. */
export interface Lineage {
  /** The view traced, or empty when a statement was. */
  relation: string;
  columns: Trace[];
  /** Every view passed through, in the order they were met. */
  through: string[];
  /** The depth limit was reached somewhere; some traces stop for that reason. */
  truncated: boolean;
}

/** The base table a trace ends on, or `''` when it does not end on one. */
export function baseRelation(trace: Trace): string {
  return trace.verdict === 'resolved' ? (trace.hops.at(-1)?.relation ?? '') : '';
}

/** The column's name on that base table, or `''`. */
export function baseColumn(trace: Trace): string {
  return trace.verdict === 'resolved' ? (trace.hops.at(-1)?.column ?? '') : '';
}

/** Did the name change on the way down? What makes a chain worth reading. */
export function renamed(trace: Trace): boolean {
  const base = baseColumn(trace);
  return !!base && base !== trace.output;
}

/** Trace every column of one view back to the tables behind it. */
export function relationLineage(connectionId: string, relation: string): Promise<Lineage> {
  return picus('picus_relation_lineage', { connectionId, relation });
}

/** Trace the columns one statement projects — what the result on screen is traced by. */
export function statementLineage(connectionId: string, sql: string): Promise<Lineage> {
  return picus('picus_statement_lineage', { connectionId, sql });
}
