/**
 * The schema, as the editor is allowed to see it.
 *
 * Every SQL feature in this folder reads facts through this adapter rather than
 * touching `schemaStore` directly, for one reason: **the difference between "this
 * table does not exist" and "we have not looked yet" has to be represented once,
 * in one place, and be impossible to forget.** A diagnostic engine that treats an
 * unread schema as an empty one paints the whole buffer red the moment you open a
 * file, and the feature is switched off within the minute — which costs more than
 * never having written it.
 *
 * So {@link SchemaView.known} is the gate, and it is deliberately conservative. It
 * is true only when *all* of the following hold:
 *
 *  • the editor is bound to a connection at all (a script file with no database
 *    open is not evidence of anything);
 *  • a catalogue has been read for **that** connection — asked for by id, so a tab
 *    bound to one connection is never answered from another's;
 *  • that catalogue is not mid-load, carries no error, and is not empty.
 *
 * The middle one used to read "the store's single snapshot happens to describe this
 * connection", which meant only the *selected* connection had any intelligence at
 * all: a query tab on any other one lost completion, expansion and validation
 * silently. The store holds several now, and this asks it for the right one.
 *
 * When `known` is false the object still answers every question — it simply
 * answers "I don't know", and every caller degrades to keywords and buffer text.
 */

import type { Column, Connection, SequenceInfo, TableInfo } from '$lib/types/picus';
import { connectionsStore } from '$lib/stores/picus/connections.svelte';
import { schemaStore } from '$lib/stores/picus/schema.svelte';

export interface SchemaView {
  /** Is there a schema snapshot that describes this editor's connection? */
  known: boolean;
  /** Server-side schema the connection is pinned to — `''` when unbound. */
  schemaName: string;
  /** The connection refuses writes. False whenever there is no connection. */
  readOnly: boolean;
  connection: Connection | null;

  relations: TableInfo[];
  sequences: SequenceInfo[];

  /** A table or a view by name, case-insensitively. */
  relation(name: string): TableInfo | null;
  sequence(name: string): SequenceInfo | null;
  column(relation: string, column: string): Column | null;
}

const NOTHING: TableInfo[] = [];
const NO_SEQUENCES: SequenceInfo[] = [];

function ci(a: string, b: string): boolean {
  return a.toUpperCase() === b.toUpperCase();
}

/**
 * Build the view for an editor bound to `connectionId`.
 *
 * Cheap and allocation-light: it reads the store's getters (so a Svelte `$derived`
 * that calls it re-runs when the schema changes) and closes over the arrays it
 * already holds. Call it per computation rather than caching it — a cached view
 * would be exactly the stale snapshot this module exists to avoid.
 */
export function schemaViewFor(connectionId: string | undefined): SchemaView {
  const connection = connectionsStore.byId(connectionId);
  const catalogue = schemaStore.of(connectionId);
  const describesThisConnection = catalogue.loaded && !catalogue.loading;
  const relations = describesThisConnection ? catalogue.relations : NOTHING;
  const sequences = describesThisConnection ? catalogue.sequences : NO_SEQUENCES;
  // An empty catalogue is indistinguishable from an unread one, and guessing which
  // it is would be guessing about the only thing that must not be guessed.
  const known = describesThisConnection && relations.length > 0;

  return {
    known,
    schemaName: connection?.schema ?? '',
    readOnly: !!connection?.readOnly,
    connection,
    relations,
    sequences,

    relation(name: string) {
      if (!name) return null;
      return relations.find((r) => ci(r.name, name)) ?? null;
    },
    sequence(name: string) {
      if (!name) return null;
      return sequences.find((s) => ci(s.name, name)) ?? null;
    },
    column(relation: string, column: string) {
      const rel = this.relation(relation);
      return rel?.columns.find((c) => ci(c.name, column)) ?? null;
    },
  };
}

/**
 * Does a schema-qualified reference point somewhere we have a catalogue for?
 *
 * `ALTRO.CLIENTI` when the session is pinned to `PUBLIC` is not an unknown table —
 * it is a table in a schema nobody read. Silence is the only correct answer.
 */
export function inReadableSchema(view: SchemaView, schema: string): boolean {
  return !schema || ci(schema, view.schemaName);
}

/** Relations whose detail has been asked for, keyed `<connection>::<TABLE>`. */
const detailAttempted = new Set<string>();

/**
 * Make sure a relation's constraints are loaded, once.
 *
 * The snapshot deliberately carries no constraints — reading every foreign key in
 * a database up front is not worth it — so the two features that need them (a
 * column's FK target on hover, an FK-implied join predicate in ghost text) pull
 * the detail in on demand. Attempted names are remembered rather than retried: a
 * table that genuinely has no foreign keys must not turn every hover into a round
 * trip.
 */
export async function ensureRelationDetail(
  connectionId: string | undefined, name: string,
): Promise<TableInfo | null> {
  if (!connectionId || !name) return null;
  // Against that connection's own catalogue, not the selected one's — a tab bound
  // elsewhere would otherwise pull the constraints of a same-named table on a
  // different database, which is the worst kind of right-looking answer.
  const view = schemaViewFor(connectionId);
  const current = view.relation(name);
  if (!current) return null;
  const key = `${connectionId}::${current.name.toUpperCase()}`;
  if (current.foreignKeys !== undefined || detailAttempted.has(key)) return current;
  detailAttempted.add(key);
  try {
    return await schemaStore.detail(current.name, connectionId);
  } catch {
    return current;
  }
}
