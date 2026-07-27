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
 * MOCK: one shared snapshot for every connection until `picus-be` reads real
 * catalogues. Keyed lookups already take the shape the real thing will have.
 */

import type { SchemaSnapshot, SequenceInfo, TableInfo, TriggerInfo } from '$lib/types/picus';
import { MOCK_SEQUENCES, MOCK_TABLES, MOCK_TRIGGERS, MOCK_VIEWS } from '$lib/ipc/picus/mock';

function createSchemaStore() {
  let snapshot = $state<SchemaSnapshot>({
    tables: MOCK_TABLES,
    views: MOCK_VIEWS,
    sequences: MOCK_SEQUENCES,
    triggers: MOCK_TRIGGERS,
  });
  let loadedAt = $state<string | null>(null);
  let loading = $state(false);

  /** Tables and views together — the things that have columns and rows. */
  const relations = $derived<TableInfo[]>([...snapshot.tables, ...snapshot.views]);

  return {
    get tables() { return snapshot.tables; },
    get views() { return snapshot.views; },
    get sequences() { return snapshot.sequences; },
    get triggers() { return snapshot.triggers; },
    get relations() { return relations; },
    get loadedAt() { return loadedAt; },
    get loading() { return loading; },

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

    sequence(name: string): SequenceInfo | null {
      return snapshot.sequences.find((s) => s.name === name) ?? null;
    },

    trigger(name: string): TriggerInfo | null {
      return snapshot.triggers.find((t) => t.name === name) ?? null;
    },

    /** Triggers attached to one table — shown on that table's structure tab. */
    triggersOf(table: string): TriggerInfo[] {
      const upper = table.toUpperCase();
      return snapshot.triggers.filter((t) => t.table.toUpperCase() === upper);
    },

    /** Foreign keys pointing AT a table — the other half of its relationships. */
    incomingForeignKeys(table: string) {
      const upper = table.toUpperCase();
      return snapshot.tables.flatMap((t) =>
        (t.foreignKeys ?? [])
          .filter((fk) => fk.referencedTable.toUpperCase() === upper)
          .map((fk) => ({ from: t.name, fk })),
      );
    },

    /** Re-read the catalogue. MOCK: just re-stamps the load time. */
    refresh() {
      loading = true;
      setTimeout(() => {
        loading = false;
        loadedAt = new Date().toTimeString().slice(0, 5);
      }, 350);
    },
  };
}

export const schemaStore = createSchemaStore();
