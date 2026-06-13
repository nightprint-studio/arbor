/**
 * nemus project store — the open nemus project + its `.nemus` file/source model.
 * `nemus_open_project` / `nemus_create_project` give the manifest + file list;
 * the source text is read lazily on the FE via `fs_read_text_file` and cached
 * (so re-selecting a tab is instant and edits are not re-read off disk).
 *
 * The editor fan-out (Step 2/3) owns the live buffer: it pushes edits via
 * `setSource(path, text)` and reads `sourceOf(path)` / `activeSource`. `save()`
 * flushes a buffer back to disk. Eval/run pull `activeSource` + `project.path`.
 */

import { SvelteMap } from 'svelte/reactivity';
import {
  nemusOpenProject, nemusCreateProject, nemusSetProjectName,
  type NemusProjectInfo, type NemusProjectFile,
} from '$lib/ipc/nemus';
import { fsReadTextFile, fsWriteTextFile } from '$lib/ipc/fs';
import { workspaceStore } from './workspace.svelte';

function createProjectStore() {
  let project        = $state<NemusProjectInfo | null>(null);
  // Source cache keyed by absolute path. SvelteMap so reads are reactive.
  const sources      = new SvelteMap<string, string>();
  let activeFilePath = $state<string | null>(null);
  let openFilePaths  = $state<string[]>([]);

  /** First non-library file (the playable entry), or the first file, or null. */
  function entryFile(p: NemusProjectInfo): NemusProjectFile | null {
    return p.files.find(f => !f.library) ?? p.files[0] ?? null;
  }

  /** Read a file's source into the cache if not already present. */
  async function ensureLoaded(path: string) {
    if (sources.has(path)) return;
    try { sources.set(path, await fsReadTextFile(path)); }
    catch { sources.set(path, ''); }
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
      const info = await nemusOpenProject(dir);
      project = info;
      const entry = entryFile(info);
      if (entry) {
        await ensureLoaded(entry.path);
        activeFilePath = entry.path;
        openFilePaths = [entry.path];
      } else {
        activeFilePath = null;
        openFilePaths = [];
      }
      workspaceStore.addRecent(dir);
    },

    /** Open a file as the active editor tab (loads its source if needed). */
    async openFile(path: string) {
      await ensureLoaded(path);
      activeFilePath = path;
      if (!openFilePaths.includes(path)) openFilePaths = [...openFilePaths, path];
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
      await nemusCreateProject(dir, name, audience);
      await this.open(dir);
    },

    /** Rename the open project (writes `nemus.toml`). Updates the manifest in
     *  place — open tabs / buffers / selection are untouched. */
    async rename(name: string) {
      if (!project) return;
      const trimmed = name.trim();
      if (!trimmed || trimmed === project.name) return;
      project = await nemusSetProjectName(project.path, trimmed);
    },
  };
}

export const projectStore = createProjectStore();
