/**
 * Hirundo workspace — the open project, its collections tree, its environments, and the request tabs.
 *
 * A tab holds a **draft** of a request next to the last saved text, so "dirty" is a comparison rather than a
 * flag someone has to remember to set. Sending goes through the backend (`request_send`), which refuses to
 * send while a variable has no value and while a protected environment has not been confirmed; the store
 * only carries those answers to the window.
 *
 * The response lives on its tab, with the one before it (for the diff) — session state, never persisted.
 */
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import {
  collectionsTree, curlImport, metaGet, metaSave, envState, getConfig, localSave, nodeDelete, projectForget, projectOpened,
  requestDuplicate, requestGet, requestSave, requestSend, requestCancel, testsRun,
  workspaceMove, workspaceMoveProject, projectCreate, workspaceAddProject, workspaceCreate, workspaceDelete, workspaceRemoveProject, workspaceSwitch, workspaceUpdate, detectedRootsSet,
} from '$lib/ipc/hirundo';
import { blankRequest, cloneRequest, sameRequest } from '$lib/components/hirundo/request-model';
import type {
  CollectionNode, EnvBrief, EnvState, EnvType, Environment, HirundoConfig, HttpRequest, Local, ResponseData, Target, TestReport,
} from '$lib/types/hirundo';

export interface HistoryEntry {
  at: number;
  name: string;
  method: string;
  url: string;
  status: number | null;
  time_ms: number | null;
  /** The request as it was sent, so a history row can be reopened. */
  request: HttpRequest;
}

export interface RequestTab {
  id: string;
  /** Where it is saved; `null` until it is (a new request, or a pasted cURL). */
  path: string | null;
  /** Collection or folder a new request will be saved in. */
  parent: string | null;
  request: HttpRequest;
  /** The last saved (or loaded) text, for the dirty dot. */
  saved: HttpRequest;
  /** Replaced by the next single-click open; becomes permanent when edited or double-clicked. */
  preview: boolean;
  /** Environment for this tab only; `null` follows the active one. */
  envId: string | null;
  sending: boolean;
  requestId: string | null;
  response: ResponseData | null;
  previous: ResponseData | null;
  error: string | null;
  /** Variables the last attempt could not resolve (nothing was sent). */
  unresolved: string[];
  missingSecrets: string[];
  report: TestReport | null;
}

export interface PendingConfirm {
  tabId: string;
  env: EnvBrief | null;
  method: string;
  url: string;
}

const MAX_HISTORY = 100;
let tabCounter = 0;
let sendCounter = 0;

