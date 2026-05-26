import { listen } from '@tauri-apps/api/event';
import {
  listWorkspaces, listRegistryRepos, setActiveWorkspace as ipcSetActive,
  createWorkspace as ipcCreateWs, updateWorkspace as ipcUpdateWs,
  deleteWorkspace as ipcDeleteWs, reorderWorkspaces as ipcReorderWs,
  createWorkspaceGroup as ipcCreateGroup, updateWorkspaceGroup as ipcUpdateGroup,
  deleteWorkspaceGroup as ipcDeleteGroup, reorderWorkspaceGroups as ipcReorderGroups,
  setWorkspaceGroup as ipcSetWsGroup,
  addRepoToWorkspace as ipcAddRepo, removeRepoFromWorkspace as ipcRemoveRepo,
  moveRepoBetweenWorkspaces as ipcMoveRepo,
  updateRegistryRepo as ipcUpdateRepo, deleteRegistryRepo as ipcDeleteRepo,
  saveWorkspaceSnapshot, loadWorkspaceSnapshot, registerRepoPath as ipcRegisterPath,
} from '../ipc/workspace';
import type {
  WorkspaceDef, WorkspaceGroup, RepoRegistryEntry, CrossWsTabRef, TabMeta,
  WorkspacePatch, WorkspaceGroupPatch, WorkspaceFetchProgressEvent,
  WorkspacePullProgressEvent, WorkspacePullDoneEvent,
  WorkspaceTagProgressEvent, WorkspaceTagDoneEvent,
} from '../types/workspace';
import { SCRATCH_ID } from '../types/workspace';
import { tabsStore } from './tabs.svelte';

// ---------------------------------------------------------------------------
// Cross-tab switching callbacks — set by AppShell so the store can drive
// actual opens/closes on the backend without having to import every ipc.
// ---------------------------------------------------------------------------

export interface WorkspaceTabBridge {
  /** Open a repo by path; returns once the tab is in the UI.
   *  `tabId` should be used so cross-WS restoration keeps stable ids. */
  openRepo: (path: string, tabId: string) => Promise<void>;
  /** Close a tab without touching the workspace membership.  Must use a
   *  silent-style removal (no auto-activation of siblings) — the workspace
   *  switcher clears activeTabId up-front so intermediate auto-activations
   *  don't trigger graph loads for tabs that are about to be closed. */
  closeTab: (tabId: string) => Promise<void>;
  /** Activate an already-open tab. */
  setActiveTab: (tabId: string) => void;
  /** Clear activeTabId so no subscribers load state for a tab we're about
   *  to close during a workspace swap. */
  clearActiveTab: () => void;
  /** Pull the set of currently-open tab ids.  The store compares this to
   *  the desired set when switching workspace. */
  currentOpenTabIds: () => string[];
  /** Currently active tab id in the UI. */
  currentActiveTabId: () => string | null;
  /** Per-tab metadata to persist alongside the open-tab list (name override,
   *  worktree icon, …).  Only tabs with non-default state need to appear. */
  currentTabMeta?: () => TabMeta[];
  /** Apply persisted metadata to a tab that was just opened from a snapshot. */
  applyTabMeta?: (repoId: string, meta: { nameOverride?: string | null; isLinkedWorktree?: boolean }) => void;
  /** Fire just before we swap out the tab set — gives the consumer a
   *  chance to flush in-flight work (cache saves, draft commits, …). */
  beforeSwap?: () => Promise<void>;
}

