/**
 * Bennu workspaces store — the SET of named workspaces (Corvus-light).
 *
 * A workspace is a named, colored group of Java projects; switching workspace reopens a whole
 * different set of projects where the user left off. The same project may belong to several
 * workspaces (each keeps its own tabs). This store owns the list + which is active + the
 * `workspace.toml` persistence; the sibling {@link projectStore} is the LIVE runtime of the
 * ACTIVE workspace only (its opened projects, trees, editor buffers, tabs).
 *
 * Division of labour:
 *  - projectStore mutates the active workspace's membership + tabs and reports a session snapshot
 *    up via {@link WorkspacesStore.saveActiveSession} on every change.
 *  - this store handles cross-workspace concerns: create / rename / recolor / delete / **switch**,
 *    and drives projectStore to load a different project set on switch.
 *
 * Rune store — private `$state`, returned object of getters + methods (CLAUDE.md · store pattern).
 * Circular import with projectStore is safe: neither store touches the other at construction, only
 * inside methods called later.
 */

import { getBennuWorkspaces, setBennuWorkspaces } from '$lib/ipc/bennu/config';
import type { BennuWorkspace, ProjectSession } from '$lib/ipc/bennu/config';
import { projectStore } from './project.svelte';

/** Palette size — mirrors Corvus's `--ws-color-0 … --ws-color-11` theme vars. */
export const WS_COLOR_COUNT = 12;

/** CSS var reference for a palette index (clamped) — feeds `<Monogram color=…>`. Reuses the SAME
 *  global `--ws-color-N` vars as Corvus, so the two products stay visually consistent. */
export function wsColorVar(idx: number): string {
  const i = Number.isFinite(idx) ? Math.max(0, Math.min(WS_COLOR_COUNT - 1, Math.floor(idx))) : 0;
  return `var(--ws-color-${i})`;
}

/** The name of the workspace that always exists. Mirrors Corvus's scratch workspace: you never
 *  have to create one before opening something, and there is always somewhere for a project to
 *  land. Not deletable — deleting it empties it instead. */
export const DEFAULT_WORKSPACE_NAME = 'Scratch';

/** How long to wait before retrying a restore that could not reach the backend, and how many
 *  times. `bennu-be` is spawned lazily when the window opens, and on a release build that spawn is
 *  slower than a debug one — code signature validation on macOS alone can take a second on first
 *  launch. Six tries over ~5s covers it without spinning. */
const RESTORE_RETRY_MS = 700;
const RESTORE_TRIES = 7;

/** A fresh stable workspace id. `crypto.randomUUID` is available in WebView2; the fallback keeps
 *  it robust in any odd runtime. */
function newId(): string {
  try {
    return crypto.randomUUID();
  } catch {
    return `ws-${Date.now().toString(36)}-${Math.floor(Math.random() * 1e9).toString(36)}`;
  }
}

