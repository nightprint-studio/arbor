/**
 * Picus query editor — per-tab SQL text, messages and history.
 *
 * State is keyed by tab id so several query tabs can sit on several databases at
 * once without leaking each other's results. History is per CONNECTION, not per
 * tab: "what did I run on staging" is the question people actually ask.
 *
 * Execution goes to `picus-be`. Cancellation is real: the backend holds the
 * server's cancellation key and opens a second connection to use it, which is why
 * Cancel stops a running statement instead of merely abandoning its result.
 *
 * ## The rows are not here
 *
 * A read opens a **held cursor**, and the window onto it lives in
 * `picusResultsStore` keyed by the tab. This store therefore keeps the text, the
 * messages and the outcome of a write, and asks the registry to adopt or release
 * the result — which is what makes "a second query in the same tab closes the
 * first one's cursor" a single line rather than a thing to remember.
 */

import type { QueryLogEntry } from '$lib/types/picus';
import { cancel as rpcCancel, execute } from '$lib/ipc/picus/db';
import { connectionsStore } from './connections.svelte';
import { createResult, formatRowTotal, picusResultsStore } from './result.svelte';
import { picusSettingsStore } from './settings.svelte';

export interface HistoryEntry {
  id: string;
  connectionId: string;
  sql: string;
  at: string;
  rowCount: number;
  /** `rowCount` was the planner's estimate — the entry must be marked `~`. */
  approximate: boolean;
  elapsedMs: number;
  ok: boolean;
}

/** Everything one query tab owns. */
interface QueryTabState {
  sql: string;
  messages: QueryLogEntry[];
  running: boolean;
  /** Result grid vs the server messages/plan. */
  pane: 'results' | 'messages';
  error: string | null;
  /** Rows a write touched — a write's outcome, where a read has a grid. */
  affected: number | null;
  /** A statement has run here. Distinguishes "nothing yet" from "a write". */
  hasRun: boolean;
}

function emptyTab(): QueryTabState {
  // A new tab starts empty. A pre-filled sample would be a statement the user did
  // not write, one Ctrl+Enter away from running against a real database.
  return {
    sql: '',
    messages: [],
    running: false,
    pane: 'results',
    error: null,
    affected: null,
    hasRun: false,
  };
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

  function remember(entry: Omit<HistoryEntry, 'id'>) {
    seq += 1;
    history = [{ id: `h${seq}`, ...entry }, ...history].slice(0, 100);
  }

  return {
    get history() { return history; },
    get filteredHistory() { return filteredHistory; },
    get historyFilter() { return historyFilter; },

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
      state.affected = null;
      // Release the previous cursor BEFORE opening the next one. Running a second
      // statement in the same tab looks like nothing was discarded, which is
      // exactly how a server ends up holding cursors nobody can reach any more.
      picusResultsStore.adopt(tabId, null);
      const startedAt = stamp();

      try {
        // The user's own "rows per window" governs the FIRST window too, not only
        // the ones fetched while scrolling — otherwise the setting is half-honoured
        // and the first window is a different size from every other.
        const res = await execute(connectionId, state.sql, picusSettingsStore.rowLimit);
        const result = createResult(connectionId, res);
        // The tab can be closed while the statement runs. `forget` released what
        // the tab held at the time, so adopting now would file a cursor under an
        // owner nothing will ever release again — close it instead.
        if (!tabs[tabId]) { void result?.close(); return; }
        picusResultsStore.adopt(tabId, result);
        state.affected = res.affected ?? null;
        state.hasRun = true;

        const summary = result
          ? `${formatRowTotal(result)} row(s)`
          : res.affected !== null
            ? `${res.affected.toLocaleString()} row(s) affected`
            : 'statement completed';
        state.messages = [
          {
            time: startedAt,
            text: `${summary} in ${res.elapsedMs} ms on ${conn?.name ?? connectionId}`,
            level: 'info',
          },
          ...state.messages,
        ];
        state.pane = 'results';
        remember({
          connectionId,
          sql: state.sql,
          at: startedAt,
          rowCount: result ? result.total : (res.affected ?? 0),
          approximate: !!result && result.approximate,
          elapsedMs: res.elapsedMs,
          ok: true,
        });
      } catch (e) {
        const message = String(e);
        state.error = message;
        state.messages = [{ time: startedAt, text: message, level: 'error' }, ...state.messages];
        state.pane = 'messages';
        remember({
          connectionId,
          sql: state.sql,
          at: startedAt,
          rowCount: 0,
          approximate: false,
          elapsedMs: 0,
          ok: false,
        });
      } finally {
        state.running = false;
      }
    },

    /**
     * Stop what is running on this connection.
     *
     * Covers the background row count as well as the statement itself — both are
     * work the server is doing for this session, and "cancel" that left a
     * `count(*)` grinding on a hundred-million-row table would be a cancel in
     * name only.
     *
     * Sends the cancel and stops there: the running flag is cleared by whichever
     * outcome `run` receives, because the server decides whether the statement
     * actually stopped. Clearing it here would show "done" over a query still
     * executing.
     */
    async cancel(tabId: string, connectionId: string) {
      const state = ensure(tabId);
      const result = picusResultsStore.forOwner(tabId);
      if ((!state.running && !result?.counting) || !connectionId) return;
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

    /** The tab is gone: drop its text and close the cursor it was holding. */
    forget(tabId: string) {
      picusResultsStore.release(tabId);
      if (!tabs[tabId]) return;
      const { [tabId]: _gone, ...rest } = tabs;
      tabs = rest;
    },
  };
}

export const queryStore = createQueryStore();
