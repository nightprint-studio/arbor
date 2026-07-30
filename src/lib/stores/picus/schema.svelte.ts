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

import type { SchemaSnapshot, SequenceInfo, TableInfo, TriggerDetail, TriggerInfo } from '$lib/types/picus';
import { readSchema, tableDetail, triggerDetail } from '$lib/ipc/picus/db';

const EMPTY: SchemaSnapshot = { tables: [], views: [], sequences: [], triggers: [] };

function createSchemaStore() {
  let snapshot = $state<SchemaSnapshot>({ ...EMPTY });
  /** Which connection the snapshot describes — '' when nothing is loaded. */
  let connectionId = $state('');
  let loadedAt = $state<string | null>(null);
  let loading = $state(false);
  let error = $state('');
  /**
   * The connection whose catalogue is being read right now — '' when none is.
   *
   * Deliberately not `$state`: it guards against a *second* call, and a reactive
   * read of it inside the effects that call `load` would be one more dependency
   * able to re-trigger them.
   */
  let reading = '';

  /** Tables and views together — the things that have columns and rows. */
  const relations = $derived<TableInfo[]>([...snapshot.tables, ...snapshot.views]);

  /**
   * Trigger definitions already read, by connection and name.
   *
   * Kept apart from the snapshot rather than merged into it: a definition is not
   * part of what a catalogue read returns, so folding it in would make the
   * snapshot's contents depend on which tabs somebody happened to open. Cleared
   * with the snapshot, since a definition belongs to the connection it came from.
   */
  let definitions = $state<Record<string, TriggerDetail>>({});

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
      definitions = {};
      connectionId = '';
      loadedAt = null;
      error = '';
      // Abandons a read still in flight: its answer describes a connection nobody
      // is looking at any more, and the `reading !== id` checks drop it when it
      // lands. The spinner goes with it.
      reading = '';
      loading = false;
    },

    /**
     * Read the catalogue of an open connection.
     *
     * ## One read at a time, per connection
     *
     * The callers are an effect. `connectionId` is only set **after** the read
     * returns, so for as long as one is in flight the condition that started it is
     * still true — and every re-render of the connection list started another. On a
     * small schema that is invisible; on a real one it is a dozen catalogue queries
     * pipelined down one connection, each waiting for the ones before it, and the
     * last of them takes long enough that the studio looks like it has stopped.
     *
     * That is the shape of the bug this guard exists for, and it is worth stating
     * plainly: **the read was not slow, it was running several times.** A second ask
     * for a connection already being read is the same question, so it is dropped
     * rather than queued.
     */
    async load(id: string) {
      if (!id) {
        this.clear();
        return;
      }
      if (reading === id) return;
      reading = id;
      loading = true;
      try {
        const read = await readSchema(id);
        // The active connection can change while a large schema is being read.
        // Landing the answer anyway would file one database's catalogue under
        // another's name, which is the kind of quiet wrongness that gets a DELETE
        // written against the wrong server.
        if (reading !== id) return;
        snapshot = read;
        connectionId = id;
        loadedAt = new Date().toTimeString().slice(0, 5);
        error = '';
      } catch (e) {
        if (reading !== id) return;
        snapshot = { ...EMPTY };
        error = String(e);
      } finally {
        if (reading === id) {
          reading = '';
          loading = false;
        }
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

    /** A trigger definition already read, or `null` — never a fetch. */
    triggerDefinition(name: string): TriggerDetail | null {
      return definitions[`${connectionId}::${name.toUpperCase()}`] ?? null;
    },

    /**
     * Read one trigger's definition, once.
     *
     * Held per connection and name, so re-opening the tab is instant and switching
     * database cannot show the previous server's answer. A failure is not cached:
     * the next open asks again rather than remembering that it did not work.
     */
    async loadTriggerDefinition(name: string): Promise<TriggerDetail | null> {
      if (!connectionId) return null;
      const key = `${connectionId}::${name.toUpperCase()}`;
      const held = definitions[key];
      if (held) return held;
      try {
        const detail = await triggerDetail(connectionId, name);
        definitions = { ...definitions, [key]: detail };
        return detail;
      } catch {
        return null;
      }
    },
  };
}

export const schemaStore = createSchemaStore();