function createWorkspacesStore() {
  let workspaces = $state<BennuWorkspace[]>([]);
  let activeId = $state<string>('');
  let loaded = $state(false);

  /**
   * Whether the persisted file has actually been **read**.
   *
   * This is the load-bearing piece of state in this store, and the bug it exists for was data
   * loss. A failed restore used to be indistinguishable from an empty store: `bennu-be` is spawned
   * lazily when the window opens, so on a release build — where that spawn is slower — the restore
   * lost the race, left the list empty, and then the very first write **overwrote a real
   * `workspace.toml` with nothing**. Workspaces appeared to be created, projects would not stick,
   * and closing the window lost the lot.
   *
   * So nothing is ever written until a read has succeeded. An unreachable backend now costs a
   * session that does not persist — which is recoverable — instead of one that destroys the
   * previous session on the way out.
   */
  let restored = $state(false);
  /** Set when every retry failed, so the UI can say so rather than looking merely empty. */
  let restoreFailed = $state(false);

  const active = $derived(workspaces.find((w) => w.id === activeId) ?? null);

  /** The write itself, awaited by [`flush`] so a close can wait for it. */
  function write(): Promise<void> {
    return setBennuWorkspaces({
      active_id: activeId,
      // Snapshot the reactive proxies to plain data before crossing the IPC boundary.
      workspaces: $state.snapshot(workspaces) as BennuWorkspace[],
    }).catch(() => { /* the backend went away mid-session; the next write will try again */ });
  }

  // Tab churn arrives in bursts, so `saveActiveSession` coalesces. Everything else — creating,
  // renaming, deleting, switching — writes at once: those are rare, they are what the user just
  // asked for, and a 300ms window in which they can be lost is exactly the window a window-close
  // falls into.
  let persistTimer: ReturnType<typeof setTimeout> | undefined;
  function persist() {
    if (!restored) return; // never overwrite a file we have not read — see `restored`
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = undefined;
    void write();
  }

  function persistSoon() {
    if (!restored) return;
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      persistTimer = undefined;
      void write();
    }, 300);
  }

  /** Replace the workspace with `id` via a mapper (reassigns the array for reactivity). */
  function patchWorkspace(id: string, patch: Partial<BennuWorkspace>) {
    const i = workspaces.findIndex((w) => w.id === id);
    if (i < 0) return;
    workspaces[i] = { ...workspaces[i], ...patch };
  }

  return {
    /** Every workspace, in display order. */
    get workspaces() { return workspaces; },
    /** The active workspace (or null before the first project is opened). */
    get active() { return active; },
    /** Id of the active workspace ('' when none). */
    get activeId() { return activeId; },
    /** True once {@link restore} has run (drives first-paint gating if needed). */
    get loaded() { return loaded; },
    /** True when the persisted store was actually read — and therefore when this session is
     *  allowed to write. */
    get restored() { return restored; },
    /** True when every restore attempt failed. The session works but will not persist, and saying
     *  so is the difference between a bug and a warning. */
    get restoreFailed() { return restoreFailed; },
    /** True when there is more than one workspace (drives showing the switcher list / manager). */
    get hasMany() { return workspaces.length > 1; },
    /** Friendly display name for the active workspace (implicit default → "Workspace"). */
    get activeName() { return active ? (active.name || 'Workspace') : ''; },

    /** Reported by projectStore whenever the active workspace's live session changes (tab / project
     *  churn). Mirrors the snapshot into the active workspace + persists. Auto-creates an implicit
     *  default workspace the first time a project is opened with no workspace yet. */
    saveActiveSession(snap: { active_project: string; projects: ProjectSession[] }) {
      // There is always a workspace to save into: the default one is created the moment a restore
      // succeeds. The old implicit "create one on the first project" path is gone, and with it the
      // window where a project was opened into nothing and then written over the real file.
      if (!activeId) return;
      const patch: Partial<BennuWorkspace> = {
        active_project: snap.active_project,
        projects: snap.projects,
      };
      // An unnamed workspace names itself after the first project it gets. A workspace exists to
      // hold projects, so its first one is the best name anybody has for it — and it saves the trip
      // through the manager that made creating one feel like paperwork.
      const current = workspaces.find((w) => w.id === activeId);
      if (current && !current.name && current.projects.length === 0 && snap.projects.length > 0) {
        const first = snap.projects[0].root.replace(/[\\/]+$/, '').split(/[\\/]/).pop();
        if (first) patch.name = first;
      }
      patchWorkspace(activeId, patch);
      persistSoon();
    },

    /** Switch the active workspace to `id` — flushes the current live session into its workspace,
     *  then drives projectStore to reopen the target's project set. No-op if already active. */
    async switchTo(id: string) {
      if (id === activeId || !workspaces.some((w) => w.id === id)) return;
      if (activeId) patchWorkspace(activeId, projectStore.snapshotSession()); // flush current
      activeId = id;
      const target = workspaces.find((w) => w.id === id)!;
      await projectStore.loadWorkspace($state.snapshot(target.projects) as ProjectSession[], target.active_project);
      persist();
    },

    /** Create a new empty workspace and switch to it. The active view is cleared; the user then
     *  opens / adds projects into it. Returns the new id. */
    async create(name: string, colorIdx?: number): Promise<string> {
      if (activeId) patchWorkspace(activeId, projectStore.snapshotSession()); // flush current
      const id = newId();
      workspaces = [
        ...workspaces,
        {
          id,
          name: name.trim() || 'Workspace',
          color_idx: colorIdx ?? workspaces.length % WS_COLOR_COUNT,
          active_project: '',
          projects: [],
        },
      ];
      activeId = id;
      projectStore.clearAll();
      persist();
      return id;
    },

    /** Switch to workspace `id` (if needed) and make project `root` its active project — the
     *  tree switcher's "click a project under a workspace" action. */
    async switchToProject(id: string, root: string) {
      if (id !== activeId) await this.switchTo(id);
      await projectStore.switchProject(root);
    },

    /** Add a project (folder `dir`) to workspace `id`, switching to it first so the add always
     *  goes through projectStore's live path (opens the manifest, indexes, badges foreign tabs). */
    async addProjectTo(id: string, dir: string) {
      if (id !== activeId) await this.switchTo(id);
      await projectStore.addProject(dir);
    },

    /** Remove project `root` from workspace `id`. For the active workspace this closes it live
     *  (projectStore persists); for an inactive one it edits the stored member list directly. */
    removeProjectFrom(id: string, root: string) {
      if (id === activeId) {
        projectStore.closeProject(root); // live path → saveActiveSession persists
        return;
      }
      const i = workspaces.findIndex((w) => w.id === id);
      if (i < 0) return;
      const w = workspaces[i];
      const projects = w.projects.filter((p) => p.root !== root);
      const active_project = w.active_project === root ? (projects[0]?.root ?? '') : w.active_project;
      workspaces[i] = { ...w, projects, active_project };
      persist();
    },

    /** Rename a workspace. */
    rename(id: string, name: string) {
      patchWorkspace(id, { name: name.trim() || 'Workspace' });
      persist();
    },

    /** Recolor a workspace (palette index). */
    setColor(id: string, colorIdx: number) {
      patchWorkspace(id, { color_idx: Math.max(0, Math.min(WS_COLOR_COUNT - 1, colorIdx)) });
      persist();
    },

    /** Delete a workspace. If it was active, switch to another (or clear when it was the last). */
    async remove(id: string) {
      if (!workspaces.some((w) => w.id === id)) return;
      const wasActive = id === activeId;
      workspaces = workspaces.filter((w) => w.id !== id);
      // Deleting the last one leaves the default rather than nothing: there is always somewhere for
      // the next project to land, which is the whole point of having a default at all.
      if (workspaces.length === 0) {
        workspaces = [{
          id: newId(),
          name: DEFAULT_WORKSPACE_NAME,
          color_idx: 0,
          active_project: '',
          projects: [],
        }];
        activeId = workspaces[0].id;
        projectStore.clearAll();
        persist();
        return;
      }
      if (wasActive) {
        const next = workspaces[0];
        activeId = next.id;
        await projectStore.loadWorkspace($state.snapshot(next.projects) as ProjectSession[], next.active_project);
      }
      persist();
    },

    /** Reopen the persisted store on window boot — reads every workspace + the active id, then
     *  drives projectStore to load the active workspace's projects. Call once from onMount. */
    async restore() {
      // Idempotent. The window asks on mount and again when `bennu-be` announces itself
      // (`arbor://bennu-be-up`), because the spawn is lazy and the mount can lose that race. Once a
      // read has succeeded there is nothing to redo — re-reading would reopen the projects under
      // the user, discarding whatever they have changed since.
      if (restored) return;
      // Retried, because "the backend is not up yet" and "there is nothing saved" are different
      // answers and only one of them means the file is empty. `bennu-be` is spawned lazily as the
      // window opens; on a release build that spawn is slower than a debug one, which is how this
      // used to lose the race — and losing it used to destroy the file.
      let store;
      for (let attempt = 0; attempt < RESTORE_TRIES; attempt += 1) {
        try {
          store = await getBennuWorkspaces();
          break;
        } catch {
          if (attempt === RESTORE_TRIES - 1) break;
          await new Promise((r) => setTimeout(r, RESTORE_RETRY_MS));
        }
      }
      loaded = true;
      if (!store) {
        // Still unreachable. Stay read-only: this session will not persist, which is recoverable,
        // whereas writing an empty store over a real one is not.
        restoreFailed = true;
        return;
      }
      restored = true;
      workspaces = store.workspaces ?? [];

      // The default workspace, created here and only here — after a successful read, so it can
      // never be the thing that replaces a file we failed to load.
      if (workspaces.length === 0) {
        workspaces = [{
          id: newId(),
          name: DEFAULT_WORKSPACE_NAME,
          color_idx: 0,
          active_project: '',
          projects: [],
        }];
        activeId = workspaces[0].id;
        persist();
        return;
      }

      const act = workspaces.find((w) => w.id === store.active_id) ?? workspaces[0];
      activeId = act.id;
      await projectStore.loadWorkspace($state.snapshot(act.projects) as ProjectSession[], act.active_project);
    },

    /**
     * Write now and wait for it — what the window's close handler awaits.
     *
     * Tauri awaits an `onCloseRequested` handler before it closes, so this is the difference
     * between "the last 300ms of your session persisted" and "it did not". Safe to call at any
     * time; a no-op before a successful restore, like every other write.
     */
    async flush(): Promise<void> {
      if (persistTimer) {
        clearTimeout(persistTimer);
        persistTimer = undefined;
      }
      if (!restored) return;
      if (activeId) patchWorkspace(activeId, projectStore.snapshotSession());
      await write();
    },
  };
}

export const workspacesStore = createWorkspacesStore();
