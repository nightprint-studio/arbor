<script lang="ts">
  /**
   * Nemus command palette — every discoverable window action in one searchable,
   * keyboard-driven list (panel toggles, transport, project ops, settings).
   * Opened with Ctrl+K (same as Arbor).
   *
   * Thin data-provider over the shared `CommandPaletteShell`: nemus only has
   * phase-1 commands (no verbs / target picker), so it just maps its command
   * list to sections and supplies an icon resolver. Type to filter, ↑/↓ to move,
   * Enter to run, Esc to close.
   */
  import type { Component } from 'svelte';
  import {
    Play, Square, SkipBack, SkipForward, FolderPlus, FolderOpen, FilePlus2, Save, Download,
    Files, ListTree, Music4, SlidersHorizontal, Terminal, AlertTriangle,
    Crosshair, BookOpen, Minimize2, PanelLeft, PanelRight, Search, Settings,
    Keyboard, Command, ArrowDownToLine, Boxes, FileInput, FileAudio, SearchCode,
    AlignLeft, PenLine, FileOutput, FileSymlink, Lightbulb, Library, FolderPen, Braces, FlaskConical,
    Repeat, PlayCircle, MapPin, Timer, Hourglass,
  } from 'lucide-svelte';
  import CommandPaletteShell, {
    type PaletteItem, type PaletteSection,
  } from '$lib/components/shared/ui/CommandPaletteShell.svelte';
  import { nemusStore } from '../nemus-store.svelte';
  import { nemusEngine } from '../stores/engine.svelte';
  import { projectStore } from '../stores/project.svelte';
  import { projectActions } from '../stores/project-actions.svelte';
  import { importActions } from '../stores/import-actions.svelte';
  import { editorSelectionStore } from '../stores/editor-selection.svelte';
  import { withFileDeps } from '../editor/nemus-lang';
  import { modelsStore } from '../stores/models.svelte';
  import { mixerStore } from '../stores/mixer.svelte';
  import { arrangementStore } from '../viz/arrangement.svelte';
  import { librariesStore } from '../stores/libraries.svelte';
  import { transportUiStore } from '../stores/transport-ui.svelte';

  let { onClose }: { onClose: () => void } = $props();

  // Icon keys resolved by the shell. Local map keeps nemus self-contained — no
  // dependency on Arbor's PLUGIN_ICONS registry.
  const ICONS: Record<string, Component> = {
    Play, Square, SkipBack, SkipForward, FolderPlus, FolderOpen, FilePlus2, Save, Download,
    Files, ListTree, Music4, SlidersHorizontal, Terminal, AlertTriangle,
    Crosshair, BookOpen, Minimize2, PanelLeft, PanelRight, Search, Settings,
    Keyboard, Command, ArrowDownToLine, Boxes, FileInput, FileAudio, SearchCode,
    AlignLeft, PenLine, FileOutput, FileSymlink, Lightbulb, Library, FolderPen, Braces, FlaskConical,
    Repeat, PlayCircle, MapPin, Timer, Hourglass,
  };
  const iconResolver = (name: string): Component => ICONS[name] ?? Command;

  interface Cmd {
    id: string;
    label: string;
    group: string;
    icon: string;
    keys?: string;
    run: () => void;
  }

  const playing = $derived(nemusEngine.running);

  const commands = $derived<Cmd[]>([
    // Transport
    { id: 'run',  label: playing ? 'Stop' : 'Run', group: 'Transport', icon: playing ? 'Square' : 'Play', keys: 'Shift+F9',
      run: () => void nemusEngine.toggleRun(projectStore.activeSource, projectStore.project?.path) },
    { id: 'seek_start', label: 'Skip to start', group: 'Transport', icon: 'SkipBack', keys: 'Ctrl+Shift+[',
      run: () => void nemusEngine.seekToStart() },
    { id: 'seek_end',   label: 'Skip to end',   group: 'Transport', icon: 'SkipForward', keys: 'Ctrl+Shift+]',
      run: () => void nemusEngine.seekToEnd(arrangementStore.contentEnd) },
    { id: 'play_from_cursor', label: 'Play from cursor (punch-in)', group: 'Transport', icon: 'PlayCircle', keys: 'F8',
      run: () => transportUiStore.playFromCursor() },
    { id: 'toggle_metronome', label: transportUiStore.metronome ? 'Metronome: off' : 'Metronome: on', group: 'Transport', icon: 'Timer', keys: 'Ctrl+Shift+B',
      run: () => transportUiStore.toggleMetronome() },
    { id: 'cycle_count_in', label: transportUiStore.countIn > 0 ? `Count-in: ${transportUiStore.countIn} bar${transportUiStore.countIn > 1 ? 's' : ''} (step)` : 'Count-in: add pre-roll', group: 'Transport', icon: 'Hourglass', keys: 'Ctrl+Shift+U',
      run: () => transportUiStore.cycleCountIn() },
    { id: 'toggle_loop', label: transportUiStore.loopActive ? 'Loop region: off' : 'Loop region: on', group: 'Transport', icon: 'Repeat', keys: 'Ctrl+Shift+L',
      run: () => transportUiStore.toggleLoop() },
    { id: 'clear_loop', label: 'Clear loop region', group: 'Transport', icon: 'Repeat',
      run: () => transportUiStore.clearLoop() },
    { id: 'add_marker', label: 'Add marker at cursor', group: 'Transport', icon: 'MapPin', keys: 'Ctrl+Shift+M',
      run: () => transportUiStore.addMarker(transportUiStore.cursor) },
    { id: 'next_marker', label: 'Jump to next marker', group: 'Transport', icon: 'SkipForward', keys: 'Ctrl+→',
      run: () => transportUiStore.seekNextMarker() },
    { id: 'prev_marker', label: 'Jump to previous marker', group: 'Transport', icon: 'SkipBack', keys: 'Ctrl+←',
      run: () => transportUiStore.seekPrevMarker() },
    { id: 'clear_markers', label: 'Clear all markers', group: 'Transport', icon: 'MapPin',
      run: () => transportUiStore.clearMarkers() },
    { id: 'play_selection', label: 'Play selection one-shot', group: 'Transport', icon: 'FlaskConical', keys: 'Ctrl+Shift+Enter',
      run: async () => {
        const file = projectStore.activeSource;
        const r = editorSelectionStore.primary;
        const src = r ? await withFileDeps(file, file.slice(r.from, r.to)) : file;
        void nemusEngine.playSnippet(src, projectStore.project?.path);
      } },
    { id: 'stop_snippet', label: 'Stop snippet preview', group: 'Transport', icon: 'Square',
      run: () => void nemusEngine.stopSnippet() },
    // Project / file
    { id: 'new_project',  label: 'New project…',     group: 'Project', icon: 'FolderPlus', keys: 'Ctrl+Shift+N', run: () => projectActions.newProject() },
    { id: 'open_project', label: 'Open project…',    group: 'Project', icon: 'FolderOpen', keys: 'Ctrl+O',       run: () => projectActions.openProject() },
    { id: 'open_file',    label: 'Open file…',       group: 'Project', icon: 'FolderOpen', keys: 'Ctrl+Shift+O', run: () => projectActions.openFile() },
    { id: 'new_file',     label: 'New .nemus file…', group: 'Project', icon: 'FilePlus2',  keys: 'Ctrl+N',       run: () => projectActions.newFile() },
    { id: 'save',         label: 'Save file',        group: 'Project', icon: 'Save',       keys: 'Ctrl+S',       run: () => projectActions.save() },
    { id: 'rename_project', label: 'Rename project…', group: 'Project', icon: 'FolderPen', run: () => { if (projectStore.project) nemusStore.openRenameProject(); } },
    { id: 'export',       label: 'Export audio…',    group: 'Project', icon: 'Download',   keys: 'Ctrl+Shift+R', run: () => projectActions.exportWav() },
    { id: 'import',       label: 'Import audio / MIDI…', group: 'Project', icon: 'FileInput', keys: 'Alt+Shift+I', run: () => importActions.start() },
    { id: 'convert_midi', label: 'Convert WAV to MIDI…', group: 'Project', icon: 'FileAudio', run: () => importActions.startConvert() },
    { id: 'dl_basic_pitch', label: 'Download polyphonic model (basic-pitch)', group: 'Project', icon: 'Download', run: () => void modelsStore.download('basic-pitch') },
    { id: 'dl_demucs', label: 'Download stem-split model (Demucs)', group: 'Project', icon: 'Download', run: () => void modelsStore.download('demucs') },
    { id: 'models', label: 'Manage transcription models…', group: 'Project', icon: 'Boxes', run: () => nemusStore.openSettings() },
    { id: 'sync_libs', label: 'Sync libraries (download / update)', group: 'Project', icon: 'Library', run: () => void librariesStore.sync() },
    // Panels
    { id: 'p_files',     label: 'Toggle Files',      group: 'View', icon: 'Files',            run: () => nemusStore.toggleLeft('files') },
    { id: 'p_outline',   label: 'Toggle Outline',    group: 'View', icon: 'ListTree',         run: () => nemusStore.toggleLeft('outline') },
    { id: 'p_sounds',    label: 'Toggle Sound bank', group: 'View', icon: 'Music4',           run: () => nemusStore.toggleLeft('soundbank') },
    { id: 'p_mixer',     label: 'Toggle Mixer',      group: 'View', icon: 'SlidersHorizontal', run: () => nemusStore.toggleBottom('mixer') },
    { id: 'commit_overrides', label: 'Commit mixer overrides to source', group: 'Mixer', icon: 'ArrowDownToLine', keys: 'Alt+Shift+C', run: () => mixerStore.commitAll() },
    { id: 'p_console',   label: 'Toggle Console',    group: 'View', icon: 'Terminal',         run: () => nemusStore.toggleBottom('console') },
    { id: 'p_problems',  label: 'Toggle Problems',   group: 'View', icon: 'AlertTriangle',    run: () => nemusStore.toggleBottom('problems') },
    { id: 'p_jobs',      label: 'Toggle Jobs',       group: 'View', icon: 'Boxes',            run: () => nemusStore.toggleBottom('jobs') },
    { id: 'p_scratch',   label: 'Toggle Scratch (expression evaluator)', group: 'View', icon: 'FlaskConical', keys: 'Ctrl+Shift+S', run: () => nemusStore.toggleBottom('scratch') },
    { id: 'p_inspector', label: 'Toggle Inspector',  group: 'View', icon: 'Crosshair',        run: () => nemusStore.toggleRight('inspector') },
    { id: 'p_docs',      label: 'Toggle Language reference', group: 'View', icon: 'Braces',    run: () => nemusStore.toggleRight('docs') },
    { id: 'c_viz',       label: 'Toggle Arrangement', group: 'View', icon: 'PanelLeft',       run: () => nemusStore.toggleCollapseUi() },
    { id: 'c_editor',    label: 'Toggle Editor',      group: 'View', icon: 'PanelRight',      run: () => nemusStore.toggleCollapseTabpane() },
    { id: 'zen',         label: 'Toggle Zen mode',   group: 'View', icon: 'Minimize2', keys: 'Ctrl+Shift+Z', run: () => nemusStore.toggleZen() },
    { id: 'find',        label: 'Search Console / Problems', group: 'View', icon: 'Search', keys: 'Ctrl+F', run: () => nemusStore.requestFind() },
    { id: 'find_usages', label: 'Find usages of symbol at caret', group: 'View', icon: 'SearchCode', keys: 'Alt+F7', run: () => nemusStore.requestFindUsages() },
    { id: 'structure',   label: 'File structure (find method / variable)', group: 'View', icon: 'ListTree', keys: 'Ctrl+F12', run: () => nemusStore.requestStructure() },
    // Edit
    { id: 'format', label: 'Format document', group: 'Edit', icon: 'AlignLeft', keys: 'Alt+Shift+L', run: () => nemusStore.requestFormat() },
    { id: 'rename', label: 'Rename symbol…', group: 'Edit', icon: 'PenLine', keys: 'Shift+F6', run: () => nemusStore.requestRename() },
    { id: 'extract', label: 'Extract selection to let…', group: 'Edit', icon: 'FileOutput', keys: 'Alt+Shift+V', run: () => nemusStore.requestExtract() },
    { id: 'inline', label: 'Inline let', group: 'Edit', icon: 'FileSymlink', keys: 'Alt+Shift+N', run: () => nemusStore.requestInline() },
    { id: 'intentions', label: 'Show context actions / quick-fixes', group: 'Edit', icon: 'Lightbulb', keys: 'Alt+Enter', run: () => nemusStore.requestIntentions() },
    // Window
    { id: 'docs',      label: 'Documentation',      group: 'Window', icon: 'BookOpen', keys: 'F1',       run: () => nemusStore.openDocs() },
    { id: 'settings',  label: 'Settings…',          group: 'Window', icon: 'Settings', keys: 'Ctrl+,',   run: () => nemusStore.openSettings() },
    { id: 'shortcuts', label: 'Keyboard Shortcuts', group: 'Window', icon: 'Keyboard', keys: 'Shift+F1', run: () => nemusStore.openShortcuts() },
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
        .map((c) => ({ c, s: score(c.label + ' ' + c.group, q) }))
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
