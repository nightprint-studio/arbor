/**
 * Picus editor tabs — the centre area's open documents.
 *
 * Every tab that runs SQL is bound to a connection; changing that binding
 * re-runs against the new database rather than silently keeping the old result.
 * The connection's colour paints the tab's top accent, so two tabs on two
 * databases are never confused at a glance.
 */

import type { Dialect, PicusTab, TabKind } from '$lib/types/picus';
import { connectionsStore } from './connections.svelte';

/** The generator is a singleton tab: there is one generation in flight. */
const GENERATE_TAB_ID = 'generate';
const INVENTORY_TAB_ID = 'inventory';

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

  return {
    get tabs() { return tabs; },
    get activeId() { return activeId; },
    get active() { return active; },
    get activeKind(): TabKind | null { return active?.kind ?? null; },

    /** The connection the active tab runs against, falling back to the window's. */
    get activeConnection() {
      return connectionsStore.byId(active?.connectionId) ?? connectionsStore.active;
    },

    select(id: string) { if (tabs.some((t) => t.id === id)) activeId = id; },

    close(id: string) {
      const i = tabs.findIndex((t) => t.id === id);
      if (i < 0) return;
      tabs = tabs.filter((t) => t.id !== id);
      if (activeId !== id) return;
      activeId = tabs[Math.min(i, tabs.length - 1)]?.id ?? '';
    },

    closeOthers(id: string) {
      tabs = tabs.filter((t) => t.id === id || !isClosable(t));
      activeId = id;
    },

    /** Close everything to the right of `id` — the "I'm done exploring" gesture. */
    closeToRight(id: string) {
      const i = tabs.findIndex((t) => t.id === id);
      if (i < 0) return;
      tabs = tabs.filter((t, idx) => idx <= i || !isClosable(t));
      if (!tabs.some((t) => t.id === activeId)) activeId = id;
    },

    /** Close every closable tab. The generator is pinned and survives. */
    closeAll() {
      tabs = tabs.filter((t) => !isClosable(t));
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
    openFile(path: string, name: string, dialect: Dialect | null, line?: number) {
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
