<script lang="ts">
  /**
   * Grove command palette — every discoverable window action in one searchable,
   * keyboard-driven list (panel toggles, transport, project ops, settings).
   * Opened with Ctrl+Shift+P.
   *
   * Thin data-provider over the shared `CommandPaletteShell`: grove only has
   * phase-1 commands (no verbs / target picker), so it just maps its command
   * list to sections and supplies an icon resolver. Type to filter, ↑/↓ to move,
   * Enter to run, Esc to close.
   */
  import type { Component } from 'svelte';
  import {
    Play, Square, FolderPlus, FolderOpen, FilePlus2, Save, Download,
    Files, ListTree, Music4, SlidersHorizontal, Terminal, AlertTriangle,
    Crosshair, BookOpen, Minimize2, PanelLeft, PanelRight, Search, Settings,
    Keyboard, Command, ArrowDownToLine,
  } from 'lucide-svelte';
  import CommandPaletteShell, {
    type PaletteItem, type PaletteSection,
  } from '$lib/components/shared/ui/CommandPaletteShell.svelte';
  import { groveStore } from '../grove-store.svelte';
  import { groveEngine } from '../stores/engine.svelte';
  import { projectStore } from '../stores/project.svelte';
  import { projectActions } from '../stores/project-actions.svelte';
  import { mixerStore } from '../stores/mixer.svelte';

  let { onClose }: { onClose: () => void } = $props();

  // Icon keys resolved by the shell. Local map keeps grove self-contained — no
  // dependency on Arbor's PLUGIN_ICONS registry.
  const ICONS: Record<string, Component> = {
    Play, Square, FolderPlus, FolderOpen, FilePlus2, Save, Download,
    Files, ListTree, Music4, SlidersHorizontal, Terminal, AlertTriangle,
    Crosshair, BookOpen, Minimize2, PanelLeft, PanelRight, Search, Settings,
    Keyboard, Command, ArrowDownToLine,
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

  const playing = $derived(groveEngine.running);

  const commands = $derived<Cmd[]>([
    // Transport
    { id: 'run',  label: playing ? 'Stop' : 'Run', group: 'Transport', icon: playing ? 'Square' : 'Play', keys: 'Ctrl+Space',
      run: () => void groveEngine.toggleRun(projectStore.activeSource, projectStore.project?.path) },
    // Project / file
    { id: 'new_project',  label: 'New project…',     group: 'Project', icon: 'FolderPlus', keys: 'Ctrl+Shift+N', run: () => projectActions.newProject() },
    { id: 'open_project', label: 'Open project…',    group: 'Project', icon: 'FolderOpen', keys: 'Ctrl+O',       run: () => projectActions.openProject() },
    { id: 'open_file',    label: 'Open file…',       group: 'Project', icon: 'FolderOpen', keys: 'Ctrl+Shift+O', run: () => projectActions.openFile() },
    { id: 'new_file',     label: 'New .grove file…', group: 'Project', icon: 'FilePlus2',  keys: 'Ctrl+N',       run: () => projectActions.newFile() },
    { id: 'save',         label: 'Save file',        group: 'Project', icon: 'Save',       keys: 'Ctrl+S',       run: () => projectActions.save() },
    { id: 'export',       label: 'Export to WAV…',   group: 'Project', icon: 'Download',   keys: 'Ctrl+Shift+R', run: () => projectActions.exportWav() },
    // Panels
    { id: 'p_files',     label: 'Toggle Files',      group: 'View', icon: 'Files',            run: () => groveStore.toggleLeft('files') },
    { id: 'p_outline',   label: 'Toggle Outline',    group: 'View', icon: 'ListTree',         run: () => groveStore.toggleLeft('outline') },
    { id: 'p_sounds',    label: 'Toggle Sound bank', group: 'View', icon: 'Music4',           run: () => groveStore.toggleLeft('soundbank') },
    { id: 'p_mixer',     label: 'Toggle Mixer',      group: 'View', icon: 'SlidersHorizontal', run: () => groveStore.toggleBottom('mixer') },
    { id: 'commit_overrides', label: 'Commit mixer overrides to source', group: 'Mixer', icon: 'ArrowDownToLine', keys: 'Alt+Shift+C', run: () => mixerStore.commitAll() },
    { id: 'p_console',   label: 'Toggle Console',    group: 'View', icon: 'Terminal',         run: () => groveStore.toggleBottom('console') },
    { id: 'p_problems',  label: 'Toggle Problems',   group: 'View', icon: 'AlertTriangle',    run: () => groveStore.toggleBottom('problems') },
    { id: 'p_inspector', label: 'Toggle Inspector',  group: 'View', icon: 'Crosshair',        run: () => groveStore.toggleRight('inspector') },
    { id: 'p_docs',      label: 'Toggle Docs',       group: 'View', icon: 'BookOpen',         run: () => groveStore.toggleRight('docs') },
    { id: 'c_viz',       label: 'Toggle Arrangement', group: 'View', icon: 'PanelLeft',       run: () => groveStore.toggleCollapseUi() },
    { id: 'c_editor',    label: 'Toggle Editor',      group: 'View', icon: 'PanelRight',      run: () => groveStore.toggleCollapseTabpane() },
    { id: 'zen',         label: 'Toggle Zen mode',   group: 'View', icon: 'Minimize2', keys: 'Ctrl+Shift+Z', run: () => groveStore.toggleZen() },
    { id: 'find',        label: 'Search Console / Problems', group: 'View', icon: 'Search', keys: 'Ctrl+F', run: () => groveStore.requestFind() },
    // Window
    { id: 'settings',  label: 'Settings…',          group: 'Window', icon: 'Settings', keys: 'Ctrl+,', run: () => groveStore.openSettings() },
    { id: 'shortcuts', label: 'Keyboard Shortcuts', group: 'Window', icon: 'Keyboard', keys: 'F1',     run: () => groveStore.openShortcuts() },
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
  width="560px"
>
  {#snippet emptyMessage()}
    No matching command
  {/snippet}
</CommandPaletteShell>