function createWorkspacesStore() {
  // ── Reactive state ────────────────────────────────────────────────────
  let workspaces  = $state<WorkspaceDef[]>([]);
  let groups      = $state<WorkspaceGroup[]>([]);
  let activeId    = $state<string | null>(null);
  let registry    = $state<RepoRegistryEntry[]>([]);
  /** Tabs in the current workspace that actually come from another one.
   *  Keyed by repo_id (== tab_id) for O(1) lookup during tab rendering. */
  let crossWsMap  = $state<Map<string, string>>(new Map()); // repo_id → source_ws_id
  /** True while a workspace switch is in progress — suspends snapshot saves. */
  let switching   = $state(false);
  let loaded      = $state(false);

  /** Optional bridge into AppShell for actual tab open/close — wired via
   *  `wire()` on startup. */
  let bridge: WorkspaceTabBridge | null = null;

  // ── Derived ───────────────────────────────────────────────────────────
  const active = $derived(workspaces.find(w => w.id === activeId) ?? null);
  const scratch = $derived(workspaces.find(w => w.id === SCRATCH_ID) ?? null);
  const registryById = $derived(new Map(registry.map(r => [r.id, r])));

  // Workspaces grouped for rendering: top-level (group_id === null) first,
  // then each group block with its children — all respecting `order`.
  const grouped = $derived.by(() => {
    type Entry = { kind: 'workspace'; ws: WorkspaceDef }
               | { kind: 'group'; group: WorkspaceGroup; children: WorkspaceDef[] };
    const out: Entry[] = [];
    const seenWs = new Set<string>();

    // Top-level (ungrouped) workspaces — excludes Scratch, which goes last.
    const topLevel = workspaces
      .filter(w => w.id !== SCRATCH_ID && !w.group_id)
      .sort((a, b) => a.order - b.order || a.name.localeCompare(b.name));
    for (const ws of topLevel) { out.push({ kind: 'workspace', ws }); seenWs.add(ws.id); }

    // Groups in persisted order, each followed by its children.
    const sortedGroups = [...groups].sort((a, b) => a.order - b.order || a.name.localeCompare(b.name));
    for (const g of sortedGroups) {
      const children = workspaces
        .filter(w => w.group_id === g.id)
        .sort((a, b) => a.order - b.order || a.name.localeCompare(b.name));
      out.push({ kind: 'group', group: g, children });
      for (const c of children) seenWs.add(c.id);
    }

    // Orphan workspaces (group_id references a deleted group): show top-level.
    for (const ws of workspaces) {
      if (ws.id === SCRATCH_ID) continue;
      if (seenWs.has(ws.id)) continue;
      out.push({ kind: 'workspace', ws });
    }

    // Scratch last.
    const s = workspaces.find(w => w.id === SCRATCH_ID);
    if (s) out.push({ kind: 'workspace', ws: s });
    return out;
  });

  // ── Lifecycle ─────────────────────────────────────────────────────────
  function wire(b: WorkspaceTabBridge) { bridge = b; }

  async function load(): Promise<void> {
    const [snap, regs] = await Promise.all([listWorkspaces(), listRegistryRepos()]);
    workspaces = snap.workspaces;
    groups     = snap.groups;
    activeId   = snap.active_workspace_id;
    registry   = regs;
    loaded = true;
  }

  /**
   * Run once at app startup, after `wire()`, to restore the tabs that were
   * open in the last active workspace.  Safe to call before any tabs have
   * been added — `applyActiveWorkspace` is a no-op when the snapshot is empty.
   */
  async function bootstrap(b: WorkspaceTabBridge): Promise<void> {
    wire(b);
    await load();
    if (activeId) {
      try { await applyActiveWorkspace(activeId); } catch { /* non-critical */ }
    }
  }

  async function reloadRegistry(): Promise<void> {
    registry = await listRegistryRepos();
  }

  // ── Listeners ─────────────────────────────────────────────────────────
  /**
   * Bind Tauri events that affect workspace UI state.
   * Returns a disposer.
   */
  function setupListeners(): () => void {
    const unsubs: Array<() => void> = [];
    listen<{ to_id: string; from_id?: string }>('arbor://workspace-switched', ev => {
      // Backend-initiated switch (e.g. from a plugin).
      if (ev.payload.to_id && ev.payload.to_id !== activeId) {
        void applyActiveWorkspace(ev.payload.to_id);
      }
    }).then(fn => unsubs.push(fn));
    return () => { for (const u of unsubs) u(); };
  }

  // ── Activation (the key flow) ─────────────────────────────────────────
  /**
   * Switch the active workspace: persist the current snapshot, close every
   * current tab that isn't in the new workspace, open new members, restore
   * cross-workspace markers, and activate the previously-focused tab.
   */
  async function setActive(newId: string): Promise<void> {
    if (newId === activeId) return;
    if (!bridge) {
      // Without the bridge we can only persist the backend state.
      await ipcSetActive(newId);
      activeId = newId;
      return;
    }

    switching = true;
    try {
      // 1) Flush any pending consumer work (cache, stats, …).
      await bridge.beforeSwap?.();

      // 2) Save current workspace's snapshot.
      if (activeId) {
        const crossList: CrossWsTabRef[] = [...crossWsMap.entries()]
          .map(([repo_id, source_ws_id]) => ({ repo_id, source_ws_id }));
        await saveWorkspaceSnapshot(
          activeId,
          bridge.currentOpenTabIds(),
          bridge.currentActiveTabId(),
          crossList,
          bridge.currentTabMeta?.() ?? [],
        );
      }

      // 3) Announce the switch to the backend (fires hook, emits event).
      await ipcSetActive(newId);

      // 4) Apply the new workspace's snapshot.
      await applyActiveWorkspace(newId);
    } finally {
      switching = false;
    }
  }

  /**
   * Load the snapshot for `wsId` and reconcile open tabs against it.
   * Called both from setActive() and when the backend announces a switch.
   *
   * Flow:
   *   1. Clear activeTabId up-front so subscribers (CommitGraph etc.) don't
   *      trigger loads for tabs during the close phase.
   *   2. Close every current tab (workspaces are exclusive — cross-WS tabs
   *      are snapshot-only and will be re-opened below if the new snapshot
   *      still lists them).
   *   3. Open every tab from the new snapshot, in order.
   *   4. Activate the last-active tab from the snapshot, if still present.
   */
  async function applyActiveWorkspace(wsId: string): Promise<void> {
    if (!bridge) return;
    const snap = await loadWorkspaceSnapshot(wsId);
    activeId = wsId;

    // Rebuild cross-WS map from the snapshot.
    crossWsMap = new Map(snap.cross_ws_tabs.map(c => [c.repo_id, c.source_ws_id]));

    // Desired tab set for the new workspace.  Members-without-snapshot are
    // NOT auto-opened — opening 30 tabs on switch would be rude; the
    // snapshot represents the last-known tabbed state.
    const wantedIds = new Set<string>(snap.open_tab_ids);

    // 1. Deactivate first — empties the graph/sidebar subscribers so the
    //    cascading closes below don't briefly re-activate tabs that are
    //    about to be torn down.
    bridge.clearActiveTab();

    // 2. Close every tab not wanted by the new workspace.
    const currentIds = bridge.currentOpenTabIds();
    for (const openId of currentIds) {
      if (!wantedIds.has(openId)) {
        try { await bridge.closeTab(openId); } catch { /* ignore */ }
      }
    }

    // 3. Open missing tabs (in snapshot order, to preserve layout).
    const metaByRepo = new Map<string, TabMeta>(
      (snap.tab_meta ?? []).map(m => [m.repo_id, m]),
    );
    const nowOpen = new Set(bridge.currentOpenTabIds());
    // Count the repos we'll actually try to open so the boot splash can
    // show a determinate progress bar while we walk the snapshot.
    const toOpen = snap.open_tab_ids.filter(
      id => !nowOpen.has(id) && registryById.has(id),
    );
    const reportInit = tabsStore.isInitializing && toOpen.length > 0;
    if (reportInit) {
      tabsStore.setInitProgress({
        current: 0,
        total:   toOpen.length,
        message: toOpen.length === 1
          ? 'Opening 1 repository…'
          : `Opening ${toOpen.length} repositories…`,
      });
    }
    let opened = 0;
    for (const repoId of snap.open_tab_ids) {
      if (nowOpen.has(repoId)) continue;
      const entry = registryById.get(repoId);
      if (!entry) continue; // registry mismatch — skip
      if (reportInit) {
        const display = entry.display_name
          || entry.path.replace(/\\/g, '/').split('/').filter(Boolean).pop()
          || 'repository';
        tabsStore.setInitProgress({
          current: opened,
          total:   toOpen.length,
          message: `Opening ${display}…`,
        });
      }
      try {
        await bridge.openRepo(entry.path, repoId);
        const meta = metaByRepo.get(repoId);
        if (meta) {
          bridge.applyTabMeta?.(repoId, {
            nameOverride:     meta.name_override,
            isLinkedWorktree: meta.is_linked_worktree,
          });
        }
      } catch { /* repo may have been deleted from disk; skip */ }
      opened += 1;
      if (reportInit && opened === toOpen.length) {
        tabsStore.setInitProgress({
          current: opened,
          total:   toOpen.length,
          message: 'Finalising workspace…',
        });
      }
    }

    // 4. Activate the previously-active tab if it's in the new set.
    if (snap.active_tab_id && bridge.currentOpenTabIds().includes(snap.active_tab_id)) {
      // Wait a tick so Svelte has flushed the newly-added tabs before we
      // trigger the graph load for the target tab.
      queueMicrotask(() => { bridge?.setActiveTab(snap.active_tab_id!); });
    }
  }

  /** Persist a snapshot for the current workspace.  No-op during switching
   *  (the switch itself is responsible for the final save). */
  async function persistSnapshotNow(): Promise<void> {
    if (switching || !activeId || !bridge) return;
    const cross: CrossWsTabRef[] = [...crossWsMap.entries()].map(([repo_id, source_ws_id]) => ({ repo_id, source_ws_id }));
    try {
      await saveWorkspaceSnapshot(
        activeId,
        bridge.currentOpenTabIds(),
        bridge.currentActiveTabId(),
        cross,
        bridge.currentTabMeta?.() ?? [],
      );
    } catch { /* non-critical */ }
  }

  // ── Cross-WS tracking ─────────────────────────────────────────────────
  /** Mark a repo id as a cross-workspace tab belonging to `sourceWsId`. */
  function markCrossWs(repoId: string, sourceWsId: string): void {
    crossWsMap = new Map(crossWsMap).set(repoId, sourceWsId);
  }
  function unmarkCrossWs(repoId: string): void {
    if (!crossWsMap.has(repoId)) return;
    const m = new Map(crossWsMap);
    m.delete(repoId);
    crossWsMap = m;
  }
  function sourceWsFor(repoId: string): string | null {
    return crossWsMap.get(repoId) ?? null;
  }

  // ── CRUD pass-throughs (reload on success so derived state stays fresh) ──

  async function createWorkspace(name: string, colorIdx: number, repoIds: string[], groupId: string | null): Promise<WorkspaceDef> {
    const ws = await ipcCreateWs(name, colorIdx, repoIds, groupId);
    await load();
    return ws;
  }
  async function updateWorkspace(id: string, patch: WorkspacePatch): Promise<void> {
    await ipcUpdateWs(id, patch);
    await load();
  }
  async function deleteWorkspace(id: string): Promise<void> {
    await ipcDeleteWs(id);
    await load();
  }
  async function reorderWorkspaces(ordered: string[]): Promise<void> {
    await ipcReorderWs(ordered);
    await load();
  }

  async function createGroup(name: string, colorIdx: number): Promise<WorkspaceGroup> {
    const g = await ipcCreateGroup(name, colorIdx);
    await load();
    return g;
  }
  async function updateGroup(id: string, patch: WorkspaceGroupPatch): Promise<void> {
    await ipcUpdateGroup(id, patch);
    await load();
  }
  async function deleteGroup(id: string): Promise<void> {
    await ipcDeleteGroup(id);
    await load();
  }
  async function reorderGroups(ordered: string[]): Promise<void> {
    await ipcReorderGroups(ordered);
    await load();
  }
  async function assignGroup(workspaceId: string, groupId: string | null): Promise<void> {
    await ipcSetWsGroup(workspaceId, groupId);
    await load();
  }
  async function toggleGroupCollapsed(groupId: string): Promise<void> {
    const g = groups.find(g => g.id === groupId);
    if (!g) return;
    await ipcUpdateGroup(groupId, { collapsed: !g.collapsed });
    await load();
  }

  async function addRepoTo(workspaceId: string, repoId: string): Promise<void> {
    await ipcAddRepo(workspaceId, repoId);
    await load();
  }
  async function removeRepoFrom(workspaceId: string, repoId: string): Promise<void> {
    await ipcRemoveRepo(workspaceId, repoId);
    // If the repo is currently open as a tab in the active workspace, close
    // the tab too. Otherwise the snapshot keeps listing it in `open_tab_ids`
    // and the next time this workspace is loaded the tab gets resurrected
    // for a repo that's no longer a member.
    if (workspaceId === activeId && bridge) {
      const isOpen = bridge.currentOpenTabIds().includes(repoId);
      const wasActive = bridge.currentActiveTabId() === repoId;
      if (isOpen) {
        try { await bridge.closeTab(repoId); } catch { /* ignore — graph handle may be gone already */ }
        // bridge.closeTab uses the silent removal path (designed for the
        // workspace-swap loop that clears activeTabId up front). When the
        // user removes the currently-active tab from the manager modal,
        // we need to shift focus to a sibling so the graph doesn't stay
        // stuck on a no-longer-open id.
        if (wasActive) {
          const next = tabsStore.tabs[0]?.id ?? null;
          if (next) tabsStore.setActive(next); else tabsStore.clearActive();
        }
      }
      // Persist the updated tab set right away — without this the workspace
      // snapshot would still hold the removed repo until the next tab-edit
      // event triggers `persistHook`.
      await persistSnapshotNow();
    }
    await load();
  }
  async function moveRepoBetween(from: string, to: string, repoId: string): Promise<void> {
    await ipcMoveRepo(from, to, repoId);
    await load();
  }

  async function renameRepo(repoId: string, displayName: string): Promise<void> {
    await ipcUpdateRepo(repoId, { display_name: displayName });
    await reloadRegistry();
  }

  /** Register a path in the central registry without GAINING workspace
   *  membership.  Used by the worktree switcher: the swapped worktree path
   *  needs a stable repo_id (so its meta can be persisted) but a transient
   *  context-swap shouldn't add the new path to the active workspace.
   *
   *  Critically, we only undo the auto-add when THIS call was the one to
   *  add the repo (`res.added_to_ws`).  If the path was already a deliberate
   *  member (the typical case: switching back to the main worktree, or to a
   *  worktree the user explicitly added), removing it here would silently
   *  delete a real workspace membership — and the user would see "their"
   *  project disappear from the workspace, plus sibling worktrees lose their
   *  workspace inheritance via `common_dir`. */
  async function registerPathTransient(path: string): Promise<string> {
    const res = await ipcRegisterPath(path, null, null);
    await reloadRegistry();
    if (activeId && res.added_to_ws) {
      try { await ipcRemoveRepo(activeId, res.id); } catch { /* ignore — likely already gone */ }
      await load();
    }
    return res.id;
  }
  async function relocateRepo(repoId: string, newPath: string): Promise<void> {
    await ipcUpdateRepo(repoId, { path: newPath });
    await reloadRegistry();
  }
  async function deregisterRepo(repoId: string): Promise<void> {
    await ipcDeleteRepo(repoId);
    await load();
  }

  /**
   * Canonical "I'm about to open this path" helper.  Registers the path (if
   * new) and guarantees it's a member of the active workspace — unless the
   * caller flags `allowCrossWs`, in which case we DO NOT auto-add and just
   * mark the tab as cross-workspace (dot on tab).  Returns the repo id.
   */
  async function ensureRepoRegistered(
    path: string,
    remoteUrl: string | null = null,
    displayName: string | null = null,
    options: { allowCrossWs?: boolean; sourceWsId?: string } = {},
  ): Promise<string> {
    const res = await ipcRegisterPath(path, remoteUrl, displayName);
    // Refresh the registry cache so derived selectors see the new entry.
    await reloadRegistry();
    // If this opened a repo owned by another workspace we want to mark it
    // as cross-WS in the active workspace's snapshot.  The backend always
    // auto-adds the repo to the active workspace via register_repo_path, so
    // `allowCrossWs` means "undo that add".
    if (options.allowCrossWs && options.sourceWsId && activeId && options.sourceWsId !== activeId) {
      if (res.added_to_ws) {
        try { await ipcRemoveRepo(activeId, res.id); } catch { /* ignore */ }
      }
      markCrossWs(res.id, options.sourceWsId);
      await load();
    } else {
      // Either a fresh registration or a normal open — reload to see the
      // membership change in `workspaces`.
      if (res.added_to_ws) await load();
    }
    return res.id;
  }

  /** Listen for workspace-fetch-progress events — external components
   *  (management modal) render per-repo spinners from this. */
  function onFetchProgress(handler: (ev: WorkspaceFetchProgressEvent) => void): () => void {
    let unlisten: (() => void) | null = null;
    listen<WorkspaceFetchProgressEvent>('arbor://workspace-fetch-progress', ev => handler(ev.payload))
      .then(fn => { unlisten = fn; })
      .catch(() => {});
    return () => { unlisten?.(); };
  }
  function onFetchDone(handler: (ev: { job_id: string; workspace_id: string; ok: number; failed: number }) => void): () => void {
    let unlisten: (() => void) | null = null;
    listen<{ job_id: string; workspace_id: string; ok: number; failed: number }>('arbor://workspace-fetch-done', ev => handler(ev.payload))
      .then(fn => { unlisten = fn; })
      .catch(() => {});
    return () => { unlisten?.(); };
  }

  /** Pull-all progress events (mirror of onFetchProgress but with a
   *  `conflict` phase for pulls that left MERGE_HEAD behind). */
  function onPullProgress(handler: (ev: WorkspacePullProgressEvent) => void): () => void {
    let unlisten: (() => void) | null = null;
    listen<WorkspacePullProgressEvent>('arbor://workspace-pull-progress', ev => handler(ev.payload))
      .then(fn => { unlisten = fn; })
      .catch(() => {});
    return () => { unlisten?.(); };
  }
  function onPullDone(handler: (ev: WorkspacePullDoneEvent) => void): () => void {
    let unlisten: (() => void) | null = null;
    listen<WorkspacePullDoneEvent>('arbor://workspace-pull-done', ev => handler(ev.payload))
      .then(fn => { unlisten = fn; })
      .catch(() => {});
    return () => { unlisten?.(); };
  }

  /** Tag-all progress events — same shape as fetch-all but with a
   *  `skipped` phase for repos in detached HEAD. */
  function onTagProgress(handler: (ev: WorkspaceTagProgressEvent) => void): () => void {
    let unlisten: (() => void) | null = null;
    listen<WorkspaceTagProgressEvent>('arbor://workspace-tag-progress', ev => handler(ev.payload))
      .then(fn => { unlisten = fn; })
      .catch(() => {});
    return () => { unlisten?.(); };
  }
  function onTagDone(handler: (ev: WorkspaceTagDoneEvent) => void): () => void {
    let unlisten: (() => void) | null = null;
    listen<WorkspaceTagDoneEvent>('arbor://workspace-tag-done', ev => handler(ev.payload))
      .then(fn => { unlisten = fn; })
      .catch(() => {});
    return () => { unlisten?.(); };
  }

  return {
    // reactive getters
    get workspaces()   { return workspaces; },
    get groups()       { return groups; },
    get activeId()     { return activeId; },
    get active()       { return active; },
    get scratch()      { return scratch; },
    get registry()     { return registry; },
    get registryById() { return registryById; },
    get grouped()      { return grouped; },
    get switching()    { return switching; },
    get loaded()       { return loaded; },
    get crossWsMap()   { return crossWsMap; },

    // lifecycle
    wire, load, bootstrap, reloadRegistry, setupListeners,

    // activation
    setActive, persistSnapshotNow,

    // cross-WS helpers
    markCrossWs, unmarkCrossWs, sourceWsFor,

    // CRUD
    createWorkspace, updateWorkspace, deleteWorkspace, reorderWorkspaces,
    createGroup, updateGroup, deleteGroup, reorderGroups, assignGroup, toggleGroupCollapsed,
    addRepoTo, removeRepoFrom, moveRepoBetween,
    renameRepo, relocateRepo, deregisterRepo, ensureRepoRegistered, registerPathTransient,

    // fetch events
    onFetchProgress, onFetchDone,
    // pull events
    onPullProgress, onPullDone,
    // tag events
    onTagProgress, onTagDone,
  };
}

export const workspacesStore = createWorkspacesStore();
