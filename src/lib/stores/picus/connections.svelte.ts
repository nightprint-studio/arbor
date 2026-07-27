/**
 * Picus connections — the simultaneous database sessions and which one is
 * "active" (the connection a newly opened query/table tab binds to).
 *
 * Colour is the identification mechanism: each connection owns a slot in the
 * shared workspace palette (`--ws-color-N`), shown on the sidebar row, on every
 * tab bound to it, and in the status bar.
 *
 * Read-only is enforced by the **server** — a read-only session is opened in a
 * read-only transaction mode, so the refusal holds for a pasted script too. The
 * flag here only greys the affordances on top of that.
 *
 * Configured and connected are different things, and this store keeps them apart:
 * a connection is listed, editable and complete with no server reachable. That is
 * not an edge case — a project routinely has an Oracle branch nobody can connect
 * to, and Picus maintains its scripts anyway.
 */

import type { Connection, Dialect } from '$lib/types/picus';
import {
  type ConnectionRow,
  type ConnectionSpec,
  connect as rpcConnect,
  deleteConnection,
  disconnect as rpcDisconnect,
  listConnections,
  readDbVersion,
  saveConnection,
  storeSecret,
} from '$lib/ipc/picus/db';
import { picusSettingsStore } from './settings.svelte';

/** Resolve a connection's palette slot to the CSS variable holding its colour. */
export function connectionColorVar(conn: Pick<Connection, 'colorIdx'> | null | undefined): string {
  return `var(--ws-color-${conn?.colorIdx ?? 0})`;
}

/**
 * Project the backend row onto the shape the UI renders.
 *
 * The one asymmetry worth naming: the backend calls it `engine`, the UI calls it
 * `dialect`. Same concept — the engine a session speaks and the dialect a folder is
 * written in must never drift apart — and this is the single place the names meet.
 */
function toUi(row: ConnectionRow, dbVersion: string): Connection {
  return {
    id: row.id,
    name: row.name,
    alias: row.alias,
    dialect: row.engine,
    schema: row.schema,
    // The UI shows one address; the spec keeps the three parts apart, because the
    // engines spell that string differently and a single box invites the wrong one.
    host: `${row.host}${row.port ? `:${row.port}` : ''}${row.database ? `/${row.database}` : ''}`,
    state: row.state,
    dbVersion,
    colorIdx: row.colorIdx,
    readOnly: row.readOnly,
  };
}

function createConnectionsStore() {
  /** The rows as the backend reports them — the source the modal edits. */
  let rows = $state<ConnectionRow[]>([]);
  /** Version-table readings, keyed by connection id. Filled after a connect. */
  let versions = $state<Record<string, string>>({});
  let activeId = $state<string>('');
  let loading = $state(false);
  /** Set when the backend can't be reached, so the panel can say so. */
  let error = $state('');

  const connections = $derived<Connection[]>(rows.map((r) => toUi(r, versions[r.id] ?? '')));
  const active = $derived(connections.find((c) => c.id === activeId) ?? null);

  /**
   * Read the application version from the project's version table.
   *
   * Best-effort by nature: a database that isn't this project's simply has no such
   * table, and the empty string is the honest answer — never a guess, never an
   * error the user has to dismiss.
   */
  async function loadVersion(id: string) {
    const v = picusSettingsStore.versionTable;
    if (!v.table || !v.versionColumn) return;
    try {
      versions = { ...versions, [id]: await readDbVersion(id, v.table, v.versionColumn, v.filter) };
    } catch {
      versions = { ...versions, [id]: '' };
    }
  }

  return {
    get connections() { return connections; },
    get rows() { return rows; },
    get activeId() { return activeId; },
    get active() { return active; },
    get loading() { return loading; },
    get error() { return error; },

    /** The dialect statements typed in this window default to. */
    get activeDialect(): Dialect { return active?.dialect ?? 'postgres'; },

    byId(id: string | undefined): Connection | null {
      if (!id) return null;
      return connections.find((c) => c.id === id) ?? null;
    },

    /** The editable spec behind a row — what the connection modal opens. */
    specById(id: string): ConnectionRow | null {
      return rows.find((r) => r.id === id) ?? null;
    },

    setActive(id: string) {
      if (rows.some((c) => c.id === id)) activeId = id;
    },

    /** Cycle to the next connection — the keyboard path for switching database. */
    cycle(step = 1) {
      if (rows.length < 2) return;
      const i = rows.findIndex((c) => c.id === activeId);
      const next = (i + step + rows.length) % rows.length;
      activeId = rows[next].id;
    },

    /** Read the configured connections and their current state. */
    async load() {
      loading = true;
      try {
        rows = await listConnections();
        error = '';
        if (!rows.some((r) => r.id === activeId)) activeId = rows[0]?.id ?? '';
      } catch (e) {
        // Backend not up yet: keep whatever is on screen rather than blanking the
        // panel, and let the `picus-be-up` listener retry.
        error = String(e);
      } finally {
        loading = false;
      }
    },

    /** Open a session, then read its version-table stamp. */
    async connect(id: string) {
      const row = rows.find((r) => r.id === id);
      if (row) row.state = 'connecting';
      try {
        const status = await rpcConnect(id);
        if (row) {
          row.state = status.state;
          row.serverVersion = status.serverVersion;
        }
        await loadVersion(id);
        return '';
      } catch (e) {
        if (row) row.state = 'disconnected';
        return String(e);
      }
    },

    async disconnect(id: string) {
      await rpcDisconnect(id);
      const row = rows.find((r) => r.id === id);
      if (row) {
        row.state = 'disconnected';
        row.serverVersion = '';
      }
    },

    /**
     * Create or update a connection.
     *
     * `password` is `undefined` when the form was left untouched — meaning "keep
     * whatever is stored". An empty string is a deliberate clear, which is why the
     * two are not collapsed: treating "didn't type" as "delete the password" is
     * exactly how a saved credential disappears on an unrelated edit.
     */
    async save(spec: ConnectionSpec, password?: string) {
      await saveConnection(spec);
      if (password !== undefined) await storeSecret(spec.id, password);
      await this.load();
      activeId = spec.id;
    },

    async remove(id: string) {
      await deleteConnection(id);
      rows = rows.filter((c) => c.id !== id);
      if (activeId === id) activeId = rows[0]?.id ?? '';
    },

    /** Re-read the version stamp of every open connection (after a settings change). */
    async refreshVersions() {
      await Promise.all(rows.filter((r) => r.state !== 'disconnected').map((r) => loadVersion(r.id)));
    },
  };
}

export const connectionsStore = createConnectionsStore();
