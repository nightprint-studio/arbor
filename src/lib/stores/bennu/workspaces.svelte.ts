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

  const active = $derived(workspaces.find((w) => w.id === activeId) ?? null);

  // Debounced persistence of the whole store (active id + every workspace). A burst of tab churn
  // through `saveActiveSession` coalesces into one BE write.
  let persistTimer: ReturnType<typeof setTimeout> | undefined;
  function persist() {
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      void setBennuWorkspaces({
        active_id: activeId,
        // Snapshot the reactive proxies to plain data before crossing the IPC boundary.
        workspaces: $state.snapshot(workspaces) as BennuWorkspace[],
      }).catch(() => { /* BE absent — the session just won't persist */ });
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
    /** True when there is more than one workspace (drives showing the switcher list / manager). */
    get hasMany() { return workspaces.length > 1; },
    /** Friendly display name for the active workspace (implicit default → "Workspace"). */
    get activeName() { return active ? (active.name || 'Workspace') : ''; },

    /** Reported by projectStore whenever the active workspace's live session changes (tab / project
     *  churn). Mirrors the snapshot into the active workspace + persists. Auto-creates an implicit
     *  default workspace the first time a project is opened with no workspace yet. */
    saveActiveSession(snap: { active_project: string; projects: ProjectSession[] }) {
      if (!activeId) {
        if (!snap.projects.length) return; // nothing to persist, and no workspace to create
        const id = newId();
        workspaces = [
          ...workspaces,
          { id, name: '', color_idx: workspaces.length % WS_COLOR_COUNT, active_project: snap.active_project, projects: snap.projects },
        ];
        activeId = id;
        persist();
        return;
      }
      patchWorkspace(activeId, { active_project: snap.active_project, projects: snap.projects });
      persist();
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
      if (wasActive) {
        const next = workspaces[0];
        if (next) {
          activeId = next.id;
          await projectStore.loadWorkspace($state.snapshot(next.projects) as ProjectSession[], next.active_project);
        } else {
          activeId = '';
          projectStore.clearAll();
        }
      }
      persist();
    },

    /** Reopen the persisted store on window boot — reads every workspace + the active id, then
     *  drives projectStore to load the active workspace's projects. Call once from onMount. */
    async restore() {
      let store;
      try {
        store = await getBennuWorkspaces();
      } catch {
        loaded = true;
        return; // BE absent — nothing to restore
      }
      workspaces = store.workspaces ?? [];
      loaded = true;
      const act = workspaces.find((w) => w.id === store.active_id) ?? workspaces[0];
      if (!act) return; // empty store — window opens with nothing
      activeId = act.id;
      await projectStore.loadWorkspace($state.snapshot(act.projects) as ProjectSession[], act.active_project);
    },
  };
}

export const workspacesStore = createWorkspacesStore();
