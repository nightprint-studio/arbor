/**
 * Bennu project-health store — projects and modules that are no longer where the workspace says.
 *
 * Two sources feed it: the workspace restore (a remembered root that no longer opens because its
 * directory is gone) and the backend (`arbor://bennu/project-missing` when an open project's root
 * vanishes mid-session, `arbor://bennu/modules-missing` when a reactor lists a module the disk lost).
 *
 * A missing project is announced once per episode — a toast with **Locate…** and **Remove from
 * workspace** — and stays listed (the command palette offers the same two verbs) until one of them
 * settles it or the root opens again.
 *
 * Rune-store pattern: private `$state`, returned getters + methods (CLAUDE.md).
 */

import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { toastStore } from '$lib/feedback/stores/toasts.svelte';
import {
  MODULES_MISSING,
  PROJECT_MISSING,
  rootStatus,
  type ModulesMissing,
  type ProjectMissing,
} from '$lib/ipc/bennu/project-health';
import { projectStore } from './project.svelte';
import { workspacesStore } from './workspaces.svelte';

/** A workspace project whose root directory is gone. */
export interface MissingProject {
  root: string;
  name: string;
  /** Where the folder picker starts: the nearest ancestor that still exists. */
  nearestExisting: string | null;
}

/** How long a missing-module toast stays — long enough to read a path, not sticky: the pom's own
 *  error diagnostic keeps saying it. */
const MODULES_TOAST_MS = 12000;

function canon(path: string): string {
  return path.replace(/\\/g, '/');
}

function baseName(path: string): string {
  return path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || path;
}

function quoteList(names: string[]): string {
  return names.map((n) => `'${n}'`).join(', ');
}

function createProjectHealthStore() {
  let missing = $state<MissingProject[]>([]);
  let locating = $state<MissingProject | null>(null);
  let attached = false;

  function report(root: string, nearestExisting: string | null) {
    const key = canon(root);
    if (missing.some((m) => m.root === key)) return; // the same episode
    const entry: MissingProject = { root: key, name: baseName(key), nearestExisting };
    missing = [...missing, entry];
    toastStore.show(`Project ${entry.name} no longer exists at ${key}`, 'warning', 0, [
      { label: 'Locate…', onClick: () => startLocate(key) },
      { label: 'Remove from workspace', onClick: () => remove(key) },
    ]);
  }

  function settle(root: string) {
    const key = canon(root);
    if (missing.some((m) => m.root === key)) missing = missing.filter((m) => m.root !== key);
  }

  function startLocate(root: string) {
    locating = missing.find((m) => m.root === canon(root)) ?? null;
  }

  /** Drop `root` from the active workspace, whether it is a live member (vanished mid-session) or a
   *  remembered entry that never opened. */
  function remove(root: string) {
    settle(root);
    workspacesStore.removeProjectFrom(workspacesStore.activeId, canon(root));
  }

  function announceModules(payload: ModulesMissing) {
    const byPom = new Map<string, string[]>();
    for (const m of payload.modules) byPom.set(m.pom, [...(byPom.get(m.pom) ?? []), m.name]);
    const unlisted = payload.unlisted.length
      ? ` — ${quoteList(payload.unlisted.map(baseName))} has a pom.xml that no <modules> lists`
      : '';
    for (const [pom, names] of byPom) {
      const message = names.length === 1
        ? `Module ${quoteList(names)} declared in ${pom} was not found${unlisted}`
        : `Modules ${quoteList(names)} declared in ${pom} were not found${unlisted}`;
      toastStore.show(message, 'warning', MODULES_TOAST_MS, {
        label: 'Open pom.xml',
        onClick: () => void projectStore.openFile(pom),
      });
    }
  }

  return {
    /** Workspace projects whose root is gone and that nobody has located or removed yet. */
    get missing() { return missing; },
    /** The missing project the folder picker is open for, or `null`. */
    get locating() { return locating; },

    /** Subscribe to the backend's two health events (once, from BennuWindow.onMount). */
    async attach(): Promise<UnlistenFn> {
      if (attached) return () => {};
      attached = true;
      const unProject = await listen<ProjectMissing>(PROJECT_MISSING, (e) => {
        report(e.payload.root, e.payload.nearest_existing);
      }).catch(() => null);
      const unModules = await listen<ModulesMissing>(MODULES_MISSING, (e) => {
        if (e.payload.modules.length) announceModules(e.payload);
      }).catch(() => null);
      return () => { unProject?.(); unModules?.(); attached = false; };
    },

    /**
     * Explain why a remembered project did not open. A vanished directory is announced; anything
     * else (no manifest, the backend down) is logged rather than swallowed — the restore carries on
     * with the projects that did open either way.
     */
    async explainFailedOpen(root: string, error: unknown): Promise<void> {
      try {
        const status = await rootStatus(root);
        if (!status.exists) {
          report(root, status.nearest_existing);
          return;
        }
      } catch { /* the status question failed too — fall through to the log */ }
      console.warn(`bennu: project ${root} could not be opened:`, error);
    },

    /** A root that opened again ends its episode. */
    settle,
    startLocate,
    cancelLocate() { locating = null; },

    /** The folder picker confirmed `dir` for the project being located: add it, then drop the old
     *  entry. A folder that is not a project leaves the old entry in place. */
    async confirmLocate(dir: string) {
      const target = locating;
      locating = null;
      if (!target) return;
      if (!(await projectStore.addProject(dir))) {
        toastStore.show(`${dir} is not a Maven or Cargo project`, 'error');
        return;
      }
      remove(target.root);
    },

    remove,
  };
}

export const projectHealthStore = createProjectHealthStore();
