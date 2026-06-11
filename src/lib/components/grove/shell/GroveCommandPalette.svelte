<script lang="ts">
  /**
   * Grove command palette — every discoverable window action in one searchable,
   * keyboard-driven list (the keyboard-first umbrella for panel toggles, the
   * transport, project ops and settings). Opened with Ctrl+Shift+P.
   *
   * Self-contained overlay (grove stays extractable — imports only shared/ui-
   * level primitives + grove-local stores). Type to filter, ↑/↓ to move, Enter
   * to run, Esc to close. The input keeps focus the whole time; the active row is
   * tracked via `aria-activedescendant`.
   */
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import {
    Play, Square, FolderPlus, FolderOpen, FilePlus2, Save, Download,
    Files, ListTree, Music4, SlidersHorizontal, Terminal, AlertTriangle,
    Crosshair, BookOpen, Minimize2, PanelLeft, PanelRight, Search, Settings,
    Keyboard, Command, ArrowDownToLine,
  } from 'lucide-svelte';
  import { animStore } from '$lib/stores/animations.svelte';
  import { groveStore } from '../grove-store.svelte';
  import { groveEngine } from '../stores/engine.svelte';
  import { projectStore } from '../stores/project.svelte';
  import { projectActions } from '../stores/project-actions.svelte';
  import { mixerStore } from '../stores/mixer.svelte';

  let { onClose }: { onClose: () => void } = $props();

  interface Cmd {
    id: string;
    label: string;
    group: string;
    icon: any;
    keys?: string;
    run: () => void;
  }

  const playing = $derived(groveEngine.running);

  const commands = $derived<Cmd[]>([
    // Transport
    { id: 'run',  label: playing ? 'Stop' : 'Run', group: 'Transport', icon: playing ? Square : Play, keys: 'Ctrl+Space',
      run: () => void groveEngine.toggleRun(projectStore.activeSource, projectStore.project?.path) },
    // Project / file
    { id: 'new_project',  label: 'New project…',   group: 'Project', icon: FolderPlus, keys: 'Ctrl+Shift+N', run: () => projectActions.newProject() },
    { id: 'open_project', label: 'Open project…',  group: 'Project', icon: FolderOpen, keys: 'Ctrl+O',       run: () => projectActions.openProject() },
    { id: 'open_file',    label: 'Open file…',     group: 'Project', icon: FolderOpen, keys: 'Ctrl+Shift+O', run: () => projectActions.openFile() },
    { id: 'new_file',     label: 'New .grove file…', group: 'Project', icon: FilePlus2, keys: 'Ctrl+N',      run: () => projectActions.newFile() },
    { id: 'save',         label: 'Save file',      group: 'Project', icon: Save,       keys: 'Ctrl+S',       run: () => projectActions.save() },
    { id: 'export',       label: 'Export to WAV…', group: 'Project', icon: Download,   keys: 'Ctrl+Shift+R', run: () => projectActions.exportWav() },
    // Panels
    { id: 'p_files',     label: 'Toggle Files',      group: 'View', icon: Files,            run: () => groveStore.toggleLeft('files') },
    { id: 'p_outline',   label: 'Toggle Outline',    group: 'View', icon: ListTree,         run: () => groveStore.toggleLeft('outline') },
    { id: 'p_sounds',    label: 'Toggle Sound bank', group: 'View', icon: Music4,           run: () => groveStore.toggleLeft('soundbank') },
    { id: 'p_mixer',     label: 'Toggle Mixer',      group: 'View', icon: SlidersHorizontal, run: () => groveStore.toggleBottom('mixer') },
    { id: 'commit_overrides', label: 'Commit mixer overrides to source', group: 'Mixer', icon: ArrowDownToLine, keys: 'Alt+Shift+C', run: () => mixerStore.commitAll() },
    { id: 'p_console',   label: 'Toggle Console',    group: 'View', icon: Terminal,         run: () => groveStore.toggleBottom('console') },
    { id: 'p_problems',  label: 'Toggle Problems',   group: 'View', icon: AlertTriangle,    run: () => groveStore.toggleBottom('problems') },
    { id: 'p_inspector', label: 'Toggle Inspector',  group: 'View', icon: Crosshair,        run: () => groveStore.toggleRight('inspector') },
    { id: 'p_docs',      label: 'Toggle Docs',       group: 'View', icon: BookOpen,         run: () => groveStore.toggleRight('docs') },
    { id: 'c_viz',       label: 'Toggle Arrangement', group: 'View', icon: PanelLeft,       run: () => groveStore.toggleCollapseUi() },
    { id: 'c_editor',    label: 'Toggle Editor',      group: 'View', icon: PanelRight,      run: () => groveStore.toggleCollapseTabpane() },
    { id: 'zen',         label: 'Toggle Zen mode',   group: 'View', icon: Minimize2, keys: 'Ctrl+Shift+Z', run: () => groveStore.toggleZen() },
    { id: 'find',        label: 'Search Console / Problems', group: 'View', icon: Search, keys: 'Ctrl+F', run: () => groveStore.requestFind() },
    // Window
    { id: 'settings',  label: 'Settings…',          group: 'Window', icon: Settings, keys: 'Ctrl+,', run: () => groveStore.openSettings() },
    { id: 'shortcuts', label: 'Keyboard Shortcuts', group: 'Window', icon: Keyboard, keys: 'F1',     run: () => groveStore.openShortcuts() },
  ]);

  let query = $state('');
  let selected = $state(0);
  let inputEl = $state<HTMLInputElement | null>(null);

  const filtered = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (!q) return commands;
    return commands.filter((c) =>
      c.label.toLowerCase().includes(q) || c.group.toLowerCase().includes(q),
    );
  });

  // The list only re-filters on keystrokes, so resetting the cursor there keeps
  // it in range without an effect that writes the state it reads.
  function onInput() { selected = 0; }

  $effect(() => { inputEl?.focus(); });

  function runAt(i: number) {
    const cmd = filtered[i];
    if (!cmd) return;
    onClose();        // close first so a re-opened overlay (e.g. Settings) isn't dismissed
    cmd.run();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') { e.preventDefault(); if (filtered.length) selected = (selected + 1) % filtered.length; }
    else if (e.key === 'ArrowUp') { e.preventDefault(); if (filtered.length) selected = (selected - 1 + filtered.length) % filtered.length; }
    else if (e.key === 'Enter') { e.preventDefault(); runAt(selected); }
    else if (e.key === 'Escape') { e.preventDefault(); onClose(); }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<div class="cp-backdrop" onclick={onClose} role="presentation" transition:fly={{ duration: animStore.dFast, y: 0 }}>
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div class="cp" role="presentation" onclick={(e) => e.stopPropagation()}
       transition:fly={{ y: -12, duration: animStore.dPanel, easing: cubicOut }}>
    <div class="cp-input">
      <Command size={14} />
      <input
        bind:this={inputEl}
        bind:value={query}
        oninput={onInput}
        onkeydown={onKeydown}
        placeholder="Type a command…"
        role="combobox"
        aria-expanded="true"
        aria-controls="cp-list"
        aria-activedescendant={filtered[selected] ? `cp-opt-${filtered[selected].id}` : undefined}
        spellcheck="false"
        autocomplete="off"
      />
    </div>

    <div class="cp-list" id="cp-list" role="listbox" aria-label="Commands">
      {#each filtered as cmd, i (cmd.id)}
        {@const Icon = cmd.icon}
        <button
          id="cp-opt-{cmd.id}"
          class="cp-row"
          class:active={i === selected}
          role="option"
          aria-selected={i === selected}
          onclick={() => runAt(i)}
          onmousemove={() => (selected = i)}
        >
          <span class="cp-icon"><Icon size={14} /></span>
          <span class="cp-label">{cmd.label}</span>
          <span class="cp-group">{cmd.group}</span>
          {#if cmd.keys}<span class="cp-keys">{cmd.keys}</span>{/if}
        </button>
      {/each}
      {#if filtered.length === 0}
        <div class="cp-empty">No matching command</div>
      {/if}
    </div>
  </div>
</div>

<style>
  .cp-backdrop {
    position: fixed; inset: 0;
    background: rgba(0, 0, 0, 0.45);
    z-index: var(--z-modal-bg);
    display: flex; align-items: flex-start; justify-content: center;
    padding-top: 12vh;
  }
  .cp {
    width: 560px; max-width: 92vw;
    max-height: 60vh;
    display: flex; flex-direction: column;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5);
    overflow: hidden;
  }

  .cp-input {
    display: flex; align-items: center; gap: 9px;
    padding: 11px 14px;
    border-bottom: 1px solid var(--border-subtle);
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .cp-input input {
    flex: 1; min-width: 0;
    background: transparent; border: none; outline: none;
    color: var(--text-primary);
    font-family: var(--font-ui-sans); font-size: 14px;
  }
  .cp-input input::placeholder { color: var(--text-disabled); }

  .cp-list { overflow-y: auto; padding: 5px; display: flex; flex-direction: column; gap: 1px; }
  .cp-row {
    display: flex; align-items: center; gap: 10px;
    width: 100%; padding: 7px 10px;
    background: transparent; border: none; border-radius: var(--radius-md);
    cursor: pointer; text-align: left;
    color: var(--text-secondary);
  }
  .cp-row.active { background: var(--accent-subtle, color-mix(in srgb, var(--accent) 14%, transparent)); color: var(--text-primary); }
  .cp-icon { display: flex; color: var(--text-muted); flex-shrink: 0; }
  .cp-row.active .cp-icon { color: var(--accent); }
  .cp-label { flex: 1; min-width: 0; font-size: 13px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .cp-group {
    font-size: 9.5px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.5px;
    color: var(--text-disabled); flex-shrink: 0;
  }
  .cp-keys {
    font-family: var(--font-code); font-size: 10px;
    color: var(--text-muted); flex-shrink: 0;
    padding: 1px 6px; background: var(--bg-overlay);
    border: 1px solid var(--border-subtle); border-radius: var(--radius-sm);
  }
  .cp-empty { padding: 18px; text-align: center; color: var(--text-muted); font-size: 12px; }
</style>
