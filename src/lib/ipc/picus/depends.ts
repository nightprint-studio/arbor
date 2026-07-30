/**
 * Picus dependencies — what an object needs, and what needs it.
 *
 * One call, one answer: the whole graph of the connected schema. Not a per-object
 * question, deliberately — "what does `ORDINI` depend on" is one hop of a walk the
 * user then continues, and asking the server again at every hop would put a round
 * trip behind every chevron. The graph is a fixed handful of catalogue queries whatever the
 * schema's size; the store above this holds it per connection and invalidates it by
 * hand.
 *
 * The shapes mirror `picus_db_api::depends` field for field — the backend
 * serialises camelCase precisely so there is no translation layer here.
 */

import { picus } from '../rpc';

/**
 * Why one object depends on another.
 *
 * The reason is carried on every edge rather than inferred, because "`ORDINI` needs
 * `CLIENTI`" is a fact with several possible causes and a foreign key, a view body
 * and a trigger are three different things to do about it.
 */
export type DependencyKind =
  /** A foreign key: `from` references `to`. */
  | 'foreignKey'
  /** `from` is a view (or a materialised view) whose body reads `to`. */
  | 'viewSource'
  /** `from` is a trigger installed on table `to`. */
  | 'triggerTable'
  /** `from` is a trigger that fires routine `to`. */
  | 'triggerRoutine'
  /** `from` has a column whose default draws from sequence `to`, or an identity. */
  | 'sequenceDefault'
  /** `from` is a routine whose body reads or writes `to`. */
  | 'routineBody';

/** One object in the graph. */
export interface DependencyNode {
  /**
   * The name edges refer to this node by.
   *
   * Unqualified for an object in the session's own schema — the spelling the
   * object tree uses. **Qualified** (`schema.name`) for one outside it, because
   * edges carry names and an `orders` in `audit` is not the `orders` here.
   */
  name: string;
  /** `table`, `view`, `sequence`, `trigger`, `function`, `procedure` — the schema
   *  browser's vocabulary, so one icon set serves both. */
  kind: string;
  /** Schema the object lives in; empty when it is the session's own. */
  schema: string;
}

/**
 * `from` needs `to`.
 *
 * The direction has one fixed meaning and it is the reason the graph exists: `to`
 * has to exist before `from` can. Sorting on it is what produces a creation order.
 */
export interface DependencyEdge {
  from: string;
  to: string;
  kind: DependencyKind;
  /** The specific thing that ties them — a constraint name, a column, a routine.
   *  Shown on the edge so "why?" needs no second question. */
  via: string | null;
}

/** The whole graph for one schema. */
export interface DependencyGraph {
  nodes: DependencyNode[];
  edges: DependencyEdge[];
  /**
   * What the engine could not work out — a routine whose body the catalogue never
   * analysed, a trigger whose function has gone.
   *
   * Reported rather than dropped, and shown rather than hidden: an order computed
   * from a graph that silently omitted what it did not understand is an order that
   * looks authoritative and is not.
   */
  unresolved: string[];
}

/** The dependency graph of an open connection's schema. */
export function dependencies(id: string): Promise<DependencyGraph> {
  return picus('picus_dependencies', { id });
}
