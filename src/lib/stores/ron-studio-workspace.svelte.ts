/**
 * ronStudioWorkspaceStore — multi-file companion to `ronStudioStore`.
 *
 * Lives alongside the single-active-doc store: holds the *list* of open
 * tabs + an optional workspace folder + the file index inside it. The
 * single-doc store keeps tracking whichever tab is active; switching
 * tabs swaps that doc out.
 *
 * Keeping this concern in a separate store avoids reflowing every
 * accessor on `ronStudioStore` into a Map<docId, …>; the per-doc
 * registry already lives on the host (RonStudioRegistry), and the
 * workspace store is mostly UI bookkeeping.
 */

import {
  studioBackend,
  type SchemaHint,
  type StudioFileEntry,
} from '$lib/ipc/studio-format';
import type { RonNodeKind } from '$lib/types/ron-studio';
import { studioStore }   from '$lib/stores/studio.svelte';
import { tabsStore }     from '$lib/stores/tabs.svelte';

const RON = studioBackend<RonNodeKind>('ron');

export interface RonTab {
  docId:      string;
  sourcePath: string | null;
  title:      string;
  /** Live-derived dirtiness flag — pulled from the per-doc snapshot
   *  refresh that runs on every tab-switch or doc-mutation. */
  dirty:      boolean;
}

function basename(p: string | null | undefined): string {
  if (!p) return 'Untitled';
  const norm = p.replace(/\\/g, '/');
  const i = norm.lastIndexOf('/');
  return i >= 0 ? norm.slice(i + 1) : norm;
}

