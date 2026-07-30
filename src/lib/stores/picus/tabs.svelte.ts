/**
 * Picus editor tabs — the centre area's open documents.
 *
 * Every tab that runs SQL is bound to a connection; changing that binding
 * re-runs against the new database rather than silently keeping the old result.
 * The connection's colour paints the tab's top accent, so two tabs on two
 * databases are never confused at a glance.
 *
 * ## Closing a tab closes a cursor
 *
 * A query or table tab holds a **result** — a cursor the server is keeping open
 * on its behalf. Every path that removes a tab from this list therefore runs
 * through {@link forget}, including the bulk ones: "close all" abandoning four
 * cursors is the same leak as "close" abandoning one, and is easier to miss.
 */

import type { FolderEngine, PicusTab, TabKind } from '$lib/types/picus';
import { connectionsStore } from './connections.svelte';
import { queryStore } from './query.svelte';

/** The generator is a singleton tab: there is one generation in flight. */
const GENERATE_TAB_ID = 'generate';
const INVENTORY_TAB_ID = 'inventory';
const RESTRUCTURE_TAB_ID = 'restructure';

/**
 * The generator is pinned: it holds work in progress that no bulk "close
 * everything" gesture should throw away, and it is the window's home view.
 */
function isClosable(tab: PicusTab): boolean {
  return tab.kind !== 'generate';
}

