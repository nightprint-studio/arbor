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
} from '../../ipc/corvus/workspace';
import type {
  WorkspaceDef, WorkspaceGroup, RepoRegistryEntry, CrossWsTabRef, TabMeta,
  WorkspacePatch, WorkspaceGroupPatch, WorkspaceFetchProgressEvent,
  WorkspacePullProgressEvent, WorkspacePullDoneEvent,
  WorkspaceTagProgressEvent, WorkspaceTagDoneEvent,
} from '../../types/corvus/workspace';
import { SCRATCH_ID } from '../../types/corvus/workspace';
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
  /** Put the open tabs in the order of `ids` (ids not listed keep their place at the end). Called after
   *  each tab of a restored workspace opens, so a tab that finished early still lands where the
   *  snapshot had it. */
  orderTabs?: (ids: string[]) => void;
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
      // Backend-initiated switch (e.g. from a plugin). Our own switch emits this event too,
      // while `activeId` still names the old workspace — applying it here would run the whole
      // close/open pass a second time, concurrently with the one `setActive` is driving.
      if (switching) return;
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
   *
   * The round trips that do not depend on each other run together — saving the old snapshot,
   * reading the new one and announcing the switch (which waits on every plugin's
   * `on_workspace_switched`) — and the announcement is NOT waited for before the tabs change:
   * the switch is finished when the user can see and use the new workspace, not when every
   * plugin has heard about it.
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

      // 2) Save the current workspace's snapshot, read the next one, announce the switch.
      let saved: Promise<unknown> = Promise.resolve();
      if (activeId) {
        const crossList: CrossWsTabRef[] = [...crossWsMap.entries()]
          .map(([repo_id, source_ws_id]) => ({ repo_id, source_ws_id }));
        saved = saveWorkspaceSnapshot(
          activeId,
          bridge.currentOpenTabIds(),
          bridge.currentActiveTabId(),
          crossList,
          bridge.currentTabMeta?.() ?? [],
        );
      }
      const announced = ipcSetActive(newId);
      const snap = await loadWorkspaceSnapshot(newId);
      await saved;

      // 3) Apply the new workspace's snapshot. The announcement is settled in the background; if the
      //    backend refused the switch, the state is re-read from it rather than left half-applied.
      const settle = announced.catch(() => { void load(); });
      await applyActiveWorkspace(newId, { snapshot: snap, settle });
    } catch (err) {
      switching = false;
      throw err;
    }
  }

  /** Bumped by every apply; a pass that finds it moved on has been overtaken by a newer switch. */
  let applyGeneration = 0;
  /** How many tabs of a restored workspace are opened at once, behind the one on screen. */
  const BACKGROUND_OPENS = 3;

  /**
   * Load the snapshot for `wsId` and reconcile open tabs against it.
   * Called both from setActive() and when the backend announces a switch.
   *
   * Flow:
   *   1. Clear activeTabId up-front so subscribers (CommitGraph etc.) don't
   *      trigger loads for tabs during the close phase.
   *   2. Drop every tab the new workspace does not want — from the UI at once, the backend is told
   *      alongside (workspaces are exclusive — cross-WS tabs are snapshot-only and will be
   *      re-opened below if the new snapshot still lists them).
   *   3. Open the tab that was active FIRST and activate it, so the graph starts loading while
   *      the rest are still being brought up.
   *   4. Open the remaining tabs behind it, a few at a time, each placed at its snapshot position.
   *
   * `switching` stays on until every tab is open: a snapshot saved from a half-restored tab set
   * would overwrite the workspace's real one.
   */
  async function applyActiveWorkspace(
    wsId: string,
    opts: { snapshot?: Awaited<ReturnType<typeof loadWorkspaceSnapshot>>; settle?: Promise<unknown> } = {},
  ): Promise<void> {
    if (!bridge) return;
    const gen = ++applyGeneration;
    switching = true;
    const pendingWork: Promise<unknown>[] = opts.settle ? [opts.settle] : [];
    // Switching ends when the whole pass has — and only if no newer switch has taken over.
    const finish = async () => {
      await Promise.allSettled(pendingWork);
      if (gen !== applyGeneration) return;
      switching = false;
      void persistSnapshotNow();
    };

    try {
      const snap = opts.snapshot ?? await loadWorkspaceSnapshot(wsId);
      if (gen !== applyGeneration) return;
      activeId = wsId;

      // Rebuild cross-WS map from the snapshot.
      crossWsMap = new Map(snap.cross_ws_tabs.map(c => [c.repo_id, c.source_ws_id]));

      // Desired tab set for the new workspace.  Members-without-snapshot are
      // NOT auto-opened — opening 30 tabs on switch would be rude; the
      // snapshot represents the last-known tabbed state.
      const order = snap.open_tab_ids;
      const wanted = new Set<string>(order);

      // 1. Deactivate first — empties the graph/sidebar subscribers so the
      //    cascading closes below don't briefly re-activate tabs that are
      //    about to be torn down.
      bridge.clearActiveTab();

      // 2. Close every tab not wanted by the new workspace. The bridge removes the tab from the UI
      //    synchronously and only then talks to the backend, so they all disappear at once and the
      //    backend work (which waits on `on_repo_close` hooks) overlaps everything below.
      for (const openId of bridge.currentOpenTabIds()) {
        if (!wanted.has(openId)) pendingWork.push(bridge.closeTab(openId).catch(() => { /* ignore */ }));
      }

      // 3. What has to be opened, and which of it the user will look at.
      const metaByRepo = new Map<string, TabMeta>(
        (snap.tab_meta ?? []).map(m => [m.repo_id, m]),
      );
      const nowOpen = new Set(bridge.currentOpenTabIds());
      const toOpen = order.filter(id => !nowOpen.has(id) && registryById.has(id));
      // Count the repos we'll actually try to open so the boot splash can
      // show a determinate progress bar while we walk the snapshot.
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
      const openOne = async (repoId: string) => {
        const entry = registryById.get(repoId);
        if (!entry) return; // registry mismatch — skip
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
          await bridge?.openRepo(entry.path, repoId);
          const meta = metaByRepo.get(repoId);
          if (meta) {
            bridge?.applyTabMeta?.(repoId, {
              nameOverride:     meta.name_override,
              isLinkedWorktree: meta.is_linked_worktree,
            });
          }
          // Tabs arrive in the order they finish, not the order they sit in: put this one where
          // the snapshot had it.
          bridge?.orderTabs?.(order);
        } catch { /* repo may have been deleted from disk; skip */ }
        opened += 1;
        if (reportInit && opened === toOpen.length) {
          tabsStore.setInitProgress({
            current: opened,
            total:   toOpen.length,
            message: 'Finalising workspace…',
          });
        }
      };

      // The tab that was active, opened first and activated the moment it exists: the graph and the
      // sidebar start loading now instead of after every other repository has been opened.
      const target = snap.active_tab_id && order.includes(snap.active_tab_id) ? snap.active_tab_id : null;
      if (target && toOpen.includes(target)) await openOne(target);
      if (gen !== applyGeneration) return;
      if (target && bridge.currentOpenTabIds().includes(target)) bridge.setActiveTab(target);

      // 4. The rest, a few at a time, behind it. Opening is not an index mutation, so it may overlap;
      //    each worker stops as soon as a newer switch has taken over.
      const rest = toOpen.filter(id => id !== target);
      const openRest = async () => {
        const queue = [...rest];
        const worker = async () => {
          for (let id = queue.shift(); id !== undefined; id = queue.shift()) {
            if (gen !== applyGeneration) return;
            await openOne(id);
          }
        };
        await Promise.all(Array.from({ length: Math.min(BACKGROUND_OPENS, queue.length) }, worker));
      };

      // At startup the boot splash is still up and counts on the tabs being all there; on a switch
      // the user is already looking at the target, so the rest fills in behind it.
      if (tabsStore.isInitializing) await openRest();
      else pendingWork.push(openRest());
    } finally {
      void finish();
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
    renameRepo, deregisterRepo, ensureRepoRegistered, registerPathTransient,

    // fetch events
    onFetchProgress, onFetchDone,
    // pull events
    onPullProgress, onPullDone,
    // tag events
    onTagProgress, onTagDone,
  };
}

export const workspacesStore = createWorkspacesStore();
