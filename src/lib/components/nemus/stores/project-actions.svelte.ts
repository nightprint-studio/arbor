/**
 * nemus project-action launcher — the single owner of the file/project picker
 * flows (New / Open project / Open file / Export WAV) and the Save flush.
 *
 * Centralised so every entry point — the hamburger menu, the titlebar, and the
 * keyboard shortcuts in NemusShell — drives the SAME picker + confirm logic
 * instead of each hand-rolling its own `FileExplorerModal` (which had already
 * drifted: the menubar export lacked `initialPath`). The store holds which
 * picker is open; `NemusProjectActions.svelte` renders it once for the window.
 */

import { projectStore } from './project.svelte';
import { renderStore, DEFAULT_RENDER_LOOPS } from './render.svelte';
import { arrangementStore } from '../viz/arrangement.svelte';
import { nemusRender } from '$lib/ipc/nemus';
import { fsWriteTextFile } from '$lib/ipc/fs';

export type NemusPicker = 'new' | 'new-file' | 'open-project' | 'open-file' | 'export' | null;

/** Last path segment (forward- or back-slash). */
function basename(path: string): string {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

/** Starter content for a freshly-created `.nemus` file. */
const STARTER_NEMUS = `cps(0.5)

tracks(
  track("main", n(c4 e4 g4).inst("synth.pluck")),
)
`;

function createProjectActions() {
  // One picker at a time; the mode decides what a confirmed path means.
  let picker = $state<NemusPicker>(null);

  // Export is a two-step flow: the options dialog (loops + live estimate) runs
  // first, THEN the save picker — so the user sees the duration/size before
  // committing to a path. `exportOptionsOpen` gates the dialog; `exportLoops`
  // is the chosen multiplier, carried into the picker's confirm.
  let exportOptionsOpen = $state(false);
  let exportLoops       = $state(DEFAULT_RENDER_LOOPS);

  function onConfirm(path: string) {
    const mode = picker;
    picker = null;
    if (mode === 'new') {
      // Minimal scaffold: folder name as project name, blank audience (a proper
      // name + "for whom" form is a follow-up). Writes nemus.toml + a starter.
      void projectStore.createProject(path, basename(path), '').catch(() => {});
    } else if (mode === 'new-file') {
      // Write a starter into the chosen path, then open it as a tab.
      void (async () => {
        try {
          await fsWriteTextFile(path, STARTER_NEMUS);
          await projectStore.openFile(path);
        } catch { /* write failed — surfaced by the picker toast host */ }
      })();
    } else if (mode === 'open-project') {
      void projectStore.open(path).catch(() => {});
    } else if (mode === 'open-file') {
      void projectStore.openFile(path).catch(() => {});
    } else if (mode === 'export') {
      // Render length = the arrangement's natural loop period × the user's loop
      // count from the options dialog. Falls back to one cycle when the
      // arrangement hasn't been evaluated (loopCycles == 0) so a stray export
      // still produces a (tiny but valid) WAV instead of an empty one.
      const cycles = (arrangementStore.loopCycles || 1) * exportLoops;
      // The render runs as a background job; the store reports start/done/fail
      // via the title-bar badge (the job resolves with an id, not the WAV).
      void renderStore.track(
        nemusRender(projectStore.activeSource, path, { cycles }, projectStore.project?.path),
        path,
      );
    }
  }

  /** Cycles that the current options would render — `loopCycles × loops`. 0
   *  when the arrangement hasn't been evaluated yet (the dialog disables Export
   *  and prompts the user to evaluate first). */
  function exportCycles(): number {
    return arrangementStore.loopCycles * exportLoops;
  }

  return {
    get picker() { return picker; },
    get exportOptionsOpen() { return exportOptionsOpen; },
    get exportLoops()       { return exportLoops; },
    /** Resulting render length for the chosen loop count (read-only echo). */
    get exportCycles()      { return exportCycles(); },

    setExportLoops(n: number) { exportLoops = Math.max(1, Math.round(n) || 1); },

    /** Open the "new nemus project" folder picker. */
    newProject()  { picker = 'new'; },
    /** Open the "new .nemus file" save picker (in the open project). With no
     *  project open there's nowhere to put it, so fall back to New Project. */
    newFile()     { picker = projectStore.project ? 'new-file' : 'new'; },
    /** Open the "open nemus project" folder picker. */
    openProject() { picker = 'open-project'; },
    /** Open the "open .nemus file" picker. */
    openFile()    { picker = 'open-file'; },
    /** Open the export options dialog (step 1 of the two-step export flow);
     *  resets the loop count to the default each time so the dialog is
     *  predictable. */
    exportWav()   { exportLoops = DEFAULT_RENDER_LOOPS; exportOptionsOpen = true; },
    /** Confirm export options → advance to the save picker (step 2). */
    confirmExportOptions() { exportOptionsOpen = false; picker = 'export'; },
    /** Dismiss the export options dialog without exporting. */
    cancelExportOptions()  { exportOptionsOpen = false; },
    /** Flush the active buffer to disk (no-op when there's no active file). */
    save()        { void projectStore.save().catch(() => {}); },

    cancel()      { picker = null; },
    onConfirm,
  };
}

export const projectActions = createProjectActions();
