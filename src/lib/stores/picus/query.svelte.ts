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

import { findBindSlots, toBindList } from '$lib/components/picus/sql-intel/binds';
import type { Dialect, QueryLogEntry } from '$lib/types/picus';
import { executeBound } from '$lib/ipc/picus/binds';
import {
  cancel as rpcCancel,
  execute,
  resetConnection,
  sourceRelation,
  sqlStatements,
  type SourceRelation,
} from '$lib/ipc/picus/db';
import { picusBindsStore } from './binds.svelte';
import { connectionsStore } from './connections.svelte';
import { picusProvidersStore } from './providers.svelte';
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

/**
 * Which pane of the result panel a tab is showing.
 *
 * `plan` is not a third kind of result: it is what the server says about the
 * statement, and it lives here because the panel it appears in is this panel. Its
 * contents belong to `picusPlanStore`, keyed by the same tab.
 */
export type ResultPane = 'results' | 'messages' | 'plan';

/** Everything one query tab owns. */
interface QueryTabState {
  sql: string;
  messages: QueryLogEntry[];
  running: boolean;
  /** Result grid vs the server messages vs the statement's plan. */
  pane: ResultPane;
  error: string | null;
  /** Rows a write touched — a write's outcome, where a read has a grid. */
  affected: number | null;
  /** A statement has run here. Distinguishes "nothing yet" from "a write". */
  hasRun: boolean;
  /**
   * The relation the rows on screen came from, as the parser and the catalogue
   * agree it is. `null` until a read has been traced.
   *
   * Resolved from the statement that **ran**, and kept here rather than derived
   * from the buffer whenever it is needed. That distinction is the whole reason
   * this field exists: a query tab is a scratchpad holding several statements, and
   * asking "which table is this?" of the *buffer* answers about all of them at
   * once — which is why an ordinary single-table query was reported as a join, and
   * its rows refused editing and refused to open their large objects.
   */
  source: SourceRelation | null;
  /**
   * How long the whole run took on the server, summed over its statements.
   *
   * Kept per tab rather than read off the result, because a *write* has no result
   * to read it off and "how long did that take" is asked about an UPDATE at least
   * as often as about a SELECT. `null` until something has run.
   */
  elapsedMs: number | null;
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
    source: null,
    elapsedMs: null,
  };
}

/**
 * A duration, at the precision it is worth reading.
 *
 * Milliseconds up to a second, because that is the range where a difference of ten
 * of them means something; seconds above it, because `18 342 ms` is a number nobody
 * parses at a glance and `18.3 s` is the same fact.
 */