function createTabsStore() {
  let tabs = $state<PicusTab[]>([
    { id: GENERATE_TAB_ID, kind: 'generate', title: 'Generate DML' },
  ]);
  let activeId = $state<string>(GENERATE_TAB_ID);
  let querySeq = 0;

  const active = $derived(tabs.find((t) => t.id === activeId) ?? null);

  /** Focus an existing tab, or append it and focus that. */
  function open(tab: PicusTab) {
    if (!tabs.some((t) => t.id === tab.id)) tabs = [...tabs, tab];
    activeId = tab.id;
  }

  /**
   * Replace the list, releasing whatever fell out of it.
   *
   * Every removal goes through here so a cursor cannot survive its tab — the
   * single-tab close and the three bulk gestures share one line of cleanup
   * instead of four copies, one of which would eventually not be updated.
   */
  function keep(predicate: (tab: PicusTab, index: number) => boolean) {
    for (const [i, t] of tabs.entries()) if (!predicate(t, i)) queryStore.forget(t.id);
    tabs = tabs.filter(predicate);
  }

  return {
    get tabs() { return tabs; },
    get activeId() { return activeId; },
    get active() { return active; },
    get activeKind(): TabKind | null { return active?.kind ?? null; },

    /**
     * The connection a tab runs against — its own binding, falling back to the
     * window's current one.
     *
     * **The one answer to that question**, and it has to be, because the fallback
     * is the interesting half: a tab whose binding does not resolve still runs, and
     * it runs against the window's connection. Anything that resolved
     * `tab.connectionId` on its own would describe a different database than the
     * statement was sent to — which is how a panel came to report "this connection
     * is read-only" about a connection that was not being used.
     */
    connectionOf(tab: PicusTab | null | undefined) {
      return connectionsStore.byId(tab?.connectionId) ?? connectionsStore.active;
    },

    /** The connection the active tab runs against. */
    get activeConnection() {
      return connectionsStore.byId(active?.connectionId) ?? connectionsStore.active;
    },

    select(id: string) { if (tabs.some((t) => t.id === id)) activeId = id; },

    close(id: string) {
      const i = tabs.findIndex((t) => t.id === id);
      if (i < 0) return;
      keep((t) => t.id !== id);
      if (activeId !== id) return;
      activeId = tabs[Math.min(i, tabs.length - 1)]?.id ?? '';
    },

    closeOthers(id: string) {
      keep((t) => t.id === id || !isClosable(t));
      activeId = id;
    },

    /** Close everything to the right of `id` — the "I'm done exploring" gesture. */
    closeToRight(id: string) {
      const i = tabs.findIndex((t) => t.id === id);
      if (i < 0) return;
      keep((t, idx) => idx <= i || !isClosable(t));
      if (!tabs.some((t) => t.id === activeId)) activeId = id;
    },

    /** Close every closable tab. The generator is pinned and survives. */
    closeAll() {
      keep((t) => !isClosable(t));
      activeId = tabs[0]?.id ?? '';
    },

    /** How many tabs a "close others / to the right" would actually remove. */
    closableCount(exceptId?: string): number {
      return tabs.filter((t) => isClosable(t) && t.id !== exceptId).length;
    },

    closableToRight(id: string): number {
      const i = tabs.findIndex((t) => t.id === id);
      if (i < 0) return 0;
      return tabs.slice(i + 1).filter(isClosable).length;
    },

    reorder(from: number, to: number) {
      const next = [...tabs];
      const [moved] = next.splice(from, 1);
      next.splice(to, 0, moved);
      tabs = next;
    },

    /** Step through the tab strip — Ctrl+Tab / Ctrl+PageUp-PageDown. */
    cycle(step: number) {
      if (tabs.length < 2) return;
      const i = tabs.findIndex((t) => t.id === activeId);
      activeId = tabs[(i + step + tabs.length) % tabs.length].id;
    },

    openGenerate() {
      open({ id: GENERATE_TAB_ID, kind: 'generate', title: 'Generate DML' });
    },

    /** The structural search-and-replace workspace. One tab, like the generator:
     *  it holds a pattern somebody is refining, and a second copy would be two
     *  half-finished migrations. */
    openRestructure() {
      open({ id: RESTRUCTURE_TAB_ID, kind: 'restructure', title: 'Structural replace' });
    },

    openInventory() {
      open({ id: INVENTORY_TAB_ID, kind: 'inventory', title: 'Inventory' });
    },

    /** A fresh query editor bound to a connection (the active one by default). */
    openQuery(connectionId?: string) {
      const conn = connectionsStore.byId(connectionId) ?? connectionsStore.active;
      querySeq += 1;
      open({
        id: `query:${querySeq}`,
        kind: 'query',
        title: `query_${querySeq}.sql`,
        connectionId: conn?.id,
        dialect: conn?.dialect,
      });
    },

    /**
     * Re-open a query tab from the saved scratchpad, keeping its id and title.
     *
     * Separate from {@link openQuery} because it must **not** number a new tab: the
     * id is the key the buffer was saved under, and renaming `query_7.sql` to
     * `query_1.sql` on every launch would make the titles meaningless. The sequence
     * is advanced past whatever was restored so the next new tab does not collide.
     */
    /**
     * Re-open a tab from the last session.
     *
     * The connection id is taken **as given**, and deliberately not looked up
     * first. Restoring runs at window mount, alongside the read that fills the
     * connection list rather than after it — so a lookup here answers "there is no
     * such connection" for every tab, every time, and each one comes back bound to
     * nothing. The tab then runs against whatever connection happens to be active
     * while the panel beside it describes none, which is how a restored tab came
     * back read-only.
     *
     * A binding is data. Resolving it to a live connection is a view's job, and
     * {@link activeConnection} does it on every read — by which time the list has
     * arrived. Nothing here needs to be ordered against anything.
     */
    reopenQuery(id: string, title: string, connectionId?: string) {
      const numbered = /^query:(\d+)$/.exec(id);
      if (numbered) querySeq = Math.max(querySeq, Number(numbered[1]));
      open({
        id,
        kind: 'query',
        title,
        connectionId: connectionId || undefined,
        dialect: connectionsStore.byId(connectionId)?.dialect,
      });
    },

    /**
     * Open a schema object. Tables, views, sequences and triggers share the
     * `table` tab kind: the frame is identical (name · connection · sub-views)
     * and only the sub-views' contents differ.
     */
    openObject(
      name: string,
      objectKind: NonNullable<PicusTab['objectKind']> = 'table',
      connectionId?: string,
    ) {
      const conn = connectionsStore.byId(connectionId) ?? connectionsStore.active;
      open({
        id: `object:${conn?.id ?? '-'}:${objectKind}:${name}`,
        kind: 'table',
        title: name,
        table: name,
        objectKind,
        connectionId: conn?.id,
        dialect: conn?.dialect,
      });
    },

    /** Shorthand for the common case. */
    openTable(table: string, connectionId?: string) {
      this.openObject(table, 'table', connectionId);
    },

    /**
     * Open a script file, optionally at a line.
     *
     * `line` is what turns a finding's location from a label into navigation: the
     * view reads it off the tab and asks the editor to reveal it. The nonce is
     * bumped every time so stepping twice onto the same line moves the caret both
     * times instead of looking ignored.
     */
    openFile(path: string, name: string, dialect: FolderEngine | null, line?: number) {
      const id = `file:${path}`;
      open({
        id,
        kind: 'file',
        title: name,
        file: path,
        dialect: dialect ?? undefined,
      });
      if (!line) return;
      const tab = tabs.find((t) => t.id === id);
      if (!tab) return;
      tab.revealLine = line;
      tab.revealNonce = (tab.revealNonce ?? 0) + 1;
    },

    /** Rebind a tab to another connection — the tab re-runs on the new database. */
    setTabConnection(id: string, connectionId: string) {
      const tab = tabs.find((t) => t.id === id);
      if (!tab) return;
      tab.connectionId = connectionId;
      tab.dialect = connectionsStore.byId(connectionId)?.dialect;
    },

    markDirty(id: string, dirty: boolean) {
      const tab = tabs.find((t) => t.id === id);
      if (tab) tab.dirty = dirty;
    },
  };
}

export const picusTabsStore = createTabsStore();
