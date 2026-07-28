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

import type { Dialect, QueryLogEntry } from '$lib/types/picus';
import { cancel as rpcCancel, execute, sqlStatements } from '$lib/ipc/picus/db';
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

/** Where the caret is and what is selected, in CodeMirror positions. */
export interface EditorSelection {
  from: number;
  to: number;
  head: number;
  empty: boolean;
}

const NO_SELECTION: EditorSelection = { from: 0, to: 0, head: 0, empty: true };

/**
 * Which part of a buffer a run is about.
 *
 * The distinction the whole feature rests on. A query buffer is a scratchpad —
 * a `SELECT` at the top, three `INSERT`s from yesterday, a `COMMIT` at the
 * bottom — and "Run" has to mean one of these three, never "send the file".
 */
export type RunScope =
  /** The one statement the caret is in. */
  | 'statement'
  /** Every statement, in order, stopping at the first that fails. */
  | 'buffer';

/** What a run resolved to, before anything is sent. */
interface RunTarget {
  sql: string;
  /** For the log line, in the user's terms: `the selection`, `line 12`. */
  label: string;
  /** The buffer range it came from, so the editor can show what ran. */
  range: { from: number; to: number } | null;
}

function createQueryStore() {
  let tabs = $state<Record<string, QueryTabState>>({});
  /**
   * How to ask each tab's editor where the caret is.
   *
   * A plain `Map`, deliberately outside `$state`: this holds a handle to a live
   * component, not something anything renders, and making it reactive would turn
   * every caret movement into a change to the query store — which is a re-render
   * of the result grid on every arrow key.
   *
   * It lives here rather than in the view because Run is reachable from three
   * places (the view's button, the toolbar, Ctrl+Enter anywhere in the window)
   * and all three have to mean the same thing. Only one of them can see the
   * editor.
   */
  const editors = new Map<string, () => EditorSelection>();
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

  /**
   * Work out exactly which statements a run will send.
   *
   * Splitting is **always** done, even for a selection of one statement, and that
   * is the fix rather than a refinement: sending several statements as one string
   * makes PostgreSQL run them over the simple protocol, which materialises every
   * result in memory and holds nothing that can be scrolled. A buffer with one
   * large `SELECT` in it then looks exactly like a frozen application.
   *
   * A selection is still honoured to the character. Highlighting a fragment of a
   * statement runs that fragment — the parser is permissive, so a fragment comes
   * back as one statement and the server is left to say what is wrong with it,
   * quoting the user's own text rather than a rewrite of it.
   */
  async function plan(
    tabId: string,
    text: string,
    region: { from: number; to: number } | null,
    dialect: Dialect,
    scope: RunScope,
  ): Promise<RunTarget[]> {
    const state = ensure(tabId);
    let spans: Awaited<ReturnType<typeof sqlStatements>>;
    try {
      spans = await sqlStatements(text, dialect);
    } catch (e) {
      // The splitter is a parse, so this only happens if the backend is not
      // answering — in which case the execute below will not answer either.
      // Sending the text as written is the honest fallback, and it is said out
      // loud because it is the shape that can hang on a multi-statement buffer.
      state.messages = [
        {
          time: stamp(),
          text: `The statements could not be located (${e}) — running the text as written.`,
          level: 'error',
        },
        ...state.messages,
      ];
      return [{ sql: text, label: 'the text as written', range: region }];
    }

    const base = region?.from ?? 0;
    const target = (span: (typeof spans)[number]): RunTarget => ({
      sql: text.slice(span.start, span.end),
      label: `line ${region ? state.sql.slice(0, base + span.start).split('\n').length : span.line}`,
      range: { from: base + span.start, to: base + span.end },
    });

    // Nothing the parser recognised, but the user typed something. The server is
    // the authority on whether it runs.
    if (!spans.length) {
      return [{ sql: text, label: region ? 'the selection' : 'the buffer', range: region }];
    }
    if (scope === 'buffer' || region) {
      return spans.map(target);
    }

    // The statement the caret is in — or, when the caret sits in a comment or a
    // blank line between two statements, the one below it. Reaching *forwards*
    // rather than backwards because a caret parked above a statement reads as
    // "this one", and running the statement above it instead would execute
    // something the user has already moved past.
    const caret = editors.get(tabId)?.().head ?? 0;
    const span =
      spans.find((s) => caret >= s.start && caret <= s.end)
      ?? spans.find((s) => s.start >= caret)
      ?? spans[spans.length - 1];
    return [target(span)];
  }

  /**
   * Send the planned statements, in order, stopping at the first failure.
   *
   * Stopping is the only defensible policy: these are the statements of one
   * script, and carrying on past a failed `CREATE TABLE` would run the `INSERT`s
   * that depend on it and fill the log with errors that are all the same error.
   *
   * Each read replaces the previous held cursor, so a run of five `SELECT`s ends
   * with the last one's rows on screen and four cursors closed rather than four
   * cursors nobody can reach.
   */
  async function runInOrder(tabId: string, connectionId: string, targets: RunTarget[]) {
    const state = ensure(tabId);
    const conn = connectionsStore.byId(connectionId);
    const many = targets.length > 1;
    let affected: number | null = null;

    // Release the previous cursor BEFORE opening the next one. Running a second
    // statement in the same tab looks like nothing was discarded, which is
    // exactly how a server ends up holding cursors nobody can reach any more.
    picusResultsStore.adopt(tabId, null);

    for (const [index, target] of targets.entries()) {
      const startedAt = stamp();
      try {
        // The user's own "rows per window" governs the FIRST window too, not only
        // the ones fetched while scrolling — otherwise the setting is half-honoured
        // and the first window is a different size from every other.
        const res = await execute(connectionId, target.sql, picusSettingsStore.rowLimit);
        const result = createResult(connectionId, res);
        // The tab can be closed while a statement runs. `forget` released what the
        // tab held at the time, so adopting now would file a cursor under an owner
        // nothing will ever release again — close it instead.
        if (!tabs[tabId]) { void result?.close(); return; }

        picusResultsStore.adopt(tabId, result);
        if (res.affected !== null) affected = (affected ?? 0) + res.affected;
        state.hasRun = true;

        const summary = result
          ? `${formatRowTotal(result)} row(s)`
          : res.affected !== null
            ? `${res.affected.toLocaleString()} row(s) affected`
            : 'statement completed';
        const where = many ? `[${index + 1}/${targets.length}] ${target.label} — ` : '';
        state.messages = [
          {
            time: startedAt,
            text: `${where}${summary} in ${res.elapsedMs} ms on ${conn?.name ?? connectionId}`,
            level: 'info',
          },
          ...state.messages,
        ];
        remember({
          connectionId,
          sql: target.sql,
          at: startedAt,
          rowCount: result ? result.total : (res.affected ?? 0),
          approximate: !!result && result.approximate,
          elapsedMs: res.elapsedMs,
          ok: true,
        });
      } catch (e) {
        const message = many ? `${target.label}: ${e}` : String(e);
        state.error = message;
        state.messages = [{ time: startedAt, text: message, level: 'error' }, ...state.messages];
        state.pane = 'messages';
        state.affected = affected;
        remember({
          connectionId,
          sql: target.sql,
          at: startedAt,
          rowCount: 0,
          approximate: false,
          elapsedMs: 0,
          ok: false,
        });
        return;
      }
    }

    state.affected = affected;
    state.pane = 'results';
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
     * Let a query tab's editor answer "where is the caret". Pass `null` on unmount.
     *
     * Registered rather than passed, because Run is reachable from three places
     * and only one of them can see the editor. See {@link editors}.
     */
    bindEditor(tabId: string, read: (() => EditorSelection) | null) {
      if (read) editors.set(tabId, read);
      else editors.delete(tabId);
    },

    /**
     * Run what the user is pointing at.
     *
     * `scope` decides what that means, and the default is the one an IDE user
     * expects: **a selection if there is one, otherwise the statement the caret is
     * in.** Never the whole buffer — that is `'buffer'`, and it is a different key.
     *
     * The read-only refusal is the **server's**: a read-only connection was opened
     * in a read-only transaction mode, so the rejection holds for anything that
     * reaches it. Nothing is pre-screened here — a client-side guess about what
     * counts as a write would only ever be a second, weaker opinion.
     */
    async run(tabId: string, connectionId: string, scope: RunScope = 'statement') {
      const state = ensure(tabId);
      if (!connectionId) {
        state.error = 'This tab is not bound to a connection.';
        state.pane = 'messages';
        return;
      }
      if (state.running) return;

      const dialect = connectionsStore.byId(connectionId)?.dialect ?? 'postgres';
      const selection = editors.get(tabId)?.() ?? NO_SELECTION;

      // A selection is an instruction, not a hint: the user drew the boundary and
      // nothing here adjusts it — not to complete a statement, not to drop a
      // trailing semicolon. Honoured for `'buffer'` too, which is what makes
      // "run these three statements" work by highlighting them.
      const region = selection.empty ? null : { from: selection.from, to: selection.to };
      const text = region ? state.sql.slice(region.from, region.to) : state.sql;

      if (!text.trim()) {
        state.error = region
          ? 'The selection is empty — there is nothing to run.'
          : 'This tab is empty — there is nothing to run.';
        state.pane = 'messages';
        return;
      }

      state.running = true;
      state.error = null;
      state.affected = null;
      try {
        const targets = await plan(tabId, text, region, dialect, scope);
        await runInOrder(tabId, connectionId, targets);
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
      editors.delete(tabId);
      picusResultsStore.release(tabId);
      if (!tabs[tabId]) return;
      const { [tabId]: _gone, ...rest } = tabs;
      tabs = rest;
    },
  };
}

export const queryStore = createQueryStore();
