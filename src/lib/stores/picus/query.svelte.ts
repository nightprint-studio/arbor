/**
 * Picus query editor — per-tab SQL text, results, messages and history.
 *
 * State is keyed by tab id so several query tabs can sit on several databases at
 * once without leaking each other's results. History is per CONNECTION, not per
 * tab: "what did I run on staging" is the question people actually ask.
 *
 * MOCK: execution replays fixtures from `ipc/picus/mock`. Cancellation is real
 * in shape (a running query can be stopped) but there is no driver behind it yet.
 */

import type { CellValue, Column, QueryLogEntry, QueryResult } from '$lib/types/picus';
import { DEFAULT_QUERY_TEXT, MOCK_TABLE_ROWS } from '$lib/ipc/picus/mock';
import { connectionsStore } from './connections.svelte';
import { schemaStore } from './schema.svelte';

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
     * A read-only connection refuses writes — the check lives in the backend for
     * real; mirrored here so the mock behaves the same way the product must.
     */
    run(tabId: string, connectionId: string) {
      const state = ensure(tabId);
      const conn = connectionsStore.byId(connectionId);
      const isWrite = /^\s*(insert|update|delete|merge|drop|alter|create|truncate)\b/i.test(state.sql);

      if (conn?.readOnly && isWrite) {
        state.error = `${conn.name} is marked read-only: write statements are refused.`;
        state.messages = [
          { time: stamp(), text: `Refused: ${conn.name} is a read-only connection.`, level: 'error' },
          ...state.messages,
        ];
        state.pane = 'messages';
        return;
      }

      state.error = null;
      state.running = true;

      // MOCK execution: replay a fixture after a beat so the running/cancel
      // states are exercisable.
      setTimeout(() => {
        if (!state.running) return; // cancelled meanwhile
        const relations = schemaStore.relations;
        const table = relations.find((t) => new RegExp(`\\b${t.name}\\b`, 'i').test(state.sql)) ?? relations[0];
        const columns: Column[] = table.columns;
        const rows: CellValue[][] = (MOCK_TABLE_ROWS[table.name] ?? []).slice(0, rowLimit);
        const elapsedMs = 38 + (seq % 7) * 6;
        state.result = { columns, rows, elapsedMs, rowCount: rows.length, truncated: false };
        state.messages = [
          { time: stamp(), text: `${rows.length} row(s) in ${elapsedMs} ms on ${conn?.name ?? 'unknown'}`, level: 'info' },
          { time: stamp(), text: `Plan: TABLE ACCESS FULL ${table.name} (cost 3)`, level: 'info' },
          ...state.messages,
        ];
        state.running = false;
        seq += 1;
        history = [
          { id: `h${seq}`, connectionId, sql: state.sql, at: stamp(), rowCount: rows.length, elapsedMs, ok: true },
          ...history,
        ].slice(0, 100);
      }, 320);
    },

    /** Stop a running query. Real cancellation is a driver call; this is the UI half. */
    cancel(tabId: string) {
      const state = ensure(tabId);
      if (!state.running) return;
      state.running = false;
      state.messages = [{ time: stamp(), text: 'Query cancelled.', level: 'error' }, ...state.messages];
    },

    clearMessages(tabId: string) { ensure(tabId).messages = []; },
  };
}

export const queryStore = createQueryStore();
