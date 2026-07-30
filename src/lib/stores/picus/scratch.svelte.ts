/**
 * The query tabs survive the window closing.
 *
 * A SQL scratchpad is where the work happens — a `SELECT` refined eight times, the
 * `UPDATE` it turned into — and none of it is a file. Before this, closing Picus
 * threw all of it away, which taught users to be careful about closing a window.
 * That is the wrong thing for a tool to teach.
 *
 * ## Debounced, and once more on the way out
 *
 * Written {@link SAVE_AFTER_MS} after the last keystroke, and again on
 * `beforeunload`. The debounce is what keeps it from being a file write per
 * character; the unload pass is what makes the last sentence typed before Alt+F4
 * survive, which is the one people notice.
 *
 * ## What is restored, and what is not
 *
 * The text, the title and the connection binding. Not the result, not the pending
 * cell edits, not the scroll position: a result is a cursor on a server that closed
 * with the process, and restoring a grid of rows that no longer exist would be a
 * lie told at startup. A restored tab is a buffer, and running it is one keystroke.
 */

import { loadScratch, saveScratch, type ScratchTab } from '$lib/ipc/picus/config';
import { queryStore } from './query.svelte';
import { picusTabsStore } from './tabs.svelte';

/** How long a buffer must sit still before it is written. */
const SAVE_AFTER_MS = 900;

function createScratchStore() {
  /** True once the restore has run, so the first save cannot precede it. */
  let restored = $state(false);
  let timer: ReturnType<typeof setTimeout> | null = null;
  /** The last text written, so an unchanged scratchpad is not rewritten. */
  let lastWritten = '';

  /** The open query tabs, in the shape the file holds. */
  function snapshot(): { tabs: ScratchTab[]; active: string } {
    const tabs = picusTabsStore.tabs
      .filter((tab) => tab.kind === 'query')
      .map((tab) => ({
        id: tab.id,
        title: tab.title,
        connectionId: tab.connectionId ?? '',
        sql: queryStore.read(tab.id).sql,
      }));
    const activeId = picusTabsStore.activeId;
    return { tabs, active: tabs.some((t) => t.id === activeId) ? activeId : '' };
  }

  async function write() {
    if (!restored) return;
    const scratch = snapshot();
    const text = JSON.stringify(scratch);
    // Nothing changed since the last write. Worth checking because the effect that
    // calls this re-runs on any tab change, including opening a table tab, and a
    // file write per unrelated click is noise on somebody's disk.
    if (text === lastWritten) return;
    lastWritten = text;
    try {
      await saveScratch(scratch);
    } catch {
      // A scratchpad that cannot be written is not worth interrupting anybody
      // over: the text is on screen, and the next keystroke tries again.
    }
  }

  return {
    get restored() { return restored; },

    /**
     * Re-open the tabs from the last session.
     *
     * Called once, from the window's mount. Empty tabs are skipped — a tab that was
     * open and untouched is not something to restore, and restoring three of them
     * would make "close everything" a gesture that has to be repeated every launch.
     */
    async restore(): Promise<void> {
      if (restored) return;
      try {
        const scratch = await loadScratch();
        for (const tab of scratch.tabs) {
          if (!tab.sql.trim()) continue;
          picusTabsStore.reopenQuery(tab.id, tab.title, tab.connectionId || undefined);
          queryStore.setSql(tab.id, tab.sql);
        }
        if (scratch.active) picusTabsStore.select(scratch.active);
      } catch {
        // A missing or unreadable scratchpad is the ordinary first-run answer.
      } finally {
        // Set even on failure: otherwise nothing would ever be saved again.
        restored = true;
        lastWritten = JSON.stringify(snapshot());
      }
    },

    /** Something changed. Writes once the typing stops. */
    touch() {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => void write(), SAVE_AFTER_MS);
    },

    /**
     * Write now — the window is closing.
     *
     * Not awaited by the caller, because `beforeunload` will not wait: the value of
     * this call is that the request is *issued* before the webview goes away, and
     * the backend outlives it.
     */
    flush() {
      if (timer) clearTimeout(timer);
      timer = null;
      void write();
    },
  };
}

export const picusScratchStore = createScratchStore();
