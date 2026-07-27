/**
 * Picus query editor — per-tab SQL text, results, messages and history.
 *
 * State is keyed by tab id so several query tabs can sit on several databases at
 * once without leaking each other's results. History is per CONNECTION, not per
 * tab: "what did I run on staging" is the question people actually ask.
 *
 * Execution goes to `picus-be`. Cancellation is real: the backend holds the
 * server's cancellation key and opens a second connection to use it, which is why
 * Cancel stops a running statement instead of merely abandoning its result.
 */

import type { QueryLogEntry, QueryResult } from '$lib/types/picus';
import { DEFAULT_QUERY_TEXT } from '$lib/ipc/picus/mock';
import { cancel as rpcCancel, execute } from '$lib/ipc/picus/db';
import { connectionsStore } from './connections.svelte';

export interface HistoryEntry {
  id: string;
  connectionId: string;
  sql: string;
  at: string;
  rowCount: number;
  elapsedMs: number;
  ok: boolean;
}

/** Everything one query tab owns. */
interface QueryTabState {
  sql: string;
  result: QueryResult | null;
  messages: QueryLogEntry[];
  running: boolean;
  /** Result grid vs the server messages/plan. */
  pane: 'results' | 'messages';
  error: string | null;
}

function emptyTab(): QueryTabState {
  return { sql: DEFAULT_QUERY_TEXT, result: null, messages: [], running: false, pane: 'results', error: null };
}

/**
 * Read-only stand-in returned for a tab that has no record yet.
 *
 * Materialising a record is a WRITE, and a write while a `$derived` is being
 * evaluated is a hard error in Svelte 5 (`state_unsafe_mutation`) — so `read()`
 * stays pure and hands this back, while `ensure()` (called from event handlers
 * and from the view's `$effect`) does the actual creation. Frozen so a stray
 * write to it fails loudly instead of silently vanishing on the next read.
 */
const FALLBACK_TAB: QueryTabState = Object.freeze(emptyTab());

function stamp(): string {
  return new Date().toTimeString().slice(0, 8);
}

function createQueryStore() {
  let tabs = $state<Record<string, QueryTabState>>({});
  let history = $state<HistoryEntry[]>([]);
  let historyFilter = $state('');
  /** Row cap; the rest is fetched on demand. */
  let rowLimit = $state(500);
  let seq = 0;

  function ensure(tabId: string): QueryTabState {
    if (!tabs[tabId]) tabs = { ...tabs, [tabId]: emptyTab() };
    return tabs[tabId];
  }

  const filteredHistory = $derived.by(() => {
    const q = historyFilter.trim().toLowerCase();
    if (!q) return history;
    return history.filter((h) => h.sql.toLowerCase().includes(q));
  });

  return {
    get rowLimit() { return rowLimit; },
    get history() { return history; },
    get filteredHistory() { return filteredHistory; },
    get historyFilter() { return historyFilter; },

    setRowLimit(n: number) { rowLimit = Math.max(1, n); },
    setHistoryFilter(v: string) { historyFilter = v; },

    /** Pure read — safe inside a `$derived`. Returns the shared default until
     *  the tab has been materialised by {@link ensure}. */
    read(tabId: string): QueryTabState { return tabs[tabId] ?? FALLBACK_TAB; },

    /** Materialise a tab's record. Call it from an effect or an event handler,
     *  never from a `$derived`. Idempotent. */
    ensure(tabId: string) { ensure(tabId); },

    setSql(tabId: string, sql: string) { ensure(tabId).sql = sql; },
    setPane(tabId: string, pane: 'results' | 'messages') { ensure(tabId).pane = pane; },

    /** History for one connection, most recent first. */
    historyFor(connectionId: string): HistoryEntry[] {
      return filteredHistory.filter((h) => h.connectionId === connectionId);
    },

    /**
     * Run the tab's SQL against its connection.
     *
     * The read-only refusal is the **server's**: a read-only connection was opened
     * in a read-only transaction mode, so the rejection holds for anything that
     * reaches it. Nothing is pre-screened here — a client-side guess about what
     * counts as a write would only ever be a second, weaker opinion.
     */
    async run(tabId: string, connectionId: string) {
      const state = ensure(tabId);
      const conn = connectionsStore.byId(connectionId);
      if (!connectionId) {
        state.error = 'This tab is not bound to a connection.';
        state.pane = 'messages';
        return;
      }

      state.error = null;
      state.running = true;
      const startedAt = stamp();

      try {
        const res = await execute(connectionId, state.sql, rowLimit);
        state.result = {
          columns: res.columns,
          rows: res.rows,
          elapsedMs: res.elapsedMs,
          rowCount: res.rowCount,
          truncated: res.truncated,
        };
        const summary = res.commandTag
          ? res.commandTag
          : `${res.rowCount} row(s)${res.truncated ? ` (capped at ${rowLimit})` : ''}`;
        state.messages = [
          {
            time: startedAt,
            text: `${summary} in ${res.elapsedMs} ms on ${conn?.name ?? connectionId}`,
            level: 'info',
          },
          ...state.messages,
        ];
        state.pane = 'results';
        seq += 1;
        history = [
          {
            id: `h${seq}`,
            connectionId,
            sql: state.sql,
            at: startedAt,
            rowCount: res.rowCount,
            elapsedMs: res.elapsedMs,
            ok: true,
          },
          ...history,
        ].slice(0, 100);
      } catch (e) {
        const message = String(e);
        state.error = message;
        state.messages = [{ time: startedAt, text: message, level: 'error' }, ...state.messages];
        state.pane = 'messages';
        seq += 1;
        history = [
          { id: `h${seq}`, connectionId, sql: state.sql, at: startedAt, rowCount: 0, elapsedMs: 0, ok: false },
          ...history,
        ].slice(0, 100);
      } finally {
        state.running = false;
      }
    },

    /**
     * Stop a running query.
     *
     * Sends the cancel and stops there: the running flag is cleared by whichever
     * outcome `run` receives, because the server decides whether the statement
     * actually stopped. Clearing it here would show "done" over a query still
     * executing.
     */
    async cancel(tabId: string, connectionId: string) {
      const state = ensure(tabId);
      if (!state.running || !connectionId) return;
      state.messages = [
        { time: stamp(), text: 'Cancellation requested…', level: 'info' },
        ...state.messages,
      ];
      try {
        await rpcCancel(connectionId);
      } catch (e) {
        state.messages = [{ time: stamp(), text: String(e), level: 'error' }, ...state.messages];
      }
    },

    clearMessages(tabId: string) { ensure(tabId).messages = []; },
  };
}

export const queryStore = createQueryStore();
