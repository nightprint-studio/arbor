/**
 * grove project-action launcher — the single owner of the file/project picker
 * flows (New / Open project / Open file / Export WAV) and the Save flush.
 *
 * Centralised so every entry point — the hamburger menu, the titlebar, and the
 * keyboard shortcuts in GroveShell — drives the SAME picker + confirm logic
 * instead of each hand-rolling its own `FileExplorerModal` (which had already
 * drifted: the menubar export lacked `initialPath`). The store holds which
 * picker is open; `GroveProjectActions.svelte` renders it once for the window.
 */

import { projectStore } from './project.svelte';
import { groveRender } from '$lib/ipc/grove';

export type GrovePicker = 'new' | 'open-project' | 'open-file' | 'export' | null;

/** Last path segment (forward- or back-slash). */
function basename(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

function createProjectActions() {
  // One picker at a time; the mode decides what a confirmed path means.
  let picker = $state<GrovePicker>(null);

  function onConfirm(path: string) {
    const mode = picker;
    picker = null;
    if (mode === 'new') {
      // Minimal scaffold: folder name as project name, blank audience (a proper
      // name + "for whom" form is a follow-up). Writes grove.toml + a starter.
      void projectStore.createProject(path, basename(path), '').catch(() => {});
    } else if (mode === 'open-project') {
      void projectStore.open(path).catch(() => {});
    } else if (mode === 'open-file') {
      void projectStore.openFile(path).catch(() => {});
    } else if (mode === 'export') {
      void groveRender(projectStore.activeSource, path, { cycles: 32 }, projectStore.project?.path);
    }
  }

  return {
    get picker() { return picker; },

    /** Open the "new grove project" folder picker. */
    newProject()  { picker = 'new'; },
    /** Open the "open grove project" folder picker. */
    openProject() { picker = 'open-project'; },
    /** Open the "open .grove file" picker. */
    openFile()    { picker = 'open-file'; },
    /** Open the "export/render to WAV" save picker. */
    exportWav()   { picker = 'export'; },
    /** Flush the active buffer to disk (no-op when there's no active file). */
    save()        { void projectStore.save().catch(() => {}); },

    cancel()      { picker = null; },
    onConfirm,
  };
}

export const projectActions = createProjectActions();
