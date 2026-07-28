/**
 * Picus schema — what the active connection's database contains.
 *
 * Split out of the project store on purpose: the **project** is the script
 * repository on disk, the **schema** is what a live database says about itself.
 * They meet only in the generator, and conflating them was already leaking (the
 * generator asked the project for column types that come from a connection).
 *
 * The cache is invalidated by hand, never on a timer: a schema that silently
 * reloads under you while you are writing DML from it is worse than a stale one
 * you know is stale.
 *
 * Two granularities, mirroring the backend. The snapshot is the tree — every
 * relation with its columns, cheap on a schema with hundreds of tables. Constraints
 * and indexes arrive per relation through {@link detail}, paid for only when a tab
 * actually opens.
 */

import type { SchemaSnapshot, SequenceInfo, TableInfo, TriggerInfo } from '$lib/types/picus';
import { readSchema, tableDetail } from '$lib/ipc/picus/db';

const EMPTY: SchemaSnapshot = { tables: [], views: [], sequences: [], triggers: [] };

function createSchemaStore() {
  let snapshot = $state<SchemaSnapshot>({ ...EMPTY });
  /** Which connection the snapshot describes — '' when nothing is loaded. */
  let connectionId = $state('');
  let loadedAt = $state<string | null>(null);
  let loading = $state(false);
  let error = $state('');

  /** Tables and views together — the things that have columns and rows. */
  const relations = $derived<TableInfo[]>([...snapshot.tables, ...snapshot.views]);

  /** Replace a relation in place once its full detail has been read. */
  function merge(detail: TableInfo) {
    const list = detail.kind === 'view' ? snapshot.views : snapshot.tables;
    const i = list.findIndex((t) => t.name === detail.name);
    if (i >= 0) list[i] = detail;
  }

  return {
    get tables() { return snapshot.tables; },
    get views() { return snapshot.views; },
    get sequences() { return snapshot.sequences; },
    get triggers() { return snapshot.triggers; },
    get relations() { return relations; },
    get connectionId() { return connectionId; },
    get loadedAt() { return loadedAt; },
    get loading() { return loading; },
    get error() { return error; },

    /** Total object count — what the connection row reports at a glance. */
    get objectCount() {
      return snapshot.tables.length + snapshot.views.length
        + snapshot.sequences.length + snapshot.triggers.length;
    },

    /** A table or a view by name. Case-insensitive: PostgreSQL folds, Oracle shouts. */
    relation(name: string): TableInfo | null {
      const upper = name.toUpperCase();
      return relations.find((t) => t.name.toUpperCase() === upper) ?? null;
    },

    /** Only real tables — the generator writes DML, and a view is not writable. */
    table(name: string): TableInfo | null {
      const upper = name.toUpperCase();
      return snapshot.tables.find((t) => t.name.toUpperCase() === upper) ?? null;
    },

    /** Case-insensitive for the same reason as `relation` — these two used to be
     *  exact-match while every one of their siblings folded, so a sequence looked
     *  up by a caller that spelled it differently from the server simply did not
     *  exist. Same engines, same rule: PostgreSQL folds, Oracle shouts. */
    sequence(name: string): SequenceInfo | null {
      const upper = name.toUpperCase();
      return snapshot.sequences.find((s) => s.name.toUpperCase() === upper) ?? null;
    },

    trigger(name: string): TriggerInfo | null {
      const upper = name.toUpperCase();
      return snapshot.triggers.find((t) => t.name.toUpperCase() === upper) ?? null;
    },

    /** Triggers attached to one table — shown on that table's structure tab. */
    triggersOf(table: string): TriggerInfo[] {
      const upper = table.toUpperCase();
      return snapshot.triggers.filter((t) => t.table.toUpperCase() === upper);
    },

    /**
     * Foreign keys pointing AT a table — the other half of its relationships.
     *
     * Only as complete as the details already loaded: the snapshot carries no
     * constraints, so a table whose tab has never been opened contributes nothing
     * here. That is a deliberate trade against reading every constraint in the
     * database up front.
     */
    incomingForeignKeys(table: string) {
      const upper = table.toUpperCase();
      return snapshot.tables.flatMap((t) =>
        (t.foreignKeys ?? [])
          .filter((fk) => fk.referencedTable.toUpperCase() === upper)
          .map((fk) => ({ from: t.name, fk })),
      );
    },

    /** Forget everything — on disconnect, so the tree can't show a dead schema. */
    clear() {
      snapshot = { ...EMPTY };
      connectionId = '';
      loadedAt = null;
      error = '';
    },

    /** Read the catalogue of an open connection. */
    async load(id: string) {
      if (!id) {
        this.clear();
        return;
      }
      loading = true;
      try {
        snapshot = await readSchema(id);
        connectionId = id;
        loadedAt = new Date().toTimeString().slice(0, 5);
        error = '';
      } catch (e) {
        snapshot = { ...EMPTY };
        error = String(e);
      } finally {
        loading = false;
      }
    },

    /** Re-read the catalogue of whatever is currently loaded. */
    async refresh() {
      if (connectionId) await this.load(connectionId);
    },

    /**
     * The full detail of one relation — constraints, indexes, or a view's SELECT.
     * Merged into the snapshot so a second open is instant.
     */
    async detail(name: string): Promise<TableInfo | null> {
      if (!connectionId) return null;
      try {
        const full = await tableDetail(connectionId, name);
        merge(full);
        return full;
      } catch {
        return this.relation(name);
      }
    },
  };
}

export const schemaStore = createSchemaStore();
