/**
 * Bennu project store — the open Java project + its file tree + open-editor model.
 *
 * `openProject` resolves the manifest (modules / JDK / capabilities) and the file
 * tree; file source text is read lazily via `bennu_read_file` and cached (so
 * re-selecting a tab is instant and edits aren't re-read off disk). The editor
 * fan-out owns the live buffer: it pushes edits via `setSource(path, text)` and
 * reads `sourceOf(path)` / `activeSource`.
 *
 * Rune store — the single approved shape: private `$state`, a returned object of
 * getters + methods (see CLAUDE.md · "Store pattern").
 *
 * MOCK fallback — see `bennu-mock.ts`. `openProject` tries real IPC first and, if
 * `bennu-be` isn't attached (calls reject / BackendNotRunning), gracefully falls
 * back to the demo project so the shell is validatable without the backend. There
 * is also an explicit `loadDemo()` affordance. Remove both when the BE is live.
 */

import { SvelteMap } from 'svelte/reactivity';
import {
  openProject as ipcOpenProject,
  projectTree as ipcProjectTree,
  readFile as ipcReadFile,
} from '$lib/ipc/bennu';
import type { ProjectInfo, TreeNode } from '$lib/types/bennu';
// MOCK — remove when bennu-be serves real data.
import { DEMO_PROJECT, DEMO_TREE, DEMO_ROOT, isDemoPath, demoReadFile } from './bennu-mock';

function createProjectStore() {
  let project = $state<ProjectInfo | null>(null);
  let tree = $state<TreeNode | null>(null);
  // True while the open project is the mock demo (backend absent / explicit demo).
  // MOCK — remove when bennu-be serves real data.
  let isDemo = $state(false);
  // Recently-opened project roots (session-only; the switcher lists them).
  let recentProjects = $state<string[]>([]);
  // Source cache keyed by absolute path. SvelteMap so reads are reactive.
  const sources = new SvelteMap<string, string>();
  // Encoding the file was decoded from, keyed by path (Phase 0 keeps it for later
  // round-tripping; the editor just displays the text).
  const encodings = new SvelteMap<string, string>();
  let activeFilePath = $state<string | null>(null);
  let openFilePaths = $state<string[]>([]);

  function rememberRecent(root: string) {
    recentProjects = [root, ...recentProjects.filter((p) => p !== root)].slice(0, 10);
  }

  /** Read a file's source + encoding into the cache if not already present. The
   *  project root is passed so the backend resolves the pom-declared encoding.
   *  Demo paths are served from the mock (no IPC). */
  async function ensureLoaded(path: string) {
    if (sources.has(path)) return;
    // MOCK — demo files never touch the backend.
    if (isDemoPath(path)) {
      const res = demoReadFile(path);
      sources.set(path, res.text);
      encodings.set(path, res.encoding);
      return;
    }
    try {
      const res = await ipcReadFile(project?.root ?? path, path);
      sources.set(path, res.text);
      encodings.set(path, res.encoding);
    } catch {
      sources.set(path, '');
      encodings.set(path, 'utf-8');
    }
  }

  function applyProject(info: ProjectInfo, nextTree: TreeNode | null, demo: boolean) {
    project = info;
    tree = nextTree;
    isDemo = demo;
    rememberRecent(info.root);
    // Reset the open-file model for the new project.
    activeFilePath = null;
    openFilePaths = [];
    sources.clear();
    encodings.clear();
  }

  /** MOCK — load the built-in demo project (explicit affordance + BE-down fallback). */
  function loadDemoProject() {
    applyProject(DEMO_PROJECT, DEMO_TREE, true);
  }

  return {
    get project()        { return project; },
    get tree()           { return tree; },
    get capabilities()   { return project?.capabilities ?? null; },
    get activeFilePath() { return activeFilePath; },
    get openFilePaths()  { return openFilePaths; },
    /** True while the open project is the mock demo. MOCK — remove with the mock. */
    get isDemo()         { return isDemo; },
    /** Recently-opened project roots (most recent first). */
    get recentProjects() { return recentProjects; },
    /** Encoding of the active file (defaults to `UTF-8`). */
    get activeEncoding() { return activeFilePath ? (encodings.get(activeFilePath) ?? 'UTF-8') : null; },
    /** Cached source of the active file (or '' when none / not yet loaded). */
    get activeSource()   { return activeFilePath ? (sources.get(activeFilePath) ?? '') : ''; },

    /** Cached source for `path` ('' when not loaded). */
    sourceOf(path: string): string { return sources.get(path) ?? ''; },
    /** Decoded encoding for `path` (defaults to `UTF-8`). */
    encodingOf(path: string): string { return encodings.get(path) ?? 'UTF-8'; },

    /** Open a project folder: resolve the manifest + file tree (no file opened).
     *  Tries real IPC; on backend-absent failure, falls back to the demo project
     *  so the shell stays populated. MOCK — drop the catch when bennu-be is live. */
    async openProject(dir: string) {
      try {
        const info = await ipcOpenProject(dir);
        let nextTree: TreeNode | null = null;
        try { nextTree = await ipcProjectTree(info.root); }
        catch { nextTree = null; }
        applyProject(info, nextTree, false);
      } catch (err) {
        // MOCK — bennu-be not attached: fall back to the demo so opening the
        // window still shows a populated tree. Remove when the BE serves data.
        loadDemoProject();
        throw err; // let callers still observe the failure (they ignore it)
      }
    },

    /** MOCK — explicitly load the built-in demo project. Remove with the mock. */
    loadDemo() { loadDemoProject(); },

    /** Open a file as the active editor tab (loads its source + encoding if needed). */
    async openFile(path: string) {
      await ensureLoaded(path);
      activeFilePath = path;
      if (!openFilePaths.includes(path)) openFilePaths = [...openFilePaths, path];
    },

    /** Close a tab; pick a neighbour as active. */
    closeFile(path: string) {
      const idx = openFilePaths.indexOf(path);
      if (idx === -1) return;
      openFilePaths = openFilePaths.filter((p) => p !== path);
      if (activeFilePath === path) {
        activeFilePath = openFilePaths.length
          ? openFilePaths[Math.min(idx, openFilePaths.length - 1)]
          : null;
      }
    },

    /** Set the active tab (must already be open). */
    setActive(path: string) {
      if (openFilePaths.includes(path)) activeFilePath = path;
    },

    /** Update the cached source (editor edits route here). */
    setSource(path: string, text: string) { sources.set(path, text); },
  };
}

export const projectStore = createProjectStore();

// MOCK — the sentinel demo root, re-exported for consumers that badge the demo.
export { DEMO_ROOT };
