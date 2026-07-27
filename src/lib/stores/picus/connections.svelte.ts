/**
 * Picus connections — the simultaneous database sessions and which one is
 * "active" (the connection a newly opened query/table tab binds to).
 *
 * Colour is the identification mechanism: each connection owns a slot in the
 * shared workspace palette (`--ws-color-N`), shown on the sidebar row, on every
 * tab bound to it, and in the status bar. Read-only is enforced by the backend,
 * never by hiding buttons — the flag here only greys the affordances.
 *
 * MOCK: fed from `ipc/picus/mock` until `picus-be` serves connections.
 */

import type { Connection, Dialect } from '$lib/types/picus';
import { MOCK_CONNECTIONS } from '$lib/ipc/picus/mock';

/** Resolve a connection's palette slot to the CSS variable holding its colour. */
export function connectionColorVar(conn: Pick<Connection, 'colorIdx'> | null | undefined): string {
  return `var(--ws-color-${conn?.colorIdx ?? 0})`;
}

function createConnectionsStore() {
  let connections = $state<Connection[]>(MOCK_CONNECTIONS.map((c) => ({ ...c })));
  let activeId = $state<string>(MOCK_CONNECTIONS[0]?.id ?? '');

  const active = $derived(connections.find((c) => c.id === activeId) ?? null);

  return {
    get connections() { return connections; },
    get activeId() { return activeId; },
    get active() { return active; },
    /** The dialect statements typed in this window default to. */
    get activeDialect(): Dialect { return active?.dialect ?? 'oracle'; },

    byId(id: string | undefined): Connection | null {
      if (!id) return null;
      return connections.find((c) => c.id === id) ?? null;
    },

    setActive(id: string) {
      if (connections.some((c) => c.id === id)) activeId = id;
    },

    /** Cycle to the next connection — the keyboard path for switching database. */
    cycle(step = 1) {
      if (connections.length < 2) return;
      const i = connections.findIndex((c) => c.id === activeId);
      const next = (i + step + connections.length) % connections.length;
      activeId = connections[next].id;
    },

    setState(id: string, state: Connection['state']) {
      const c = connections.find((x) => x.id === id);
      if (c) c.state = state;
    },

    upsert(conn: Connection) {
      const i = connections.findIndex((c) => c.id === conn.id);
      if (i >= 0) connections[i] = conn;
      else connections = [...connections, conn];
    },

    remove(id: string) {
      connections = connections.filter((c) => c.id !== id);
      if (activeId === id) activeId = connections[0]?.id ?? '';
    },
  };
}

export const connectionsStore = createConnectionsStore();