function createRonStudioWorkspaceStore() {
  let folder      = $state<string | null>(null);
  let files       = $state<StudioFileEntry[]>([]);
  let filesError  = $state<string | null>(null);
  let filesLoading = $state(false);

  let tabs        = $state<RonTab[]>([]);
  let activeTabId = $state<string | null>(null);
  /** Per-doc schema hint detected at parse time. Lets us re-apply the
   *  right schema when switching tabs (each .ron may bind to a
   *  different `.rs` root via directive or sidecar). */
  let schemaHintByDoc = $state<Map<string, SchemaHint | null>>(new Map());

  async function openFolder(path: string): Promise<void> {
    folder      = path;
    filesError  = null;
    filesLoading = true;
    try {
      files = await RON.listFiles(path);
    } catch (e) {
      files      = [];
      filesError = String(e);
    } finally {
      filesLoading = false;
    }
  }

  function closeFolder(): void {
    folder     = null;
    files      = [];
    filesError = null;
  }

  /** Open a `.ron` file as a new tab — or activate an existing tab
   *  when the file is already open. Returns the docId either way so
   *  the caller can wire the single-doc store to it. */
  async function openFile(path: string): Promise<string> {
    const existing = tabs.find(t => t.sourcePath === path);
    if (existing) {
      activeTabId = existing.docId;
      return existing.docId;
    }
    // Look the entry up in the active tab's studio file list so the
    // host can fall back to a cfg-keyed binding lookup when the
    // walk-up sidecar miss applies (external files outside the repo).
    const tabId   = tabsStore.activeTabId ?? undefined;
    const relPath = tabId && studioStore.loadedTabId === tabId
      ? studioStore.files.find(e => e.absolute_path === path)?.relative_path
      : undefined;
    const r = await RON.parse({ path, tabId, relativePath: relPath });
    // Pull source_path again because parse echoes the input, which may
    // be relative; we want the absolute form for cross-tab lookups.
    const abs = await RON.sourcePath(r.doc_id);
    const tab: RonTab = {
      docId:      r.doc_id,
      sourcePath: abs ?? path,
      title:      basename(abs ?? path),
      dirty:      false,
    };
    tabs        = [...tabs, tab];
    activeTabId = tab.docId;
    if (r.schema_hint) {
      const next = new Map(schemaHintByDoc);
      next.set(tab.docId, r.schema_hint);
      schemaHintByDoc = next;
    }
    return tab.docId;
  }

  async function closeTab(docId: string): Promise<{ nextActive: string | null }> {
    const idx = tabs.findIndex(t => t.docId === docId);
    if (idx < 0) return { nextActive: activeTabId };
    try { await RON.close(docId); } catch { /* best-effort */ }
    const wasActive = activeTabId === docId;
    const next      = tabs.filter(t => t.docId !== docId);
    tabs = next;
    if (wasActive) {
      // Pick the neighbour to the right, or the previous one.
      const fallback = next[idx] ?? next[idx - 1] ?? null;
      activeTabId = fallback?.docId ?? null;
    }
    return { nextActive: activeTabId };
  }

  function setActive(docId: string): void {
    activeTabId = docId;
  }

  /** Append a tab + activate it. Used by the modal to register a doc
   *  that was opened via the single-doc store path (e.g. plugin command,
   *  paste flow) — the workspace store wouldn't know about it
   *  otherwise. Skips when a tab with this docId is already present.
   *
   *  Uses array reassignment (not `tabs.push`) — `Tabs` widget's
   *  internal derivations only re-evaluate on identity change, so a
   *  mutation in place renders an empty label until the next assign. */
  function addTab(tab: RonTab): void {
    if (tabs.some(t => t.docId === tab.docId)) return;
    tabs        = [...tabs, tab];
    activeTabId = tab.docId;
  }

  /** Mark a tab's dirty flag — driven by the active doc's dirty
   *  state. The single-doc store knows when its `current !== original`;
   *  this getter just reflects that into the tab list so all tabs can
   *  show the dot, not just the active one. */
  function setDirty(docId: string, dirty: boolean): void {
    const i = tabs.findIndex(t => t.docId === docId);
    if (i < 0) return;
    if (tabs[i].dirty === dirty) return;
    tabs = tabs.map((t, ix) => ix === i ? { ...t, dirty } : t);
  }

  /** Rebind a tab's source path + title when a Save-As changes it.
   *  Keeps the same docId. */
  function rebindTab(docId: string, newPath: string): void {
    const i = tabs.findIndex(t => t.docId === docId);
    if (i < 0) return;
    tabs = tabs.map((t, ix) => ix === i
      ? { ...t, sourcePath: newPath, title: basename(newPath) }
      : t);
  }

  /** Swap a tab's docId — used when the source file gets rewritten
   *  on disk (rename refactor F12) and the host re-parses it from
   *  scratch into a fresh doc. Preserves source path + title; resets
   *  dirty to the supplied flag (typically `false` after reload).
   *  Also migrates the schema hint over so `bind` lookups by docId
   *  keep finding the right hint. */
  function replaceDocId(oldDocId: string, newDocId: string, dirty: boolean): void {
    const i = tabs.findIndex(t => t.docId === oldDocId);
    if (i < 0) return;
    tabs = tabs.map((t, ix) => ix === i
      ? { ...t, docId: newDocId, dirty }
      : t);
    if (activeTabId === oldDocId) activeTabId = newDocId;
    const hint = schemaHintByDoc.get(oldDocId);
    if (hint) {
      const next = new Map(schemaHintByDoc);
      next.delete(oldDocId);
      next.set(newDocId, hint);
      schemaHintByDoc = next;
    }
  }

  function setSchemaHint(docId: string, hint: SchemaHint | null): void {
    const next = new Map(schemaHintByDoc);
    if (hint === null) next.delete(docId); else next.set(docId, hint);
    schemaHintByDoc = next;
  }
  function getSchemaHint(docId: string): SchemaHint | null {
    return schemaHintByDoc.get(docId) ?? null;
  }

  return {
    get folder()       { return folder; },
    get files()        { return files; },
    get filesError()   { return filesError; },
    get filesLoading() { return filesLoading; },
    get tabs()         { return tabs; },
    get activeTabId()  { return activeTabId; },
    get activeTab()    { return tabs.find(t => t.docId === activeTabId) ?? null; },
    openFolder,
    closeFolder,
    openFile,
    closeTab,
    setActive,
    addTab,
    setDirty,
    rebindTab,
    replaceDocId,
    setSchemaHint,
    getSchemaHint,
    /** Re-list files in the current folder — useful after Save-As to
     *  pick up newly-created files. No-op when no folder is open. */
    async refreshFiles() {
      if (folder) await openFolder(folder);
    },
  };
}

export const ronStudioWorkspaceStore = createRonStudioWorkspaceStore();
