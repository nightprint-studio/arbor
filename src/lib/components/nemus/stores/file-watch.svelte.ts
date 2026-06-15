/**
 * nemus file-watch store — detects external changes to the open `.nemus` file on
 * disk (an IDE "file changed on disk, reload?" prompt).
 *
 * Reuses Arbor's per-window filesystem watcher (`fs_watch_start` → the
 * `arbor://fs-changed` event): we watch the directory of the active file and, on
 * any change, re-read it and compare to the live editor buffer. A difference is a
 * genuine external edit — our OWN saves leave disk == buffer, so they never
 * prompt, no flag needed. The shell mounts one `ConfirmModal` off `pending`.
 *
 * Window-local UI state, rune-store pattern (factory + getters).
 */

import { fsReadTextFile, fsWatchStart, fsWatchStop } from '$lib/ipc/fs';
import { projectStore } from './project.svelte';
import { nemusEngine } from './engine.svelte';

/** A detected on-disk change awaiting the user's reload decision. */
export interface PendingReload {
  /** Absolute path of the changed file. */
  path: string;
  /** Display name (basename). */
  name: string;
  /** The new on-disk content (applied verbatim on reload). */
  content: string;
}

/** Directory of an absolute file path (native separators), or null. */
function dirOf(path: string): string | null {
  const m = path.match(/^(.*)[\\/][^\\/]+$/);
  return m ? m[1] : null;
}

function createFileWatchStore() {
  let pending = $state<PendingReload | null>(null);
  let watchedDir: string | null = null;

  return {
    get pending() { return pending; },

    /** (Re)watch the project folder (recursively) so BOTH external edits to the
     *  active file AND files added/removed/renamed anywhere in the project are
     *  detected. Falls back to the active file's directory when no project is open
     *  (a lone Open File…). Idempotent per directory, so a tab switch within the
     *  same project never churns the watcher. */
    async watchActive() {
      const projectRoot = projectStore.project?.path ?? null;
      const dir = projectRoot ?? dirOf(projectStore.activeFilePath ?? '');
      if (!dir || dir === watchedDir) return;
      watchedDir = dir;
      // Recursive when watching a whole project tree (so files added in sub-folders
      // surface); a lone Open File… watches just its directory.
      try { await fsWatchStart(dir, projectRoot != null); } catch { /* best-effort */ }
    },

    /** A change fired in the watched tree. Two concerns:
     *  1. The Files sidebar tracks the folder — re-scan so an added/removed/renamed
     *     `.nemus` shows without a manual reload.
     *  2. The active file may have been edited externally — re-read it and, if it
     *     differs from the editor buffer (not our own save), stage a reload prompt.
     *  A read failure (e.g. the file was deleted) is ignored — nothing to reload. */
    async onChanged() {
      void projectStore.refreshFiles();
      const path = projectStore.activeFilePath;
      if (!path) return;
      let disk: string;
      try { disk = await fsReadTextFile(path); } catch { return; }
      if (disk === projectStore.sourceOf(path)) return; // unchanged vs buffer (incl. our saves)
      pending = { path, name: path.split(/[\\/]/).pop() ?? path, content: disk };
    },

    /** Apply the pending reload: replace the buffer with the on-disk content and
     *  re-evaluate so diagnostics / arrangement track the reloaded source. */
    reload() {
      const p = pending;
      pending = null;
      if (!p) return;
      projectStore.setSource(p.path, p.content);
      if (projectStore.activeFilePath === p.path) {
        void nemusEngine.eval(p.content, projectStore.project?.path);
      }
    },

    /** Keep the in-editor version (dismiss the prompt). The next external change
     *  re-prompts, since the buffer still differs from disk. */
    dismiss() { pending = null; },

    /** Stop watching (window teardown). */
    stop() {
      watchedDir = null;
      void fsWatchStop().catch(() => {});
    },
  };
}

export const fileWatchStore = createFileWatchStore();
