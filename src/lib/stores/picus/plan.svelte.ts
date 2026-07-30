/**
 * Picus query plans — one plan per query tab.
 *
 * Kept beside the tab's result rather than inside it because a plan is about a
 * *statement*, not about rows: it survives a result being closed, it exists for a
 * statement that has never been run, and asking for one must not disturb whatever
 * the grid is showing.
 *
 * ## The two requests are not one request with a flag
 *
 * {@link explain} plans and executes nothing. {@link measure} **runs the
 * statement** — that is what `EXPLAIN ANALYZE` is — so it is a separate call with
 * its own button and its own sentence in the interface. The backend refuses it for
 * anything that is not a read and, on a read-only connection, refuses it as a
 * write; nothing is pre-screened here, because a client-side guess about what
 * counts as a write would only ever be a second, weaker opinion.
 *
 * The answer carries `analyzed`, so which of the two is on screen is a fact about
 * the plan rather than a memory of which button was pressed.
 */

import type { Dialect } from '$lib/types/picus';
import { explainQuery, type QueryPlan } from '$lib/ipc/picus/plan';
import { queryStore } from './query.svelte';

/** Everything one tab's plan pane owns. */
export interface TabPlan {
  plan: QueryPlan | null;
  /** A request is in flight. */
  running: boolean;
  /** …and it is the one that runs the statement. Says which spinner to explain. */
  measuring: boolean;
  error: string;
  /** The statement the plan on screen is about, so a stale plan can say so. */
  sql: string;
}

function emptyPlan(): TabPlan {
  return { plan: null, running: false, measuring: false, error: '', sql: '' };
}

/**
 * Read-only stand-in for a tab that has no record yet.
 *
 * Materialising a record is a WRITE, and writing while a `$derived` evaluates is a
 * hard error in Svelte 5 (`state_unsafe_mutation`) — so the pure read hands this
 * back and only the event handlers create. Frozen so a stray write fails loudly
 * instead of vanishing on the next read.
 */
const FALLBACK: TabPlan = Object.freeze(emptyPlan());

/** What the user has to do when a run's worth of SQL is not one statement. */
const NEEDS_ONE_STATEMENT =
  'A plan is about one statement. Put the caret in the one you want explained, or select it.';

const NOTHING_TO_EXPLAIN = 'This tab is empty — there is nothing to explain.';

function createPlanStore() {
  let tabs = $state<Record<string, TabPlan>>({});

  /**
   * Which request each tab is on. Outside `$state` — it is control flow, not
   * something anything renders.
   *
   * An abandoned request still answers. Landing its plan on a tab that has since
   * asked about a different statement would put the wrong plan under the right
   * heading, which is worse than no plan at all.
   */
  const runs = new Map<string, number>();

  function ensure(tabId: string): TabPlan {
    if (!tabs[tabId]) tabs = { ...tabs, [tabId]: emptyPlan() };
    return tabs[tabId];
  }

  function nextRun(tabId: string): number {
    const n = (runs.get(tabId) ?? 0) + 1;
    runs.set(tabId, n);
    return n;
  }

  function current(tabId: string, mine: number): boolean {
    return runs.get(tabId) === mine;
  }

  /**
   * Ask for the plan of whatever the tab is pointing at.
   *
   * The statement is resolved exactly as a run resolves it — the selection if there
   * is one, otherwise the statement the caret is in — through the query store, so
   * "explain this" and "run this" can never disagree about which statement *this*
   * is. That is the whole reason it is not worked out here.
   */
  async function request(
    tabId: string,
    connectionId: string,
    dialect: Dialect,
    analyze: boolean,
  ) {
    const state = ensure(tabId);
    if (!connectionId) {
      state.error = 'This tab is not bound to a connection.';
      return;
    }

    const mine = nextRun(tabId);
    state.running = true;
    state.measuring = analyze;
    state.error = '';
    try {
      const sql = await queryStore.statementToExplain(tabId, dialect);
      if (!current(tabId, mine)) return;
      if (!sql.trim()) {
        // Two different situations, and the remedy differs: an empty tab has
        // nothing to say about, a multi-statement selection needs narrowing.
        state.error = queryStore.read(tabId).sql.trim() ? NEEDS_ONE_STATEMENT : NOTHING_TO_EXPLAIN;
        return;
      }

      const plan = await explainQuery(connectionId, sql, analyze);
      if (!current(tabId, mine)) return;
      state.plan = plan;
      state.sql = sql;
      state.error = '';
    } catch (e) {
      if (!current(tabId, mine)) return;
      // The previous plan is dropped rather than left under a new error: a plan on
      // screen beside "that failed" reads as the plan of the thing that failed.
      state.plan = null;
      state.error = String(e);
    } finally {
      if (current(tabId, mine)) {
        state.running = false;
        state.measuring = false;
      }
    }
  }

  return {
    /** Pure read — safe inside a `$derived`. */
    read(tabId: string): TabPlan {
      return tabs[tabId] ?? FALLBACK;
    },

    /** Plan the statement without running it. Every number in the answer is an
     *  estimate, and the answer says so. */
    explain(tabId: string, connectionId: string, dialect: Dialect) {
      return request(tabId, connectionId, dialect, false);
    },

    /**
     * **Run the statement** and report what actually happened.
     *
     * The caller is responsible for having said so first: this is the one call in
     * the plan feature with side effects on the database.
     */
    measure(tabId: string, connectionId: string, dialect: Dialect) {
      return request(tabId, connectionId, dialect, true);
    },

    /** Drop a tab's plan — on close, and whenever it would outlive its statement. */
    forget(tabId: string) {
      runs.delete(tabId);
      if (!tabs[tabId]) return;
      const { [tabId]: _gone, ...rest } = tabs;
      tabs = rest;
    },
  };
}

export const picusPlanStore = createPlanStore();