function createHirundoWorkspace() {
  let project = $state<string | null>(null);
  let config = $state<HirundoConfig>({ last_project: '', active_workspace: '', recent_projects: [], detected_roots: [], workspaces: [] });
  let tree = $state<CollectionNode[]>([]);
  let envs = $state<EnvState>({ types: [], environments: [], local: { active_env: '', vars: [] } });
  let tabs = $state<RequestTab[]>([]);
  let activeId = $state<string | null>(null);
  let history = $state<HistoryEntry[]>([]);
  let loading = $state(false);
  let pending = $state<PendingConfirm | null>(null);

  const active = $derived(tabs.find((t) => t.id === activeId) ?? null);

  /** The environment the active tab goes to: its own override, else the active one. */
  const activeEnv = $derived.by<Environment | null>(() => {
    const id = active?.envId || envs.local.active_env;
    return envs.environments.find((e) => e.id === id) ?? null;
  });
  const activeEnvType = $derived<EnvType | null>(envs.types.find((t) => t.id === activeEnv?.type_id) ?? null);

  function fail(e: unknown) {
    toastStore.show(String(e), 'error');
  }

  function target(tab: RequestTab): Target {
    return { project: project ?? '', request: tab.request, path: tab.path, env_id: tab.envId };
  }

  function makeTab(path: string | null, request: HttpRequest, preview: boolean, parent: string | null = null): RequestTab {
    return {
      id: `tab-${++tabCounter}`, path, parent, request, saved: cloneRequest(request), preview, envId: null,
      sending: false, requestId: null, response: null, previous: null, error: null, unresolved: [], missingSecrets: [], report: null,
    };
  }

  async function refreshTree() {
    if (!project) return;
    try { tree = await collectionsTree(project); } catch (e) { fail(e); }
  }

  async function refreshEnvs() {
    if (!project) return;
    try { envs = await envState(project); } catch (e) { fail(e); }
  }

  async function openProject(path: string) {
    loading = true;
    try {
      config = await projectOpened(path);
      project = path;
      tabs = [];
      activeId = null;
      history = [];
      await Promise.all([refreshTree(), refreshEnvs()]);
    } catch (e) {
      fail(e);
    } finally {
      loading = false;
    }
  }

  /** Reopen on the last project, if it is still there. Never opens one on its own authority: it only restores. */
  async function init() {
    try {
      const cfg = await getConfig();
      config = cfg;
      if (cfg.last_project) await openProject(cfg.last_project);
    } catch {
      // The backend races the first read; `arbor://hirundo-be-up` calls init again.
    }
  }

  async function forgetProject(path: string) {
    try { config = await projectForget(path); } catch (e) { fail(e); }
  }

  /** Switch to a workspace and to the project it was last on (its first, when none was). */
  async function switchWorkspace(id: string) {
    try {
      config = await workspaceSwitch(id);
      const ws = config.workspaces.find((w) => w.id === id);
      const target = ws?.projects.find((p) => p === project) ?? ws?.projects[0];
      if (target && target !== project) await openProject(target);
      else if (!target) closeProject();
    } catch (e) { fail(e); }
  }

  async function switchToProject(workspaceId: string, root: string) {
    try {
      if (config.active_workspace !== workspaceId) config = await workspaceSwitch(workspaceId);
      await openProject(root);
    } catch (e) { fail(e); }
  }

  async function createWorkspace(name: string) {
    try { config = await workspaceCreate(name); } catch (e) { fail(e); }
  }

  async function updateWorkspace(id: string, name: string, colorIdx: number) {
    try { config = await workspaceUpdate(id, name, colorIdx); } catch (e) { fail(e); }
  }

  async function deleteWorkspace(id: string) {
    try { config = await workspaceDelete(id); } catch (e) { fail(e); }
  }

  async function addProjectTo(id: string, root: string) {
    try { config = await workspaceAddProject(id, root); } catch (e) { fail(e); }
  }

  async function removeProjectFrom(id: string, root: string) {
    try { config = await workspaceRemoveProject(id, root); } catch (e) { fail(e); }
  }

  async function moveWorkspace(id: string, gap: number) {
    try { config = await workspaceMove(id, gap); } catch (e) { fail(e); }
  }

  async function moveProject(fromId: string, root: string, toId: string, gap: number) {
    try { config = await workspaceMoveProject(fromId, root, toId, gap); } catch (e) { fail(e); }
  }

  /** Make a project (a folder with `.hirundo/` inside) and open it; it joins the active workspace like any opened project. */
  async function createProject(location: string, name: string) {
    try {
      const path = await projectCreate(location, name);
      await openProject(path);
      return path;
    } catch (e) {
      fail(e);
      return null;
    }
  }

  async function setDetectedRoots(roots: string[]) {
    try { config = await detectedRootsSet(roots); } catch (e) { fail(e); }
  }

  /** Rename a request, folder or collection: its name inside the file (the file keeps its name, so links and diffs stay put). */
  async function renameNode(path: string, name: string) {
    if (!project || !name.trim()) return;
    try {
      if (path.endsWith('.toml')) {
        const request = await requestGet(project, path);
        request.name = name;
        await requestSave(project, request, { path });
        const open = tabs.find((t) => t.path === path);
        if (open) { open.request.name = name; open.saved.name = name; }
      } else {
        const meta = await metaGet(project, path);
        meta.name = name;
        await metaSave(project, path, meta);
      }
      await refreshTree();
    } catch (e) { fail(e); }
  }

  function closeProject() {
    project = null;
    tree = [];
    tabs = [];
    activeId = null;
  }

  async function openRequest(path: string, preview = true) {
    const existing = tabs.find((t) => t.path === path);
    if (existing) {
      if (!preview) existing.preview = false;
      activeId = existing.id;
      return;
    }
    if (!project) return;
    try {
      const request = await requestGet(project, path);
      const tab = makeTab(path, request, preview);
      // A preview tab is replaced by the next preview open, like a file in an IDE.
      const at = tabs.findIndex((t) => t.preview && !dirty(t));
      if (preview && at !== -1) tabs.splice(at, 1, tab); else tabs.push(tab);
      activeId = tab.id;
    } catch (e) {
      fail(e);
    }
  }

  function newRequest(parent: string | null = null, request: HttpRequest = blankRequest()) {
    const tab = makeTab(null, request, false, parent);
    tabs.push(tab);
    activeId = tab.id;
    return tab;
  }

  /** `curl …` → a new, unsaved request. */
  async function importCurl(text: string, parent: string | null) {
    try {
      const request = await curlImport(text);
      newRequest(parent, request);
    } catch (e) {
      fail(e);
    }
  }

  function dirty(tab: RequestTab): boolean {
    return tab.path === null || !sameRequest(tab.request, tab.saved);
  }

  function closeTab(id: string) {
    const i = tabs.findIndex((t) => t.id === id);
    if (i === -1) return;
    tabs.splice(i, 1);
    if (activeId === id) activeId = (tabs[i] ?? tabs[i - 1])?.id ?? null;
  }

  /** What a close-many is waiting on: tabs with edits that would be lost. */
  let closeConfirm = $state<{ ids: string[]; keep: string | null; edited: number } | null>(null);

  /** Edits worth asking about: a saved request that changed, or a new one that has something in it. */
  function hasEdits(tab: RequestTab): boolean {
    return tab.path ? !sameRequest(tab.request, tab.saved) : tab.request.url.trim() !== '' || tab.request.name.trim() !== '';
  }

  function closeMany(ids: string[], keep: string | null) {
    const at = Math.max(0, tabs.findIndex((t) => t.id === activeId));
    tabs = tabs.filter((t) => !ids.includes(t.id));
    if (!tabs.some((t) => t.id === activeId)) activeId = (keep && tabs.some((t) => t.id === keep) ? keep : (tabs[Math.min(at, tabs.length - 1)]?.id ?? null));
  }

  /** Close tabs, asking first when any holds edits. `keep` is the tab that stays (and becomes active). */
  function requestClose(ids: string[], keep: string | null = null) {
    if (!ids.length) return;
    const edited = tabs.filter((t) => ids.includes(t.id) && hasEdits(t)).length;
    if (edited) closeConfirm = { ids, keep, edited };
    else closeMany(ids, keep);
  }

  function confirmClose() {
    if (closeConfirm) closeMany(closeConfirm.ids, closeConfirm.keep);
    closeConfirm = null;
  }

  function dismissClose() {
    closeConfirm = null;
  }

  /** The tab-strip verbs: `id` is the tab that was clicked, the rest is relative to it. */
  function closeTabs(kind: 'one' | 'others' | 'right' | 'all' | 'saved', id: string | null = activeId) {
    const i = tabs.findIndex((t) => t.id === id);
    if (kind === 'all') return requestClose(tabs.map((t) => t.id));
    if (kind === 'saved') return closeMany(tabs.filter((t) => !dirty(t)).map((t) => t.id), null);
    if (i === -1) return;
    if (kind === 'one') requestClose([tabs[i].id]);
    else if (kind === 'others') requestClose(tabs.filter((t) => t.id !== id).map((t) => t.id), id);
    else requestClose(tabs.slice(i + 1).map((t) => t.id), id);
  }

  /** Move a tab to a new place in the strip (the order lives as long as the window's tabs do). */
  function reorderTab(from: number, to: number) {
    if (from === to || from < 0 || to < 0 || from >= tabs.length || to >= tabs.length) return;
    const [moved] = tabs.splice(from, 1);
    tabs.splice(to, 0, moved);
  }

  /** Nudge the active tab one place left (-1) or right (1). */
  function moveActive(delta: number) {
    const i = tabs.findIndex((t) => t.id === activeId);
    if (i !== -1) reorderTab(i, i + delta);
  }

  /** A saved request is renamed in its file; a new one only in the tab. */
  async function renameTab(id: string, name: string) {
    const tab = tabs.find((t) => t.id === id);
    if (!tab || !name.trim()) return;
    if (tab.path) await renameNode(tab.path, name.trim());
    else tab.request.name = name.trim();
  }

  function activate(id: string) {
    activeId = id;
  }

  function step(delta: number) {
    if (!tabs.length) return;
    const i = Math.max(0, tabs.findIndex((t) => t.id === activeId));
    activeId = tabs[(i + delta + tabs.length) % tabs.length].id;
  }

  /** Called on any edit of a tab: a preview that is edited is kept. */
  function touch(tab: RequestTab) {
    tab.preview = false;
  }

  async function save(tab: RequestTab, parentOverride?: string) {
    if (!project) return;
    try {
      const where = tab.path ? { path: tab.path } : { parent: parentOverride ?? tab.parent ?? tree[0]?.path ?? '' };
      if (!tab.path && !('parent' in where && where.parent)) {
        toastStore.show('Create a collection first, then save the request in it.', 'warning');
        return;
      }
      tab.path = await requestSave(project, tab.request, where);
      tab.saved = cloneRequest(tab.request);
      tab.preview = false;
      await refreshTree();
    } catch (e) {
      fail(e);
    }
  }

  /** A copy of the request, saved next to the original, opened in its own tab. */
  async function duplicate(tab: RequestTab) {
    if (!project) return;
    try {
      if (!tab.path) {
        newRequest(tab.parent, { ...cloneRequest(tab.request), name: `${tab.request.name} copy` });
        return;
      }
      const path = await requestDuplicate(project, tab.path);
      await refreshTree();
      await openRequest(path, false);
    } catch (e) {
      fail(e);
    }
  }

  async function deleteNode(path: string) {
    if (!project) return;
    try {
      await nodeDelete(project, path);
      // Tabs on what is gone close; their drafts go with them.
      for (const t of [...tabs]) if (t.path && (t.path === path || t.path.startsWith(`${path}/`))) closeTab(t.id);
      await refreshTree();
    } catch (e) {
      fail(e);
    }
  }

  async function setLocal(local: Local) {
    if (!project) return;
    try {
      await localSave(project, local);
      envs.local = local;
    } catch (e) {
      fail(e);
    }
  }

  /** Switch the environment every tab follows. */
  async function selectEnvironment(id: string) {
    await setLocal({ ...envs.local, active_env: id });
  }

  async function send(tab: RequestTab | null = active, confirmed = false) {
    if (!tab || !project || tab.sending) return;
    tab.sending = true;
    tab.error = null;
    tab.unresolved = [];
    tab.missingSecrets = [];
    const requestId = `send-${++sendCounter}`;
    tab.requestId = requestId;
    try {
      const result = await requestSend(target(tab), requestId, confirmed);
      if (result.outcome === 'unresolved') {
        tab.unresolved = result.names;
        tab.missingSecrets = result.missing_secrets;
      } else if (result.outcome === 'needs_confirmation') {
        pending = { tabId: tab.id, env: result.env, method: result.method, url: result.url };
      } else {
        tab.previous = tab.response;
        tab.response = result.response;
        record(tab, result.response);
        await runTests(tab);
      }
    } catch (e) {
      const text = String(e);
      if (!text.includes('cancelled')) tab.error = text;
    } finally {
      tab.sending = false;
      tab.requestId = null;
    }
  }

  async function confirmPending() {
    const p = pending;
    pending = null;
    const tab = tabs.find((t) => t.id === p?.tabId);
    if (tab) await send(tab, true);
  }

  function dismissPending() {
    pending = null;
  }

  async function cancel(tab: RequestTab | null = active) {
    if (tab?.requestId) {
      try { await requestCancel(tab.requestId); } catch (e) { fail(e); }
    }
  }

  /** Assertions + Lua against the tab's response; values the script set become local variables. */
  async function runTests(tab: RequestTab) {
    tab.report = null;
    if (!tab.request.tests.trim() || !tab.response) return;
    try {
      tab.report = await testsRun(tab.request.tests, tab.response);
      const set = tab.report.set_vars;
      if (set.length) {
        const vars = envs.local.vars.filter((v) => !set.some(([k]) => k === v.key));
        for (const [key, value] of set) vars.push({ key, value, description: 'set by a test', enabled: true });
        await setLocal({ ...envs.local, vars });
      }
    } catch (e) {
      fail(e);
    }
  }

  function record(tab: RequestTab, response: ResponseData | null) {
    history.unshift({
      at: Date.now(), name: tab.request.name, method: tab.request.method, url: tab.request.url,
      status: response?.status ?? null, time_ms: response?.time_ms ?? null, request: cloneRequest(tab.request),
    });
    if (history.length > MAX_HISTORY) history.length = MAX_HISTORY;
  }

  function openFromHistory(entry: HistoryEntry) {
    newRequest(null, cloneRequest(entry.request));
  }

  return {
    get project() { return project; },
    get config() { return config; },
    get recents() { return config.recent_projects; },
    get workspaces() { return config.workspaces; },
    get activeWorkspace() { return config.workspaces.find((w) => w.id === config.active_workspace) ?? null; },
    get tree() { return tree; },
    get envs() { return envs; },
    get tabs() { return tabs; },
    get active() { return active; },
    get activeId() { return activeId; },
    get activeEnv() { return activeEnv; },
    get activeEnvType() { return activeEnvType; },
    get history() { return history; },
    get loading() { return loading; },
    get pending() { return pending; },
    get closeConfirm() { return closeConfirm; },

    init, openProject, switchWorkspace, switchToProject, createWorkspace, updateWorkspace, deleteWorkspace, addProjectTo, removeProjectFrom, setDetectedRoots, moveWorkspace, moveProject, createProject,
    closeProject, forgetProject, refreshTree, refreshEnvs,
    openRequest, newRequest, importCurl, dirty, closeTab, closeTabs, confirmClose, dismissClose, reorderTab, moveActive, renameTab, activate, step, touch,
    save, duplicate, deleteNode, renameNode, setLocal, selectEnvironment,
    send, cancel, runTests, confirmPending, dismissPending, openFromHistory, target,
  };
}

export const hirundoStore = createHirundoWorkspace();