export function formatElapsed(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  const minutes = Math.floor(ms / 60_000);
  const seconds = Math.round((ms % 60_000) / 1000);
  return `${minutes} m ${seconds.toString().padStart(2, '0')} s`;
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
 * How long a cancel is given to be honoured before the connection is abandoned.
 *
 * Long enough that a statement which *is* stopping gets to stop — the server has to
 * notice the request, unwind, and answer — and short enough that a user who has
 * decided to stop waiting is not made to wait again. A cancel that works is normally
 * acted on in well under a second.
 */
const CANCEL_GRACE_MS = 2500;

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

  /**
   * Which run each tab is on. Outside `$state` — it is control flow, not something
   * anything renders.
   *
   * The reason it exists is **zombie results**. Giving up on a statement does not
   * un-send it: the reply may still arrive, minutes later, carrying a `resultId`
   * for a cursor the server is holding. Adopting it then would file that cursor
   * under a tab that has since run something else — closing the *current* result to
   * make room for a stale one, and leaving whatever the newer run opened with
   * nobody to close it. So every run carries its ordinal, and a reply that is no
   * longer the tab's current run has its result closed on the spot instead.
   */
  const runs = new Map<string, number>();

  function nextRun(tabId: string): number {
    const n = (runs.get(tabId) ?? 0) + 1;
    runs.set(tabId, n);
    return n;
  }

  /** Is this run still the one the tab is waiting on? */
  function current(tabId: string, mine: number): boolean {
    return runs.get(tabId) === mine;
  }

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
  /**
   * Ask the backend which relation a statement read, and file the answer on the tab.
   *
   * Silent on failure: a source that cannot be traced leaves the features that need
   * one switched off, which is exactly what a `null` already means, and an error
   * about it would be noise over rows that arrived perfectly well.
   */
  async function traceSource(tabId: string, connectionId: string, sql: string, mine: number) {
    try {
      const found = await sourceRelation(connectionId, sql);
      if (!tabs[tabId] || !current(tabId, mine)) return;
      tabs[tabId].source = found;
    } catch {
      /* untraceable is the same as untraced, for every caller of this */
    }
  }

  /**
   * Send one statement, binding its placeholders when the engine has that concept.
   *
   * Decided per statement rather than per run: a buffer holds a parameterised
   * `SELECT` and a plain `COMMIT` side by side, and sending the second one down the
   * bound path would hand the server a value list it never asked for. Each
   * statement's own list is built from the values the tab supplied, addressed by
   * placeholder — which is what makes two statements sharing `:CODICE` share the
   * value, and `$1` in each of them mean each one's own first parameter.
   */
  function send(tabId: string, connectionId: string, sql: string, binding: Dialect | null) {
    const slots = binding ? findBindSlots(sql, binding) : [];
    // The user's own "rows per window" governs the FIRST window too, not only the
    // ones fetched while scrolling — otherwise the setting is half-honoured and the
    // first window is a different size from every other.
    if (!slots.length) return execute(connectionId, sql, picusSettingsStore.rowLimit);
    const list = toBindList(slots, (label) => picusBindsStore.valueOf(tabId, label));
    return executeBound(connectionId, sql, list, picusSettingsStore.rowLimit);
  }

  async function runInOrder(
    tabId: string,
    connectionId: string,
    targets: RunTarget[],
    mine: number,
    binding: Dialect | null,
  ) {
    const state = ensure(tabId);
    const conn = connectionsStore.byId(connectionId);
    const many = targets.length > 1;
    let affected: number | null = null;
    let elapsed = 0;

    // Release the previous cursor BEFORE opening the next one. Running a second
    // statement in the same tab looks like nothing was discarded, which is
    // exactly how a server ends up holding cursors nobody can reach any more.
    picusResultsStore.adopt(tabId, null);

    for (const [index, target] of targets.entries()) {
      const startedAt = stamp();
      try {
        const res = await send(tabId, connectionId, target.sql, binding);
        const result = createResult(connectionId, res);
        // Two ways this reply is no longer wanted, and both end the same way —
        // **close the cursor rather than adopt it**. A held result nobody owns is a
        // tuplestore on somebody's database that nothing will ever release.
        //
        //  • the tab was closed while the statement ran: `forget` released what it
        //    held at the time, so adopting now would file a cursor under an owner
        //    that no longer exists;
        //  • the tab has since run something else — this is a reply to a question
        //    that was abandoned, and adopting it would close the answer the user is
        //    actually looking at.
        if (!tabs[tabId] || !current(tabId, mine)) { void result?.close(); return; }

        picusResultsStore.adopt(tabId, result);
        // Traced from **this statement**, not from the tab's text, and once per run
        // rather than on every keystroke. `void`: the rows are already on screen and
        // nothing waits on the answer — it decides whether the grid offers editing,
        // which is a thing that can appear a moment later.
        state.source = null;
        if (result) void traceSource(tabId, connectionId, target.sql, mine);
        if (res.affected !== null) affected = (affected ?? 0) + res.affected;
        // Summed across the run: `Run all` is one thing the user asked for, so its
        // cost is one number. The per-statement times stay in the messages.
        elapsed += res.elapsedMs;
        state.elapsedMs = elapsed;
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
            text: `${where}${summary} in ${formatElapsed(res.elapsedMs)} on ${conn?.name ?? connectionId}`,
            level: 'info',
          },
          ...state.messages,
        ];
        // A read that holds no cursor cannot be scrolled into, so a window that
        // came back full is the end of what the user can reach. Said once, plainly,
        // rather than left to be discovered by a scrollbar that stops.
        if (result && !res.resultId && !res.endOfResult) {
          state.messages = [
            {
              time: startedAt,
              text: `Only the first ${res.rowCount.toLocaleString()} row(s) are here — a statement `
                + 'that carries values cannot be scrolled, because a cursor takes no parameters. '
                + 'Add your own LIMIT/OFFSET, or run it without placeholders.',
              level: 'info',
            },
            ...state.messages,
          ];
        }
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
        // What it cost before it failed is still worth knowing: a statement that
        // took four minutes and then failed is a different problem from one that
        // failed immediately.
        state.elapsedMs = elapsed > 0 ? elapsed : null;
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
    setPane(tabId: string, pane: ResultPane) { ensure(tabId).pane = pane; },

    /**
     * The SQL a run with the default scope **would** send — the selection if there
     * is one, otherwise the statement the caret is in.
     *
     * For the features that act on "the statement you are pointing at" without
     * running it; the plan is the only one today. It goes through the same
     * resolution a run does rather than repeating it, so "explain this" and "run
     * this" can never disagree about which statement *this* is.
     *
     * Empty when there is nothing to point at, and also when the pointing resolves
     * to **more than one** statement: a plan is about one statement, and explaining
     * the first of several silently would answer a question nobody asked. The
     * caller says which of the two it was.
     */
    async statementToExplain(tabId: string, dialect: Dialect): Promise<string> {
      const state = ensure(tabId);
      const selection = editors.get(tabId)?.() ?? NO_SELECTION;
      const region = selection.empty ? null : { from: selection.from, to: selection.to };
      const text = region ? state.sql.slice(region.from, region.to) : state.sql;
      if (!text.trim()) return '';
      const targets = await plan(tabId, text, region, dialect, 'statement');
      return targets.length === 1 ? targets[0].sql : '';
    },

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
     *
     * ## Placeholders stop the run and ask
     *
     * When the text carries placeholders and the engine binds them, nothing is sent:
     * the run files a prompt (`picusBindsStore`) and returns, and the modal restarts
     * it with `bindsResolved` once the values are in. Asking every time is the point
     * — the boxes come back filled in, but the values are what a Run is *about*, so
     * they are never reused behind the user's back.
     *
     * An engine without the capability runs exactly as it does today: the text goes
     * as written, and whatever the placeholder means to that server is what happens.
     */
    async run(
      tabId: string,
      connectionId: string,
      scope: RunScope = 'statement',
      opts: { bindsResolved?: boolean } = {},
    ) {
      const state = ensure(tabId);
      if (!connectionId) {
        state.error = 'This tab is not bound to a connection.';
        state.pane = 'messages';
        return;
      }
      if (state.running) {
        // Said rather than swallowed. A Run that does nothing and explains nothing
        // is indistinguishable from a broken button, and this is exactly the state
        // a user reaches when the previous statement will not stop.
        state.messages = [
          {
            time: stamp(),
            text: 'This tab is still waiting on the previous statement. Cancel it first '
              + '(Ctrl+Shift+C) — if the server will not stop it, that reconnects.',
            level: 'error',
          },
          ...state.messages,
        ];
        state.pane = 'messages';
        return;
      }

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

      // Bound values, when the engine has them. Read from the descriptor rather
      // than from the engine's name: a capability that is false must make the flow
      // ABSENT, not present and refused.
      const binding = picusProvidersStore.capabilities(dialect)?.bindParameters ? dialect : null;
      if (binding && !opts.bindsResolved) {
        const slots = findBindSlots(text, binding);
        if (slots.length) {
          picusBindsStore.ask({ tabId, connectionId, scope, slots });
          return;
        }
      }

      // A session is ONE database connection. A background row count still running
      // on it would make this statement queue behind it — which is why running a
      // second query straight after a first one looked like the studio had hung,
      // and why it worked after a restart. The cancel is what actually stops the
      // server; abandoning the count is what stops us waiting for its answer.
      if (picusResultsStore.yieldConnection(connectionId)) {
        try {
          await rpcCancel(connectionId);
        } catch {
          // Nothing to cancel is the ordinary case, and never worth a message.
        }
      }

      const mine = nextRun(tabId);
      state.running = true;
      state.error = null;
      state.affected = null;
      // Cleared, not carried over: the previous run's time beside this run's rows
      // is the kind of stale number people quote at each other.
      state.elapsedMs = null;
      try {
        const targets = await plan(tabId, text, region, dialect, scope);
        if (!current(tabId, mine)) return;
        await runInOrder(tabId, connectionId, targets, mine, binding);
      } finally {
        // Only this run's own spinner. An abandoned run whose reply finally arrives
        // must not clear the spinner of the one the user started afterwards.
        if (current(tabId, mine)) state.running = false;
      }
    },

    /**
     * Stop what is running on this connection, and **do not come back without
     * having stopped it**.
     *
     * Covers the background row count as well as the statement itself — both are
     * work the server is doing for this session, and a cancel that left a
     * `count(*)` grinding on a hundred-million-row table would be a cancel in name
     * only.
     *
     * ## Asking is the first step, not the only one
     *
     * The old version sent the server's cancel key and stopped, on the reasoning
     * that the server decides whether a statement stops. That reasoning is right
     * about the *server* and wrong about the *product*: PostgreSQL ignores a cancel
     * while a backend is inside an uninterruptible wait, and there are ordinary ways
     * to end up in one. What the user then saw was a spinner that never went out, a
     * Cancel that did nothing each time it was pressed, and a tab that would never
     * run anything again — with nothing on screen admitting any of it.
     *
     * So a cancel that is not honoured escalates. After {@link CANCEL_GRACE_MS} the
     * connection is **abandoned**: the session is dropped without being spoken to
     * (which is the only thing that works once it has stopped answering), a new one
     * is opened, and this tab stops waiting. The old statement may still be running
     * on the server — that is said plainly rather than papered over — but the studio
     * is usable again, which is the part Picus owes.
     */
    async cancel(tabId: string, connectionId: string) {
      const state = ensure(tabId);
      const result = picusResultsStore.forOwner(tabId);
      if ((!state.running && !result?.counting) || !connectionId) return;

      const say = (text: string, level: QueryLogEntry['level'] = 'info') => {
        state.messages = [{ time: stamp(), text, level }, ...state.messages];
      };
      say('Cancellation requested…');
      try {
        await rpcCancel(connectionId);
      } catch (e) {
        say(String(e), 'error');
      }
      if (!state.running) return;

      await new Promise((done) => setTimeout(done, CANCEL_GRACE_MS));
      if (!state.running) return;

      say(
        'The server has not stopped it. Dropping this connection and opening a new one — '
          + 'the statement may still be running there until the database ends it.',
        'error',
      );
      // Invalidate the run FIRST. Its reply may still arrive, and by then this tab
      // may be running something else; the ordinal is what stops it landing on top.
      nextRun(tabId);
      state.running = false;
      state.pane = 'messages';
      // Every result on this connection belonged to a socket that is about to go.
      picusResultsStore.releaseConnection(connectionId);

      try {
        await resetConnection(connectionId);
        say('Reconnected. This tab can run statements again.');
        // The sidebar reads the pool; it is a different socket now.
        void connectionsStore.load();
      } catch (e) {
        say(`The connection could not be reopened — ${e}`, 'error');
      }
    },

    clearMessages(tabId: string) { ensure(tabId).messages = []; },

    /** The tab is gone: drop its text and close the cursor it was holding. */
    forget(tabId: string) {
      editors.delete(tabId);
      picusResultsStore.release(tabId);
      // Its bound values go with it. They are somebody's customer numbers as often
      // as not, and a closed tab has no business keeping them in memory.
      picusBindsStore.forget(tabId);
      if (!tabs[tabId]) return;
      const { [tabId]: _gone, ...rest } = tabs;
      tabs = rest;
    },
  };
}

export const queryStore = createQueryStore();
