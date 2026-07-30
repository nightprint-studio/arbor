/**
 * Picus query plans — `EXPLAIN`, and the separate, explicit `EXPLAIN ANALYZE`.
 *
 * Its own module rather than another section of `db.ts` because it is its own
 * thing: everything here is about a statement the user is **not** running, which
 * is the opposite of what every call in `db.ts` does — with the one exception that
 * makes this dangerous and is therefore named in every doc comment below.
 *
 * ## Estimate versus measurement
 *
 * Without `analyze` the server plans the statement and executes nothing: every
 * number is a guess. With `analyze` the server **runs the statement** and reports
 * what actually happened. The two are not display options of one another, and the
 * answer carries {@link QueryPlan.analyzed} so nothing downstream has to remember
 * which it asked for.
 *
 * The backend refuses `analyze` for anything that is not a read, and on a read-only
 * connection refuses it as a write. That refusal is the engine's, not this layer's:
 * a client-side guess about what counts as a write would only ever be a second,
 * weaker opinion.
 */

import { picus } from '../rpc';

/** One step of the plan. Flat, with {@link depth} carrying the tree. */
export interface PlanNode {
  /** Indentation depth; 0 is the root. */
  depth: number;
  /** `Seq Scan`, `Index Scan using orders_pkey`, `Hash Join`, … */
  label: string;
  /** The relation this node reads, when it reads one. */
  relation: string | null;
  /** Estimated total cost, in the engine's own units. */
  cost: number | null;
  /** Estimated rows out — **per loop**, like `actualRows` beside it. */
  rows: number | null;
  /** Rows actually produced. Only present on an analysed plan. */
  actualRows: number | null;
  /** Time actually spent. Only present on an analysed plan. */
  actualMs: number | null;
  /** Filters, sort keys, index conditions, buffer accounting — the server's words. */
  detail: string[];
  /** A remark worth surfacing, in prose. Absent far more often than present. */
  warning: string | null;
}

/** A plan, as text and as structure. */
export interface QueryPlan {
  /** The engine's own output, verbatim — what gets pasted into a ticket. */
  text: string;
  /** The same plan, one entry per node, in execution-tree order. */
  nodes: PlanNode[];
  /**
   * The statement was executed. **`false` means every number here is an estimate**,
   * and the interface must say so — this is the single most important field on the
   * screen it draws.
   */
  analyzed: boolean;
  /** Total estimated cost of the root node. */
  totalCost: number | null;
  /** Wall time of the whole plan, when it was analysed. */
  actualMs: number | null;
  /** How long producing the plan took. */
  elapsedMs: number;
}

/**
 * Ask for a statement's plan.
 *
 * `analyze` **runs the statement**. Never send it because a plan would be more
 * useful with real numbers in it — send it because the user asked for it, on a
 * statement they are willing to have executed. Omitted, it is false.
 */
export function explainQuery(
  connectionId: string,
  sql: string,
  analyze = false,
): Promise<QueryPlan> {
  return picus('picus_explain', { connectionId, sql, analyze });
}
