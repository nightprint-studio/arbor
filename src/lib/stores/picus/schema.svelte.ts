/**
 * Picus schema — what each open connection's database contains.
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
 *
 * ## Several connections at once, and why that matters
 *
 * This used to hold exactly **one** catalogue, the selected connection's. Every
 * consumer read it as "the schema", so a query tab bound to any other connection
 * had no tables, no views, and therefore no completion, no abbreviation expansion
 * and no live validation — while nothing on screen explained why. The editor simply
 * stopped knowing things, and switching tabs back and forth re-read hundreds of
 * relations each way.
 *
 * So catalogues are held **by connection**, up to {@link MAX_CACHED}, and callers
 * that know which connection they are asking about say so ({@link of}). The bare
 * accessors are unchanged and still describe the *selected* connection — the object
 * tree and the generator are about what is selected, and rewriting them to carry an
 * id would be churn for no answer they need.
 *
 * ## Why a small cache and not all of them
 *
 * A catalogue is hundreds of relations with their columns, and it is a Svelte deep
 * proxy. Holding every connection a session ever opened would grow without bound
 * for the sake of a database nobody has looked at since. Three is enough for the
 * thing people actually do — a tab on production beside one on test, and the
 * repository's own — and the eviction is least-recently-used, so the pair you are
 * moving between never falls out.
 *
 * The selected connection is never evicted: it is the one on screen.
 */

import type { SchemaSnapshot, SequenceInfo, TableInfo, TriggerDetail, TriggerInfo } from '$lib/types/picus';
import { readSchema, tableDetail, triggerDetail } from '$lib/ipc/picus/db';

const EMPTY: SchemaSnapshot = { tables: [], views: [], sequences: [], triggers: [] };

/** How many connections' catalogues are held at once. See the header note. */
export const MAX_CACHED = 3;

/** One connection's catalogue, and how it got here. */
interface Catalogue {
  snapshot: SchemaSnapshot;
  /** `HH:MM` of the read, or `null` when it failed. */
  loadedAt: string | null;
  /** Why the read failed, in the server's words. Empty on success. */
  error: string;
}

/** What one connection's catalogue answers, whether or not it is the selected one. */
export interface CatalogueView {
  /** A catalogue has been read for this connection and it is not an error. */
  loaded: boolean;
  /** A read is in flight for it right now. */
  loading: boolean;
  error: string;
  loadedAt: string | null;
  tables: TableInfo[];
  views: TableInfo[];
  sequences: SequenceInfo[];
  triggers: TriggerInfo[];
  /** Tables and views together — the things that have columns and rows. */
  relations: TableInfo[];
}

const NO_TABLES: TableInfo[] = [];
const NO_SEQUENCES: SequenceInfo[] = [];
const NO_TRIGGERS: TriggerInfo[] = [];

const NOTHING: CatalogueView = {
  loaded: false,
  loading: false,
  error: '',
  loadedAt: null,
  tables: NO_TABLES,
  views: NO_TABLES,
  sequences: NO_SEQUENCES,
  triggers: NO_TRIGGERS,
  relations: NO_TABLES,
};

