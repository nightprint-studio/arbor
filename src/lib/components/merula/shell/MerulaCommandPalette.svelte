<script lang="ts">
  /**
   * Merula command palette — every discoverable window action in one searchable,
   * keyboard-driven list (panel toggles, transport, project ops, settings).
   * Opened with Ctrl+K (same as Arbor).
   *
   * Thin data-provider over the shared `CommandPaletteShell`: merula only has
   * phase-1 commands (no verbs / target picker), so it just maps its command
   * list to sections and supplies an icon resolver. Type to filter, ↑/↓ to move,
   * Enter to run, Esc to close.
   */
  import type { IconComponent } from '$lib/types/icon';
  import {
    Play, Square, SkipBack, SkipForward, FolderPlus, FolderOpen, FilePlus2, Save, Download,
    Files, ListTree, Music4, SlidersHorizontal, Terminal, AlertTriangle,
    Crosshair, BookOpen, Minimize2, Maximize2, PanelLeft, PanelRight, Search, Settings, Piano,
    Keyboard, Command, ArrowDownToLine, Boxes, FileInput, FileAudio, SearchCode,
    AlignLeft, PenLine, FileOutput, FileSymlink, Lightbulb, Library, FolderPen, Braces, FlaskConical,
    Repeat, PlayCircle, MapPin, Timer, Hourglass, Gauge, Plus, Minus, RotateCcw, ZoomIn, ZoomOut, Map as MapIcon, StretchVertical,
    FileMusic, Layers, Crop, Snowflake, Grid3x3, LayoutGrid, FilePen, Trash2, Rewind, FastForward,
  } from 'lucide-svelte';
  import CommandPaletteShell, {
    type PaletteItem, type PaletteSection,
  } from '$lib/components/shared/ui/CommandPaletteShell.svelte';
  import { merulaStore } from '../merula-store.svelte';
  import { merulaEngine } from '../stores/engine.svelte';
  import { configStore } from '../stores/config.svelte';
  import { projectStore } from '../stores/project.svelte';
  import { projectActions } from '../stores/project-actions.svelte';
  import { importActions } from '../stores/import-actions.svelte';
  import { editorSelectionStore } from '../stores/editor-selection.svelte';
  import { withFileDeps } from '../editor/merula-lang';
  import { modelsStore } from '../stores/models.svelte';
  import { mixerStore } from '../stores/mixer.svelte';
  import { arrangementStore } from '../viz/arrangement.svelte';
  import { librariesStore } from '../stores/libraries.svelte';
  import { transportUiStore } from '../stores/transport-ui.svelte';
  import { tempoStore } from '../stores/tempo.svelte';
  import { arrViewOptions } from '../viz/arr-view-options.svelte';
  import { laneSizes } from '../viz/lane-sizes.svelte';
  import { launcherStore } from '../stores/launcher.svelte';

  let { onClose }: { onClose: () => void } = $props();

  // Icon keys resolved by the shell. Local map keeps merula self-contained — no
  // dependency on Arbor's PLUGIN_ICONS registry.
  const ICONS: Record<string, IconComponent> = {
    Play, Square, SkipBack, SkipForward, FolderPlus, FolderOpen, FilePlus2, Save, Download,
    Files, ListTree, Music4, SlidersHorizontal, Terminal, AlertTriangle,
    Crosshair, BookOpen, Minimize2, Maximize2, PanelLeft, PanelRight, Search, Settings, Piano,
    Keyboard, Command, ArrowDownToLine, Boxes, FileInput, FileAudio, SearchCode,
    AlignLeft, PenLine, FileOutput, FileSymlink, Lightbulb, Library, FolderPen, Braces, FlaskConical,
    Repeat, PlayCircle, MapPin, Timer, Hourglass, Gauge, Plus, Minus, RotateCcw, ZoomIn, ZoomOut, Map: MapIcon, StretchVertical,
    FileMusic, Layers, Crop, Snowflake, Grid3x3, LayoutGrid, FilePen, Trash2, Rewind, FastForward,
  };
  const iconResolver = (name: string): IconComponent => ICONS[name] ?? Command;

  interface Cmd {
    id: string;
    label: string;
    group: string;
    icon: string;
    keys?: string;
    /** One-line description shown as a muted second line + searched alongside the label. */
    desc?: string;
    run: () => void;
  }

  const playing = $derived(merulaEngine.running);

  const commands = $derived<Cmd[]>([
    // Transport
    { id: 'run',  label: playing ? 'Stop' : 'Run', group: 'Transport', icon: playing ? 'Square' : 'Play', keys: 'Shift+F9',
      desc: playing ? 'Stop the scheduler (the playhead keeps its position).' : 'Evaluate the active file and start playback.',
      run: () => void merulaEngine.toggleRun(projectStore.activeSource, projectStore.project?.path) },
    { id: 'seek_start', label: 'Skip to start', group: 'Transport', icon: 'SkipBack', keys: 'Ctrl+Shift+[',
      desc: 'Jump the playhead back to the beginning of the song.',
      run: () => void merulaEngine.seekToStart() },
    { id: 'seek_end',   label: 'Skip to end',   group: 'Transport', icon: 'SkipForward', keys: 'Ctrl+Shift+]',
      desc: 'Jump the playhead to the end of the arrangement content.',
      run: () => void merulaEngine.seekToEnd(arrangementStore.contentEnd) },
    { id: 'step_back', label: 'Step back', group: 'Transport', icon: 'Rewind', keys: 'Ctrl+[',
      desc: `Move the playhead back ${configStore.skipStepLabel}.`,
      run: () => transportUiStore.stepBy(-configStore.skipStep, arrangementStore.contentEnd) },
    { id: 'step_fwd', label: 'Step forward', group: 'Transport', icon: 'FastForward', keys: 'Ctrl+]',
      desc: `Move the playhead forward ${configStore.skipStepLabel}.`,
      run: () => transportUiStore.stepBy(configStore.skipStep, arrangementStore.contentEnd) },
    { id: 'play_from_cursor', label: 'Play from cursor (punch-in)', group: 'Transport', icon: 'PlayCircle', keys: 'F8',
      desc: 'Start playback from the editor caret position.',
      run: () => transportUiStore.playFromCursor() },
    { id: 'toggle_metronome', label: transportUiStore.metronome ? 'Metronome: off' : 'Metronome: on', group: 'Transport', icon: 'Timer', keys: 'Ctrl+Shift+B',
      desc: 'Toggle the audible click track (monitoring aid, bypasses the mix).',
      run: () => transportUiStore.toggleMetronome() },
    { id: 'cycle_count_in', label: transportUiStore.countIn > 0 ? `Count-in: ${transportUiStore.countIn} bar${transportUiStore.countIn > 1 ? 's' : ''} (step)` : 'Count-in: add pre-roll', group: 'Transport', icon: 'Hourglass', keys: 'Ctrl+Shift+U',
      desc: 'Cycle the metronome pre-roll length before playback starts.',
      run: () => transportUiStore.cycleCountIn() },
    { id: 'tap_tempo', label: 'Tap tempo', group: 'Transport', icon: 'Gauge',
      desc: 'Set the tempo by tapping this command rhythmically.',
      run: () => tempoStore.tap() },
    { id: 'tempo_up', label: 'Tempo +1 BPM', group: 'Transport', icon: 'Plus',
      desc: 'Nudge the live tempo up by one beat per minute.',
      run: () => tempoStore.nudge(1) },
    { id: 'tempo_down', label: 'Tempo −1 BPM', group: 'Transport', icon: 'Minus',
      desc: 'Nudge the live tempo down by one beat per minute.',
      run: () => tempoStore.nudge(-1) },
    { id: 'tempo_reset', label: 'Reset tempo to the score', group: 'Transport', icon: 'RotateCcw',
      desc: 'Drop the live tempo override and follow the script tempo again.',
      run: () => tempoStore.reset() },
    { id: 'toggle_loop', label: transportUiStore.loopActive ? 'Loop region: off' : 'Loop region: on', group: 'Transport', icon: 'Repeat', keys: 'Ctrl+Shift+L',
      desc: 'Loop playback over the selected region instead of the whole song.',
      run: () => transportUiStore.toggleLoop() },
    { id: 'clear_loop', label: 'Clear loop region', group: 'Transport', icon: 'Repeat',
      desc: 'Remove the loop region so playback covers the full arrangement.',
      run: () => transportUiStore.clearLoop() },
    { id: 'add_marker', label: 'Add marker at cursor', group: 'Transport', icon: 'MapPin', keys: 'Ctrl+Shift+M',
      desc: 'Drop a named timeline marker at the current cursor position.',
      run: () => transportUiStore.addMarker(transportUiStore.cursor) },
    { id: 'next_marker', label: 'Jump to next marker', group: 'Transport', icon: 'SkipForward', keys: 'Ctrl+→',
      desc: 'Move the playhead to the next timeline marker.',
      run: () => transportUiStore.seekNextMarker() },
    { id: 'prev_marker', label: 'Jump to previous marker', group: 'Transport', icon: 'SkipBack', keys: 'Ctrl+←',
      desc: 'Move the playhead to the previous timeline marker.',
      run: () => transportUiStore.seekPrevMarker() },
    { id: 'clear_markers', label: 'Clear all markers', group: 'Transport', icon: 'MapPin',
      desc: 'Remove every timeline marker from the arrangement.',
      run: () => transportUiStore.clearMarkers() },
    { id: 'play_selection', label: 'Play selection one-shot', group: 'Transport', icon: 'FlaskConical', keys: 'Ctrl+Shift+Enter',
      desc: 'Audition the selected expression once, without touching the song.',
      run: async () => {
        const file = projectStore.activeSource;
        const r = editorSelectionStore.primary;
        const src = r ? await withFileDeps(file, file.slice(r.from, r.to)) : file;
        void merulaEngine.playSnippet(src, projectStore.project?.path);
      } },
    { id: 'stop_snippet', label: 'Stop snippet preview', group: 'Transport', icon: 'Square',
      desc: 'Cut off any one-shot audition currently sounding.',
      run: () => void merulaEngine.stopSnippet() },
    // Project / file
    { id: 'new_project',  label: 'New project…',     group: 'Project', icon: 'FolderPlus', keys: 'Ctrl+Shift+N', desc: 'Scaffold a new merula project folder with a starter file.', run: () => projectActions.newProject() },
    { id: 'open_project', label: 'Open project…',    group: 'Project', icon: 'FolderOpen', keys: 'Ctrl+O',       desc: 'Open an existing merula project folder.', run: () => projectActions.openProject() },
    { id: 'open_file',    label: 'Open file…',       group: 'Project', icon: 'FolderOpen', keys: 'Ctrl+Shift+O', desc: 'Open a single .merula file as an editor tab.', run: () => projectActions.openFile() },
    { id: 'new_file',     label: 'New .merula file…', group: 'Project', icon: 'FilePlus2',  keys: 'Ctrl+N',       desc: 'Create a new .merula file in the open project.', run: () => projectActions.newFile() },
    { id: 'save',         label: 'Save file',        group: 'Project', icon: 'Save',       keys: 'Ctrl+S',       desc: 'Flush the active editor buffer to disk.', run: () => projectActions.save() },
    { id: 'rename_project', label: 'Rename project…', group: 'Project', icon: 'FolderPen', desc: 'Change the project name stored in merula.toml.', run: () => { if (projectStore.project) merulaStore.openRenameProject(); } },
    { id: 'workspaces', label: 'Manage workspaces…', group: 'Project', icon: 'LayoutGrid', desc: 'Create, switch and colour named groups of projects.', run: () => merulaStore.openWorkspaces() },
    { id: 'rename_file', label: 'Rename active file…', group: 'Project', icon: 'FilePen', desc: 'Rename the .merula file open in the editor.',
      run: () => { const p = projectStore.activeFilePath; if (p) merulaStore.openRenameFile(p); } },
    { id: 'delete_file', label: 'Delete active file…', group: 'Project', icon: 'Trash2', desc: 'Move the active .merula file to the Recycle Bin.',
      run: () => { const p = projectStore.activeFilePath; if (p) merulaStore.openDeleteFile(p); } },
    { id: 'export',       label: 'Export audio…',    group: 'Project', icon: 'Download',   keys: 'Ctrl+Shift+R', desc: 'Bounce the arrangement to a WAV / OGG file.', run: () => projectActions.exportWav() },
    { id: 'export_region', label: 'Export loop region…', group: 'Project', icon: 'Crop',  desc: 'Bounce only the loop region to a single file.', run: () => projectActions.exportRegion() },
    { id: 'export_stems', label: 'Export stems…',    group: 'Project', icon: 'Layers',     desc: 'Bounce one audio file per track into a folder.', run: () => projectActions.exportStems() },
    { id: 'export_midi',  label: 'Export MIDI…',     group: 'Project', icon: 'FileMusic',  desc: 'Write the arrangement notes to a Standard MIDI File.', run: () => projectActions.exportMidi() },
    { id: 'check_levels', label: 'Check levels (clip analysis)', group: 'Project', icon: 'Gauge', desc: 'Offline-scan for clipping without playing the song.', run: () => projectActions.checkLevels() },
    { id: 'import',       label: 'Import audio / MIDI…', group: 'Project', icon: 'FileInput', keys: 'Alt+Shift+I', desc: 'Transcribe audio or convert MIDI into a .merula file.', run: () => importActions.start() },
    { id: 'convert_midi', label: 'Convert WAV to MIDI…', group: 'Project', icon: 'FileAudio', desc: 'Transcribe a WAV file straight to a .mid file.', run: () => importActions.startConvert() },
    { id: 'dl_basic_pitch', label: 'Download polyphonic model (basic-pitch)', group: 'Project', icon: 'Download', desc: 'Fetch the polyphonic pitch-detection model for import.', run: () => void modelsStore.download('basic-pitch') },
    { id: 'dl_demucs', label: 'Download stem-split model (Demucs)', group: 'Project', icon: 'Download', desc: 'Fetch the Demucs source-separation model for import.', run: () => void modelsStore.download('demucs') },
    { id: 'models', label: 'Manage transcription models…', group: 'Project', icon: 'Boxes', desc: 'Open Settings to manage downloaded transcription models.', run: () => merulaStore.openSettings() },
    { id: 'sync_libs', label: 'Sync libraries (download / update)', group: 'Project', icon: 'Library', desc: 'Resolve and fetch the project’s external $lib modules.', run: () => void librariesStore.sync() },
    // Panels
    { id: 'p_files',     label: 'Toggle Files',      group: 'View', icon: 'Files',            desc: 'Show or hide the project files sidebar.', run: () => merulaStore.toggleLeft('files') },
    { id: 'p_outline',   label: 'Toggle Outline',    group: 'View', icon: 'ListTree',         desc: 'Show or hide the symbol outline of the active file.', run: () => merulaStore.toggleLeft('outline') },
    { id: 'p_sounds',    label: 'Toggle Sound bank', group: 'View', icon: 'Music4',           desc: 'Browse and preview the resolvable instruments.', run: () => merulaStore.toggleLeft('soundbank') },
    { id: 'p_mixer',     label: 'Toggle Mixer',      group: 'View', icon: 'SlidersHorizontal', desc: 'Show or hide the per-track mixer strips.', run: () => merulaStore.toggleBottom('mixer') },
    { id: 'commit_overrides', label: 'Commit mixer overrides to source', group: 'Mixer', icon: 'ArrowDownToLine', keys: 'Alt+Shift+C', desc: 'Write the live mixer knob values back into the .merula source.', run: () => mixerStore.commitAll() },
    { id: 'p_console',   label: 'Toggle Console',    group: 'View', icon: 'Terminal',         desc: 'Show or hide the script log console.', run: () => merulaStore.toggleBottom('console') },
    { id: 'p_problems',  label: 'Toggle Problems',   group: 'View', icon: 'AlertTriangle',    desc: 'Show or hide the diagnostics (errors / warnings) list.', run: () => merulaStore.toggleBottom('problems') },
    { id: 'p_jobs',      label: 'Toggle Jobs',       group: 'View', icon: 'Boxes',            desc: 'Show or hide background job output (renders, downloads).', run: () => merulaStore.toggleBottom('jobs') },
    { id: 'p_scratch',   label: 'Toggle Scratch (expression evaluator)', group: 'View', icon: 'FlaskConical', keys: 'Ctrl+Shift+S', desc: 'Evaluate / audition arbitrary .merula expressions in isolation.', run: () => merulaStore.toggleBottom('scratch') },
    { id: 'p_keyboard',  label: 'Toggle Keyboard (live notes)', group: 'View', icon: 'Piano', desc: 'Piano that lights the notes sounding at the playhead.', run: () => merulaStore.toggleBottom('keyboard') },
    { id: 'p_launcher',  label: 'Toggle Clip launcher', group: 'View', icon: 'LayoutGrid', keys: 'Ctrl+Shift+G', desc: 'Show or hide the scene / clip launcher grid.', run: () => merulaStore.toggleBottom('launcher') },
    // Launcher: one entry per declared scene + stop-all (keyboard-first firing).
    ...launcherStore.scenes.map((s) => ({
      id: `launch_scene_${s.name}`, label: `Launch scene "${s.name}"`, group: 'Launcher', icon: 'LayoutGrid',
      desc: 'Fire this scene’s clips, quantized to the next boundary.',
      run: () => launcherStore.launchScene(s.name),
    })),
    ...(launcherStore.anyActive
      ? [{ id: 'launch_stop_all', label: 'Stop all clips', group: 'Launcher', icon: 'Square', desc: 'Return every track to its base pattern.', run: () => launcherStore.stopAll() }]
      : []),
    { id: 'launch_quantum', label: `Launch quantization: ${launcherStore.quantum} ${launcherStore.quantum === 1 ? 'cycle' : 'cycles'} (cycle)`, group: 'Launcher', icon: 'LayoutGrid', desc: 'Cycle the boundary clips snap to when launched.', run: () => launcherStore.cycleQuantum() },
    { id: 'p_minimap',   label: arrViewOptions.minimap ? 'Hide minimap' : 'Show minimap', group: 'View', icon: 'Map', desc: 'Toggle the arrangement overview minimap.', run: () => arrViewOptions.toggleMinimap() },
    { id: 'p_velocity',  label: arrViewOptions.velocity ? 'Hide velocity heatmap' : 'Show velocity heatmap', group: 'View', icon: 'Gauge', desc: 'Tint arrangement haps by velocity / gain.', run: () => arrViewOptions.toggleVelocity() },
    { id: 'zoom_in',     label: 'Zoom in timeline',  group: 'View', icon: 'ZoomIn',  desc: 'Increase the arrangement horizontal zoom.', run: () => arrViewOptions.zoomIn() },
    { id: 'zoom_out',    label: 'Zoom out timeline', group: 'View', icon: 'ZoomOut', desc: 'Decrease the arrangement horizontal zoom.', run: () => arrViewOptions.zoomOut() },
    { id: 'zoom_reset',  label: 'Reset timeline zoom', group: 'View', icon: 'RotateCcw', desc: 'Restore the default arrangement zoom level.', run: () => arrViewOptions.zoomReset() },
    { id: 'reset_lane_heights', label: 'Reset lane heights', group: 'View', icon: 'StretchVertical', desc: 'Restore every arrangement lane to its default height.', run: () => laneSizes.resetAll() },
    { id: 'p_inspector', label: 'Toggle Inspector',  group: 'View', icon: 'Crosshair',        desc: 'Show or hide the selected track inspector.', run: () => merulaStore.toggleRight('inspector') },
    { id: 'p_docs',      label: 'Toggle Language reference', group: 'View', icon: 'Braces',    desc: 'Show or hide the .merula language reference panel.', run: () => merulaStore.toggleRight('docs') },
    { id: 'c_viz',       label: 'Toggle Arrangement', group: 'View', icon: 'PanelLeft',       desc: 'Collapse or restore the arrangement pane.', run: () => merulaStore.toggleCollapseUi() },
    { id: 'c_editor',    label: 'Toggle Editor',      group: 'View', icon: 'PanelRight',      desc: 'Collapse or restore the editor pane.', run: () => merulaStore.toggleCollapseTabpane() },
    { id: 'zen',         label: 'Toggle Zen mode',   group: 'View', icon: 'Minimize2', keys: 'Ctrl+Shift+Z', desc: 'Hide the rails, footer and bottom panel for focus.', run: () => merulaStore.toggleZen() },
    { id: 'performance', label: merulaStore.performance ? 'Exit performance mode' : 'Performance mode (full-screen stage)', group: 'View', icon: 'Maximize2', keys: 'F11', desc: 'Full-screen distraction-free stage for live play.', run: () => merulaStore.togglePerformance() },
    { id: 'find',        label: 'Search Console / Problems', group: 'View', icon: 'Search', keys: 'Ctrl+F', desc: 'Filter the active bottom panel by text.', run: () => merulaStore.requestFind() },
    { id: 'find_usages', label: 'Find usages of symbol at caret', group: 'View', icon: 'SearchCode', keys: 'Alt+F7', desc: 'List every reference to the symbol under the caret.', run: () => merulaStore.requestFindUsages() },
    { id: 'structure',   label: 'File structure (find method / variable)', group: 'View', icon: 'ListTree', keys: 'Ctrl+F12', desc: 'Jump to a declaration via the file-structure picker.', run: () => merulaStore.requestStructure() },
    // Edit
    { id: 'format', label: 'Format document', group: 'Edit', icon: 'AlignLeft', keys: 'Alt+Shift+L', desc: 'Reformat the active file to canonical style.', run: () => merulaStore.requestFormat() },
    { id: 'rename', label: 'Rename symbol…', group: 'Edit', icon: 'PenLine', keys: 'Shift+F6', desc: 'Rename the symbol at the caret across the file.', run: () => merulaStore.requestRename() },
    { id: 'extract', label: 'Extract selection to let…', group: 'Edit', icon: 'FileOutput', keys: 'Alt+Shift+V', desc: 'Lift the selected expression into a named let binding.', run: () => merulaStore.requestExtract() },
    { id: 'inline', label: 'Inline let', group: 'Edit', icon: 'FileSymlink', keys: 'Alt+Shift+N', desc: 'Replace a let binding with its value at each use.', run: () => merulaStore.requestInline() },
    { id: 'freeze', label: 'Freeze pattern to notes', group: 'Edit', icon: 'Snowflake', desc: 'Materialize a generated pattern into literal notes.', run: () => merulaStore.requestFreeze() },
    { id: 'euclid', label: 'Insert euclidean rhythm…', group: 'Edit', icon: 'Grid3x3', desc: 'Insert a euclid(...) rhythm via a small builder.', run: () => merulaStore.requestEuclid() },
    { id: 'chordprog', label: 'Insert chord progression…', group: 'Edit', icon: 'Music4', desc: 'Insert a chord progression via a small builder.', run: () => merulaStore.requestChord() },
    { id: 'intentions', label: 'Show context actions / quick-fixes', group: 'Edit', icon: 'Lightbulb', keys: 'Alt+Enter', desc: 'Open the quick-fix / intentions popup at the caret.', run: () => merulaStore.requestIntentions() },
    // Window
    { id: 'docs',      label: 'Documentation',      group: 'Window', icon: 'BookOpen', keys: 'F1',       desc: 'Open the in-app merula documentation.', run: () => merulaStore.openDocs() },
    { id: 'settings',  label: 'Settings…',          group: 'Window', icon: 'Settings', keys: 'Ctrl+,',   desc: 'Open the merula settings (audio, render, editor).', run: () => merulaStore.openSettings() },
    { id: 'shortcuts', label: 'Keyboard Shortcuts', group: 'Window', icon: 'Keyboard', keys: 'Shift+F1', desc: 'View the full keyboard shortcut reference.', run: () => merulaStore.openShortcuts() },
  ]);

  let query = $state('');

  /** Match score: exact > prefix > word-boundary > substring > 0. */
  function score(text: string, q: string): number {
    if (!q) return 50;
    const t = text.toLowerCase();
    const lq = q.toLowerCase();
    if (t === lq) return 100;
    if (t.startsWith(lq)) return 85;
    if (t.split(/[\s\-_/.]+/).some((w) => w.startsWith(lq))) return 70;
    if (t.includes(lq)) return 55;
    return 0;
  }

  function toItem(c: Cmd): PaletteItem {
    return {
      id: c.id,
      title: c.label,
      subtitle: c.desc,
      icon: c.icon,
      shortcut: c.keys,
      action: () => { onClose(); c.run(); }, // close first so a re-opened overlay isn't dismissed
    };
  }

  const sections = $derived.by<PaletteSection[]>(() => {
    const q = query.trim();
    // With a query: one ranked list (keyboard-first — no header hopping).
    if (q) {
      const items = commands
        .map((c) => ({ c, s: score(c.label + ' ' + c.group + ' ' + (c.desc ?? ''), q) }))
        .filter((x) => x.s > 0)
        .sort((a, b) => b.s - a.s)
        .map((x) => toItem(x.c));
      return items.length ? [{ id: 'commands', label: 'Commands', items }] : [];
    }
    // No query: grouped by category in declaration order.
    const groups = new Map<string, PaletteItem[]>();
    for (const c of commands) {
      const list = groups.get(c.group) ?? [];
      list.push(toItem(c));
      groups.set(c.group, list);
    }
    return [...groups].map(([label, items]) => ({ id: `g:${label}`, label, items }));
  });
</script>

<CommandPaletteShell
  {onClose}
  {iconResolver}
  {sections}
  bind:query
  placeholder="Type a command…"
>
  {#snippet emptyMessage()}
    No matching command
  {/snippet}
</CommandPaletteShell>
