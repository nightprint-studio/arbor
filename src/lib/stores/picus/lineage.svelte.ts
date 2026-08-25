/**
 * The lineage of whatever result is being looked at.
 *
 * ## Asked for, never computed
 *
 * A trace parses every view between a column and its table. That is cheap on the
 * backend and cached there, but it is still a deliberate question — so nothing here
 * runs on its own. A tab has no lineage until somebody presses the button, and
 * running a new statement discards the one it had rather than silently re-tracing
 * something the user has not asked about again.
 *
 * ## Keyed by tab, like the plan
 *
 * Two query tabs are two independent questions, and a lineage outlives the result it
 * was taken of — you trace a column, scroll away, run something else in another tab,
 * and come back to read the chain. Keyed by tab is what makes all of that work
 * without any of it being a special case.
 */

import { statementLineage, type Lineage } from '$lib/ipc/picus/lineage';

/** What one tab knows about where its columns come from. */
export interface TabLineage {
  lineage: Lineage | null;
  running: boolean;
  error: string;
  /** The statement the lineage is about, so a stale one can say so. */
  sql: string;
  /**
   * The result it was taken of.
   *
   * How staleness is decided, and deliberately **not** an effect that clears on a
   * new result: an effect keyed on the active tab fires when you merely *switch* to
   * a tab, which would throw away a trace taken on it earlier. Comparing an id at
   * read time makes a stale lineage unrepresentable instead of cleaned up a moment
   * later, and costs nothing.
   */
  resultId: string;
}

function empty(): TabLineage {
  return { lineage: null, running: false, error: '', sql: '', resultId: '' };
}

/**
 * Read-only stand-in for a tab that has never asked.
 *
 * Materialising a record is a WRITE, and writing while a `$derived` evaluates is a
 * hard error in Svelte 5 (`state_unsafe_mutation`) — so the pure read hands this
 * back and only the event handlers create. Frozen so a stray write fails loudly
 * instead of vanishing on the next read.
 */
const FALLBACK: TabLineage = Object.freeze(empty());

function createLineageStore() {
  let byTab = $state<Record<string, TabLineage>>({});

  function ensure(tabId: string): TabLineage {
    if (!byTab[tabId]) byTab[tabId] = empty();
    return byTab[tabId];
  }

  return {
    /** What a tab knows. Never writes — safe to call from a `$derived`. */
    read(tabId: string): TabLineage {
      return byTab[tabId] ?? FALLBACK;
    },

    /**
     * Trace the columns `sql` projects, on `connectionId`.
     *
     * A second press while one is in flight is the same question and is dropped;
     * the button is disabled anyway, and a guard that only lives in the view is a
     * guard that stops existing the first time something else calls this.
     */
    async trace(
      tabId: string,
      connectionId: string,
      sql: string,
      resultId: string,
    ): Promise<void> {
      if (!tabId || !connectionId || !sql.trim()) return;
      const state = ensure(tabId);
      if (state.running) return;
      state.running = true;
      state.error = '';
      state.sql = sql;
      state.resultId = resultId;
      try {
        const lineage = await statementLineage(connectionId, sql);
        // The tab may have been closed, or asked a different question, while the
        // views were being read. Landing this anyway would show one statement's
        // chain under another's columns.
        const current = byTab[tabId];
        if (!current || current.sql !== sql) return;
        current.lineage = lineage;
      } catch (e) {
        const current = byTab[tabId];
        if (current && current.sql === sql) {
          current.error = String(e);
          current.lineage = null;
        }
      } finally {
        const current = byTab[tabId];
        if (current) current.running = false;
      }
    },

    /** Drop what a tab knew — a new result is a new question. */
    clear(tabId: string) {
      if (byTab[tabId]) {
        const { [tabId]: _gone, ...rest } = byTab;
        byTab = rest;
      }
    },
  };
}

export const picusLineageStore = createLineageStore();
