/**
 * merula project store — the open merula project + its `.merula` file/source model.
 * `merula_open_project` / `merula_create_project` give the manifest + file list;
 * the source text is read lazily on the FE via `fs_read_text_file` and cached
 * (so re-selecting a tab is instant and edits are not re-read off disk).
 *
 * The editor fan-out (Step 2/3) owns the live buffer: it pushes edits via
 * `setSource(path, text)` and reads `sourceOf(path)` / `activeSource`. `save()`
 * flushes a buffer back to disk. Eval/run pull `activeSource` + `project.path`.
 */

import { SvelteMap } from 'svelte/reactivity';
import {
  merulaOpenProject, merulaCreateProject, merulaSetProjectName,
  getMerulaProjectTabs, setMerulaProjectTabs,
  type MerulaProjectInfo, type MerulaProjectFile,
} from '$lib/ipc/merula/merula';
import { fsReadTextFile, fsWriteTextFile, fsRename, fsTrash } from '$lib/ipc/fs';
import { workspaceStore } from './workspace.svelte';

const TABS_PERSIST_DELAY = 400;

function createProjectStore() {
  let project        = $state<MerulaProjectInfo | null>(null);
  // Source cache keyed by absolute path. SvelteMap so reads are reactive.
  const sources      = new SvelteMap<string, string>();
  let activeFilePath = $state<string | null>(null);
  let openFilePaths  = $state<string[]>([]);
  // Suppress tab-persist while we're restoring a snapshot (avoid echoing it back).
  let restoring = false;
  let persistTimer: ReturnType<typeof setTimeout> | null = null;

  /** First non-library file (the playable entry), or the first file, or null. */
  function entryFile(p: MerulaProjectInfo): MerulaProjectFile | null {
    return p.files.find(f => !f.library) ?? p.files[0] ?? null;
  }

  /** Read a file's source into the cache if not already present. */
  async function ensureLoaded(path: string) {
    if (sources.has(path)) return;
    try { sources.set(path, await fsReadTextFile(path)); }
    catch { sources.set(path, ''); }
  }

  /** Persist the open tabs to `<project>/.merula/tabs.json` (debounced). */
  function schedulePersistTabs() {
    if (persistTimer) clearTimeout(persistTimer);
    persistTimer = setTimeout(() => {
      persistTimer = null;
      if (!project || restoring) return;
      void setMerulaProjectTabs(project.path, {
        open_file_paths: [...openFilePaths],
        active_file_path: activeFilePath,
      }).catch(() => {});
    }, TABS_PERSIST_DELAY);
  }

  /** Restore a project's saved open tabs (filtered to files that still exist).
   *  Returns false when there's no usable snapshot (caller opens the entry file). */
  async function restoreTabs(info: MerulaProjectInfo): Promise<boolean> {
    try {
      const snap = await getMerulaProjectTabs(info.path);
      const known = new Set(info.files.map(f => f.path));
      const paths = snap.open_file_paths.filter(p => known.has(p));
      if (!paths.length) return false;
      for (const p of paths) await ensureLoaded(p);
      openFilePaths = paths;
      activeFilePath = snap.active_file_path && paths.includes(snap.active_file_path)
        ? snap.active_file_path
        : paths[0];
      return true;
    } catch {
      return false;
    }
  }

  return {
    get project()        { return project; },
    get files()          { return project?.files ?? []; },
    get activeFilePath() { return activeFilePath; },
    get openFilePaths()  { return openFilePaths; },
    /** Cached source of the active file (or '' when none / not yet loaded). */
    get activeSource()   { return activeFilePath ? (sources.get(activeFilePath) ?? '') : ''; },

    /** Cached source for `path` ('' when not loaded). */
    sourceOf(path: string): string { return sources.get(path) ?? ''; },

    /** Open a project folder: parse the manifest, load + open the entry file,
     *  and record it as a recent. */
    async open(dir: string) {
      const info = await merulaOpenProject(dir);
      project = info;
      // Restore the saved open tabs; if there's no snapshot, open the entry file.
      restoring = true;
      try {
        const restored = await restoreTabs(info);
        if (!restored) {
          const entry = entryFile(info);
          if (entry) {
            await ensureLoaded(entry.path);
            activeFilePath = entry.path;
            openFilePaths = [entry.path];
          } else {
            activeFilePath = null;
            openFilePaths = [];
          }
        }
      } finally {
        restoring = false;
      }
      workspaceStore.addRecent(dir);
    },

    /** Open a file as the active editor tab (loads its source if needed). */
    async openFile(path: string) {
      await ensureLoaded(path);
      activeFilePath = path;
      if (!openFilePaths.includes(path)) openFilePaths = [...openFilePaths, path];
      schedulePersistTabs();
    },

    /** Close a tab; pick a neighbour as active (mirrors the Step-0 logic). */
    closeFile(path: string) {
      const idx = openFilePaths.indexOf(path);
      if (idx === -1) return;
      openFilePaths = openFilePaths.filter(p => p !== path);
      if (activeFilePath === path) {
        activeFilePath = openFilePaths.length
          ? openFilePaths[Math.min(idx, openFilePaths.length - 1)]
          : null;
      }
      schedulePersistTabs();
    },

    /** Reorder the open tabs (drag-to-reorder in the tab strip). Indices are into
     *  `openFilePaths`; out-of-range requests are ignored. */
    reorderTab(fromIndex: number, toIndex: number) {
      if (fromIndex === toIndex) return;
      const arr = [...openFilePaths];
      if (fromIndex < 0 || fromIndex >= arr.length || toIndex < 0 || toIndex >= arr.length) return;
      const [moved] = arr.splice(fromIndex, 1);
      arr.splice(toIndex, 0, moved);
      openFilePaths = arr;
      schedulePersistTabs();
    },

    /** Update the cached source (editor edits route here). */
    setSource(path: string, text: string) { sources.set(path, text); },

    /** Flush a buffer to disk (defaults to the active file). */
    async save(path?: string) {
      const target = path ?? activeFilePath;
      if (!target) return;
      await fsWriteTextFile(target, sources.get(target) ?? '');
    },

    /** Scaffold a new project then open it. */
    async createProject(dir: string, name: string, audience: string) {
      await merulaCreateProject(dir, name, audience);
      await this.open(dir);
    },

    /** Rename the open project (writes `merula.toml`). Updates the manifest in
     *  place — open tabs / buffers / selection are untouched. */
    async rename(name: string) {
      if (!project) return;
      const trimmed = name.trim();
      if (!trimmed || trimmed === project.name) return;
      project = await merulaSetProjectName(project.path, trimmed);
    },

    /** Re-scan the project folder and refresh only the `.merula` file list — open
     *  tabs, cached buffers and the active selection are untouched. Used after a
     *  delete/rename and by the FS watcher when a file appears/disappears on disk
     *  (so the Files sidebar tracks the folder without a full re-open). */
    async refreshFiles() {
      if (!project) return;
      try {
        const info = await merulaOpenProject(project.path);
        // Keep the live manifest identity but adopt the fresh file list (+ any
        // name/audience edited on disk meanwhile).
        project = { ...project, name: info.name, audience: info.audience, files: info.files };
      } catch { /* folder vanished mid-flight — leave the stale list */ }
    },

    /** Whether `path` is one of the project's current `.merula` files. */
    hasFile(path: string): boolean {
      return !!project?.files.some(f => f.path === path);
    },

    /** Move a `.merula` file to the OS trash (recoverable), close its tab, drop its
     *  cached buffer and refresh the file list. No-op when nothing is open. */
    async deleteFile(path: string) {
      await fsTrash([path]);
      this.closeFile(path);
      sources.delete(path);
      await this.refreshFiles();
    },

    /** Rename a `.merula` file in place (same folder). `newName` is a bare file name
     *  (extension optional — `.merula` is appended when missing). Migrates the cached
     *  buffer + any open tab to the new path and refreshes the list. Returns the new
     *  absolute path, or null when the rename was a no-op / invalid. */
    async renameFile(path: string, newName: string): Promise<string | null> {
      const trimmed = newName.trim();
      if (!trimmed || /[\\/]/.test(trimmed)) return null;
      const finalName = trimmed.toLowerCase().endsWith('.merula') ? trimmed : `${trimmed}.merula`;
      const idx = Math.max(path.lastIndexOf('\\'), path.lastIndexOf('/'));
      const newPath = `${path.slice(0, idx)}${path[idx]}${finalName}`;
      if (newPath === path) return null;
      await fsRename(path, newPath);
      // Migrate the cached source + open tab + selection to the new path.
      if (sources.has(path)) { sources.set(newPath, sources.get(path) ?? ''); sources.delete(path); }
      openFilePaths = openFilePaths.map(p => (p === path ? newPath : p));
      if (activeFilePath === path) activeFilePath = newPath;
      schedulePersistTabs();
      await this.refreshFiles();
      return newPath;
    },
  };
}

export const projectStore = createProjectStore();