function createSchemaStore() {
  /** Every catalogue held, by connection id. */
  let held = $state<Record<string, Catalogue>>({});
  /** Which connection the bare accessors describe — '' when none is selected. */
  let selected = $state('');
  /** Connections with a read in flight. Reactive: the tree draws a spinner off it. */
  let busy = $state<string[]>([]);
  /**
   * Least-recently-used order, oldest first.
   *
   * Deliberately **not** `$state`: it is eviction bookkeeping, and a reactive read
   * of it would make every touch of a catalogue re-run whatever was looking at one.
   */
  let recency: string[] = [];
  /**
   * Connections being read right now — the same set as `busy`, kept separately and
   * non-reactively because it guards against a *second* call, and a reactive read
   * inside the effects that call `load` would be one more dependency able to
   * re-trigger them.
   */
  const reading = new Set<string>();

  /**
   * Trigger definitions already read, by connection and name.
   *
   * Kept apart from the catalogues rather than merged into them: a definition is
   * not part of what a catalogue read returns, so folding it in would make a
   * snapshot's contents depend on which tabs somebody happened to open. Dropped
   * with the connection they came from.
   */
  let definitions = $state<Record<string, TriggerDetail>>({});

  /** Note that `id` was just used, and drop whatever fell off the end. */
  function touch(id: string) {
    recency = recency.filter((c) => c !== id);
    recency.push(id);
    while (recency.length > MAX_CACHED) {
      // Never the selected one — it is what is on screen — and never one still
      // being read, whose answer would land in a slot nobody holds any more.
      const victim = recency.find((c) => c !== selected && !reading.has(c));
      if (!victim) break;
      recency = recency.filter((c) => c !== victim);
      drop(victim);
    }
  }

  /** Forget one connection's catalogue and everything keyed to it. */
  function drop(id: string) {
    if (!(id in held)) return;
    const { [id]: _gone, ...rest } = held;
    held = rest;
    const prefix = `${id}::`;
    definitions = Object.fromEntries(
      Object.entries(definitions).filter(([key]) => !key.startsWith(prefix)),
    );
  }

  function snapshotOf(id: string): SchemaSnapshot {
    return held[id]?.snapshot ?? EMPTY;
  }

  /** The selected connection's snapshot — what every bare accessor reads. */
  function current(): SchemaSnapshot {
    return snapshotOf(selected);
  }

  /**
   * Tables and views as one list.
   *
   * Computed on each call rather than memoised, and that is deliberate: `merge`
   * replaces a relation **in place** once its detail is read, which a cached flat
   * list would not see — it would go on holding the constraint-less copy for as
   * long as the catalogue lived. One spread of a few hundred references, at
   * completion and lint frequency, is not worth a staleness bug.
   */
  function relationsIn(snapshot: SchemaSnapshot): TableInfo[] {
    return [...snapshot.tables, ...snapshot.views];
  }

  function byName<T extends { name: string }>(list: T[], name: string): T | null {
    const upper = name.toUpperCase();
    return list.find((item) => item.name.toUpperCase() === upper) ?? null;
  }

  /** Replace a relation in place once its full detail has been read. */
  function merge(id: string, detail: TableInfo) {
    const snapshot = held[id]?.snapshot;
    if (!snapshot) return;
    const list = detail.kind === 'view' ? snapshot.views : snapshot.tables;
    const i = list.findIndex((t) => t.name === detail.name);
    if (i >= 0) list[i] = detail;
  }

  /**
   * Mark a connection as being read, or not.
   *
   * Writes **only on a change**. `forget` is reached from an effect that re-runs on
   * every render while a connection is down, and an unconditional assignment there
   * would hand a fresh array to every consumer of {@link of} each time — no loop,
   * because nothing that writes this also reads it, but a steady drip of
   * invalidations through the object tree and every editor's intelligence.
   */
  function markBusy(id: string, on: boolean) {
    if (busy.includes(id) === on) return;
    busy = on ? [...busy, id] : busy.filter((c) => c !== id);
  }

  return {
    // ── The selected connection, for everything that is about what is on screen ──

    get tables() { return current().tables; },
    get views() { return current().views; },
    get sequences() { return current().sequences; },
    get triggers() { return current().triggers; },
    get relations() { return relationsIn(current()); },
    get connectionId() { return selected; },
    get loadedAt() { return held[selected]?.loadedAt ?? null; },
    get loading() { return busy.includes(selected); },
    get error() { return held[selected]?.error ?? ''; },

    /** Total object count — what the connection row reports at a glance. */
    get objectCount() {
      const snapshot = current();
      return snapshot.tables.length + snapshot.views.length
        + snapshot.sequences.length + snapshot.triggers.length;
    },

    /** A table or a view by name. Case-insensitive: PostgreSQL folds, Oracle shouts. */
    relation(name: string): TableInfo | null {
      return byName(relationsIn(current()), name);
    },

    /** Only real tables — the generator writes DML, and a view is not writable. */
    table(name: string): TableInfo | null {
      return byName(current().tables, name);
    },

    /** Case-insensitive for the same reason as `relation` — these two used to be
     *  exact-match while every one of their siblings folded, so a sequence looked
     *  up by a caller that spelled it differently from the server simply did not
     *  exist. Same engines, same rule: PostgreSQL folds, Oracle shouts. */
    sequence(name: string): SequenceInfo | null {
      return byName(current().sequences, name);
    },

    trigger(name: string): TriggerInfo | null {
      return byName(current().triggers, name);
    },

    /** Triggers attached to one table — shown on that table's structure tab. */
    triggersOf(table: string): TriggerInfo[] {
      const upper = table.toUpperCase();
      return current().triggers.filter((t) => t.table.toUpperCase() === upper);
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
      return current().tables.flatMap((t) =>
        (t.foreignKeys ?? [])
          .filter((fk) => fk.referencedTable.toUpperCase() === upper)
          .map((fk) => ({ from: t.name, fk })),
      );
    },

    // ── Any connection, for callers that know which one they mean ───────────────

    /**
     * One connection's catalogue, whether or not it is the selected one.
     *
     * Always answers. A connection nothing has been read for reports `loaded:
     * false` with empty lists — which is the distinction every caller has to make
     * anyway, between "this database has no tables" and "we have not looked".
     */
    of(id: string | undefined | null): CatalogueView {
      if (!id) return NOTHING;
      const entry = held[id];
      const loading = busy.includes(id);
      if (!entry) return { ...NOTHING, loading };
      return {
        loaded: !entry.error,
        loading,
        error: entry.error,
        loadedAt: entry.loadedAt,
        tables: entry.snapshot.tables,
        views: entry.snapshot.views,
        sequences: entry.snapshot.sequences,
        triggers: entry.snapshot.triggers,
        relations: relationsIn(entry.snapshot),
      };
    },

    /** Point the bare accessors at a connection. Reading is a separate question. */
    select(id: string) {
      if (selected === id) return;
      selected = id;
      if (id && id in held) touch(id);
    },

    /**
     * Read `id`'s catalogue unless it is already held or already being read.
     *
     * The idempotent door. Callers are effects and fire on every re-render, so
     * "already have it" has to be cheap and has to be here rather than at each of
     * them.
     */
    async ensure(id: string): Promise<void> {
      if (!id || id in held || reading.has(id)) return;
      await this.load(id);
    },

    /** Forget one connection's catalogue — on disconnect, so the tree can't show a
     *  dead schema. Other connections are untouched. */
    forget(id: string) {
      if (!id) return;
      recency = recency.filter((c) => c !== id);
      drop(id);
      // Abandons a read still in flight: its answer describes a connection nobody
      // is looking at any more, and the `reading` check drops it when it lands.
      reading.delete(id);
      markBusy(id, false);
      if (selected === id) selected = '';
    },

    /** Forget everything — every connection, every definition. */
    clear() {
      for (const id of Object.keys(held)) this.forget(id);
      held = {};
      definitions = {};
      recency = [];
      reading.clear();
      busy = [];
      selected = '';
    },

    /**
     * Read the catalogue of an open connection.
     *
     * ## One read at a time, per connection
     *
     * The callers are effects, so for as long as one read is in flight the
     * condition that started it is still true — and every re-render of the
     * connection list started another. On a small schema that is invisible; on a
     * real one it is a dozen catalogue queries pipelined down one connection, each
     * waiting for the ones before it, and the last of them takes long enough that
     * the studio looks like it has stopped.
     *
     * That is the shape of the bug this guard exists for, and it is worth stating
     * plainly: **the read was not slow, it was running several times.** A second ask
     * for a connection already being read is the same question, so it is dropped
     * rather than queued. Two *different* connections may read at once — they are
     * different sockets and different questions.
     */
    async load(id: string): Promise<void> {
      if (!id) return;
      if (reading.has(id)) return;
      reading.add(id);
      markBusy(id, true);
      try {
        const read = await readSchema(id);
        // The connection can be dropped while a large schema is being read. Landing
        // the answer anyway would file one database's catalogue under a name nobody
        // holds any more.
        if (!reading.has(id)) return;
        held = {
          ...held,
          [id]: { snapshot: read, loadedAt: new Date().toTimeString().slice(0, 5), error: '' },
        };
        touch(id);
      } catch (e) {
        if (!reading.has(id)) return;
        held = { ...held, [id]: { snapshot: { ...EMPTY }, loadedAt: null, error: String(e) } };
        touch(id);
      } finally {
        reading.delete(id);
        markBusy(id, false);
      }
    },

    /** Re-read a catalogue that is already held — the selected one by default. */
    async refresh(id: string = selected): Promise<void> {
      if (!id) return;
      reading.delete(id);
      await this.load(id);
    },

    /**
     * The full detail of one relation — constraints, indexes, or a view's SELECT.
     * Merged into the catalogue so a second open is instant.
     */
    async detail(name: string, id: string = selected): Promise<TableInfo | null> {
      if (!id) return null;
      try {
        const full = await tableDetail(id, name);
        merge(id, full);
        return full;
      } catch {
        return byName(relationsIn(snapshotOf(id)), name);
      }
    },

    /** A trigger definition already read, or `null` — never a fetch. */
    triggerDefinition(name: string, id: string = selected): TriggerDetail | null {
      return definitions[`${id}::${name.toUpperCase()}`] ?? null;
    },

    /**
     * Read one trigger's definition, once.
     *
     * Held per connection and name, so re-opening the tab is instant and switching
     * database cannot show the previous server's answer. A failure is not cached:
     * the next open asks again rather than remembering that it did not work.
     */
    async loadTriggerDefinition(name: string, id: string = selected): Promise<TriggerDetail | null> {
      if (!id) return null;
      const key = `${id}::${name.toUpperCase()}`;
      const already = definitions[key];
      if (already) return already;
      try {
        const detail = await triggerDetail(id, name);
        definitions = { ...definitions, [key]: detail };
        return detail;
      } catch {
        return null;
      }
    },
  };
}

export const schemaStore = createSchemaStore();
