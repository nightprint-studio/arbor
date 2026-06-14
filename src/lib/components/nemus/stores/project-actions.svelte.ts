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
import { configStore } from './config.svelte';
import { transportUiStore } from './transport-ui.svelte';
import { arrangementStore } from '../viz/arrangement.svelte';
import { nemusRender, nemusRenderStems, nemusExportMidi } from '$lib/ipc/nemus';
import { fsWriteTextFile } from '$lib/ipc/fs';
import { transfersStore } from '$lib/feedback/stores/transfers.svelte';

export type NemusPicker =
  | 'new' | 'new-file' | 'open-project' | 'open-file'
  | 'export' | 'export-region' | 'export-stems' | 'export-midi' | null;

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
  // The chosen output format is a *persistent* preference (saved to the nemus
  // render config, never localStorage — Arbor hard rule #11), so it survives
  // across sessions and is shared by the split button + the export dialog.
  // Read/written straight through `configStore.render.format`.
  function currentFormat(): 'wav' | 'ogg' {
    return configStore.render.format === 'ogg' ? 'ogg' : 'wav';
  }
  // Per-export render-format overrides. Seeded from the global Settings → Render
  // defaults each time an export starts (so a one-off tweak in the dialog never
  // silently rewrites the global config), then sent as `nemus_render` opts.
  let exportSampleRate  = $state(48_000);
  let exportBitDepth    = $state('int24');
  let exportTail        = $state(4.0);
  // LUFS normalization (per-export, session-sticky, off by default). When on, the
  // bounce is normalized to `exportNormalizeTarget` LUFS (peak-limited in the
  // engine). Stems deliberately skip this — normalizing each stem alone would
  // wreck their relative balance.
  let exportNormalizeOn     = $state(false);
  let exportNormalizeTarget = $state(-14);
  // Region export window (first cycle + length), snapshotted from the loop region
  // when the flow launches so the save-picker confirm bounces exactly that span.
  let regionWindow      = $state<{ start: number; cycles: number } | null>(null);

  /** Reset the render-format overrides to the current global defaults. Called at
   *  each export entry point (quick export + the options dialog) so both start
   *  from Settings → Render. */
  function loadRenderDefaults() {
    const r = configStore.render;
    exportSampleRate = r.sample_rate;
    exportBitDepth   = r.bit_depth;
    exportTail       = r.tail_max_secs;
  }

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
      // via the title-bar badge (the job resolves with an id, not the audio file).
      // Format overrides come from the options dialog (seeded from Settings).
      void renderStore.track(
        nemusRender(
          projectStore.activeSource,
          path,
          { cycles, format: currentFormat(), sample_rate: exportSampleRate, bit_depth: exportBitDepth, tail_max_secs: exportTail, normalize_lufs: exportNormalizeOn ? exportNormalizeTarget : undefined },
          projectStore.project?.path,
        ),
        path,
      );
    } else if (mode === 'export-region') {
      // Bounce only the loop region [start, start+cycles) into a single file.
      const w = regionWindow;
      regionWindow = null;
      if (w) {
        void renderStore.track(
          nemusRender(
            projectStore.activeSource,
            path,
            { cycles: w.cycles, start_cycle: w.start, format: currentFormat(), sample_rate: exportSampleRate, bit_depth: exportBitDepth, tail_max_secs: exportTail, normalize_lufs: exportNormalizeOn ? exportNormalizeTarget : undefined },
            projectStore.project?.path,
          ),
          path,
        );
      }
    } else if (mode === 'export-stems') {
      // One WAV/OGG per track, written into the chosen folder. Same render
      // config as the WAV bounce; tracked via the title-bar badge + overlay
      // (the job reveals the folder on finish).
      const cycles = (arrangementStore.loopCycles || 1) * exportLoops;
      void renderStore.track(
        nemusRenderStems(
          projectStore.activeSource,
          path,
          { cycles, format: currentFormat(), sample_rate: exportSampleRate, bit_depth: exportBitDepth, tail_max_secs: exportTail },
          projectStore.project?.path,
        ),
        path,
      );
    } else if (mode === 'export-midi') {
      void runMidiExport(path);
    }
  }

  /** Export the arrangement to a `.mid` file. Instant (note-only, no audio job),
   *  but still surfaced in the shared Downloads & Exports overlay for parity with
   *  the WAV export — start → finish/fail, with a result summary on success. */
  async function runMidiExport(path: string) {
    const id = path;
    transfersStore.start({
      id, kind: 'export', label: basename(path), sublabel: 'Writing MIDI…', progress: 0, path,
    });
    try {
      const r = await nemusExportMidi(
        projectStore.activeSource,
        path,
        projectStore.project?.path,
      );
      transfersStore.finish(id, `${r.notes} notes · ${r.tracks} tracks`);
    } catch (e) {
      transfersStore.fail(id, e instanceof Error ? e.message : String(e));
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
    get exportFormat()      { return currentFormat(); },
    get exportSampleRate()  { return exportSampleRate; },
    get exportBitDepth()    { return exportBitDepth; },
    get exportTail()        { return exportTail; },
    /** Resulting render length for the chosen loop count (read-only echo). */
    get exportCycles()      { return exportCycles(); },

    setExportLoops(n: number) { exportLoops = Math.max(1, Math.round(n) || 1); },
    /** Persist the chosen format to the nemus render config (sticky across
     *  sessions). Drives both the split-button selection and the dialog picker. */
    setExportFormat(f: 'wav' | 'ogg') {
      if (currentFormat() === f) return;
      configStore.setRender({ ...configStore.render, format: f });
    },
    setExportSampleRate(n: number) { exportSampleRate = n; },
    setExportBitDepth(d: string)   { exportBitDepth = d; },
    setExportTail(s: number)       { exportTail = Math.max(0, s); },
    get exportNormalizeOn()        { return exportNormalizeOn; },
    get exportNormalizeTarget()    { return exportNormalizeTarget; },
    setExportNormalizeOn(v: boolean) { exportNormalizeOn = v; },
    setExportNormalizeTarget(n: number) { exportNormalizeTarget = Math.max(-40, Math.min(0, Math.round(n) || -14)); },

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
    exportWav()   { exportLoops = DEFAULT_RENDER_LOOPS; loadRenderDefaults(); exportOptionsOpen = true; },
    /** Quick export (IntelliJ "run current configuration") — skip the options
     *  dialog and go straight to the save picker, reusing the last-chosen format
     *  and the global render defaults. The split-button's main action; "Edit
     *  export…" (→ `exportWav`) is the dialog path for tweaking the details. */
    quickExport() { loadRenderDefaults(); picker = 'export'; },
    /** Whether a loop region is defined — a region export needs a span to bounce. */
    get canExportRegion() {
      const lp = transportUiStore.loop;
      return lp != null && lp.end > lp.start;
    },
    /** Export only the loop region `[start, end)` to a single WAV/OGG. No-op when
     *  no region is set; opens the save picker seeded from Settings → Render. */
    exportRegion() {
      const lp = transportUiStore.loop;
      if (!lp || lp.end <= lp.start) return;
      regionWindow = { start: Math.max(0, Math.floor(lp.start)), cycles: Math.max(1, Math.round(lp.end - lp.start)) };
      loadRenderDefaults();
      picker = 'export-region';
    },
    /** Export per-track stems (one WAV/OGG per track) into a chosen folder —
     *  opens the folder picker; seeds the render config from Settings → Render. */
    exportStems() { exportLoops = DEFAULT_RENDER_LOOPS; loadRenderDefaults(); picker = 'export-stems'; },
    /** Export the arrangement as a Standard MIDI File (note data, no audio) —
     *  opens the `.mid` save picker; the write is instant on confirm. */
    exportMidi()  { picker = 'export-midi'; },
    /** Confirm export options → advance to the save picker (step 2). */
    confirmExportOptions() { exportOptionsOpen = false; picker = 'export'; },
    /** Dismiss the export options dialog without exporting. */
    cancelExportOptions()  { exportOptionsOpen = false; },
    /** Flush the active buffer to disk (no-op when there's no active file). */
    save()        { void projectStore.save().catch(() => {}); },

    cancel()      { picker = null; regionWindow = null; },
    onConfirm,
  };
}

export const projectActions = createProjectActions();
