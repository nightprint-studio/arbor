<!--
  ConflictFileSidebar — left-hand file list for the conflict modal.

  Renders one of two file lists (regular conflicts or stash-blocking files)
  with a flat / tree view toggle, status icons, context-menu wiring, and an
  optional "Next conflict" jump button at the bottom.

  The variation between the two modes is small — sidebar label, icon scheme,
  badge layout — so the consumer passes a `mode` plus an `items` array of
  row records the widget can render uniformly. Keeping a single rendering
  path here means future tweaks (a11y attr, hover treatment, drag-reorder
  affordance) land in one place.

  Display state (flat vs tree, expanded dirs) is *persistent* across modal
  re-opens but session-scoped (no value, just UX preference), so we keep it
  in `localStorage` per the CLAUDE.md exception for "session UI state".
  Migrating to typed config would be over-engineering — this isn't a setting
  the user reaches for through Settings.
-->
<script lang="ts">
  import {
    AlertTriangle, CheckCircle2, Eye,
    ChevronRight, ChevronDown, Folder, FolderTree, List,
  } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import {
    buildConflictTree, flattenConflictTree, toggleTreeDir,
  } from '$lib/utils/conflict/conflict-file-tree';
  import type { FileItem, Status } from './types';

  interface Props {
    /** Drives the sidebar header label only. */
    label:        string;
    items:        FileItem[];
    selectedPath: string | null;
    onSelect:     (path: string) => void;
    onContextMenu: (e: MouseEvent, path: string) => void;

    /** Optional "next unresolved" jump button at the bottom. */
    showNextButton?: boolean;
    nextDisabled?:   boolean;
    onNext?:         () => void;
  }

  let {
    label, items, selectedPath, onSelect, onContextMenu,
    showNextButton = false, nextDisabled = false, onNext,
  }: Props = $props();

  // ── Persistent UI prefs ───────────────────────────────────────────────────
  let viewMode = $state<'list' | 'tree'>(
    (localStorage.getItem('arbor:conflict-files-view-mode') as 'list' | 'tree') ?? 'list',
  );
  $effect(() => {
    localStorage.setItem('arbor:conflict-files-view-mode', viewMode);
  });
  let expanded = $state<Set<string>>(new Set());

  const tree = $derived(buildConflictTree(items.map(i => ({ path: i.path }))));
  const rows = $derived(flattenConflictTree(tree, expanded));
  const byPath = $derived(new Map(items.map(it => [it.path, it])));

  function statusIcon(s: Status) {
    if (s === 'resolved') return { Icon: CheckCircle2, cls: 'icon-resolved' };
    if (s === 'viewed')   return { Icon: Eye,         cls: 'icon-viewed'   };
    return                       { Icon: AlertTriangle, cls: 'icon-conflict' };
  }
</script>

<div class="file-sidebar">
  <div class="sidebar-label-row">
    <span class="sidebar-label">{label}</span>
    <button
      class="sidebar-toggle-btn"
      class:active={viewMode === 'list'}
      use:tooltip={'Show file names'}
      onclick={() => viewMode = 'list'}
      aria-label="List view"
    >
      <List size={12} />
    </button>
    <button
      class="sidebar-toggle-btn"
      class:active={viewMode === 'tree'}
      use:tooltip={'Show tree structure'}
      onclick={() => viewMode = 'tree'}
      aria-label="Tree view"
    >
      <FolderTree size={12} />
    </button>
  </div>
  <div class="sidebar-divider"></div>
  <div class="files-list" class:tree-mode={viewMode === 'tree'}>
    {#if viewMode === 'list'}
      {#each items as it}
        {@const isActive = selectedPath === it.path}
        {@const resolved = it.status === 'resolved' || it.status === 'viewed'}
        {@const ico = statusIcon(it.status)}
        <button
          class="file-item"
          class:active={isActive}
          class:resolved
          onclick={() => onSelect(it.path)}
          oncontextmenu={(e) => onContextMenu(e, it.path)}
          use:tooltip={it.path}
        >
          {#if it.monogram !== undefined}
            <span
              class="status-badge"
              class:s-resolved={it.status === 'resolved'}
              class:s-added={it.status === 'added'}
              class:s-deleted={it.status === 'deleted'}
              class:s-conflict={it.status === 'conflict'}
              use:tooltip={it.monoTip ?? ''}
            >{it.monogram}</span>
          {:else}
            <span class="file-status-icon">
              <ico.Icon size={12} class={ico.cls} />
            </span>
          {/if}
          <span class="file-name truncate">{it.path.split('/').pop()}</span>
          {#if it.decisionBadge}
            <span
              class="blocking-decision-badge"
              class:keep={it.decisionBadge.kind === 'keep_mine'}
              class:stash={it.decisionBadge.kind === 'use_stash'}
              class:custom={it.decisionBadge.kind === 'custom'}
              use:tooltip={it.decisionBadge.tooltip}
            >{it.decisionBadge.kind === 'keep_mine' ? 'M' : it.decisionBadge.kind === 'use_stash' ? 'S' : '✎'}</span>
          {/if}
          {#if isActive && it.saving}<span class="file-saving">…</span>{/if}
        </button>
      {/each}
    {:else}
      {#each rows as row}
        {#if row.kind === 'dir'}
          <button
            class="tree-dir"
            style="padding-left: {6 + row.depth * 12}px"
            onclick={() => expanded = toggleTreeDir(expanded, row.fullPath)}
            use:tooltip={row.fullPath}
          >
            {#if expanded.has(row.fullPath)}
              <ChevronDown size={11} class="tree-chev" />
            {:else}
              <ChevronRight size={11} class="tree-chev" />
            {/if}
            <Folder size={12} class="tree-folder-icon" />
            <span class="tree-dir-name truncate">{row.name}</span>
          </button>
        {:else}
          {@const it = byPath.get(row.path)}
          {#if it}
            {@const isActive = selectedPath === it.path}
            {@const resolved = it.status === 'resolved' || it.status === 'viewed'}
            {@const ico = statusIcon(it.status)}
            <button
              class="file-item tree-file"
              class:active={isActive}
              class:resolved
              style="padding-left: {6 + row.depth * 12}px"
              onclick={() => onSelect(it.path)}
              oncontextmenu={(e) => onContextMenu(e, it.path)}
              use:tooltip={it.path}
            >
              {#if it.monogram !== undefined}
                <span
                  class="status-badge"
                  class:s-resolved={it.status === 'resolved'}
                  class:s-added={it.status === 'added'}
                  class:s-deleted={it.status === 'deleted'}
                  class:s-conflict={it.status === 'conflict'}
                  use:tooltip={it.monoTip ?? ''}
                >{it.monogram}</span>
              {:else}
                <span class="file-status-icon">
                  <ico.Icon size={12} class={ico.cls} />
                </span>
              {/if}
              <span class="file-name truncate">{row.name}</span>
              {#if it.decisionBadge}
                <span
                  class="blocking-decision-badge"
                  class:keep={it.decisionBadge.kind === 'keep_mine'}
                  class:stash={it.decisionBadge.kind === 'use_stash'}
                  class:custom={it.decisionBadge.kind === 'custom'}
                  use:tooltip={it.decisionBadge.tooltip}
                >{it.decisionBadge.kind === 'keep_mine' ? 'M' : it.decisionBadge.kind === 'use_stash' ? 'S' : '✎'}</span>
              {/if}
              {#if isActive && it.saving}<span class="file-saving">…</span>{/if}
            </button>
          {/if}
        {/if}
      {/each}
    {/if}
    {#if showNextButton}
      <button class="next-btn" onclick={onNext} disabled={nextDisabled}>
        <ChevronRight size={12} /> Next conflict
      </button>
    {/if}
  </div>
</div>

<style>
  .file-sidebar {
    flex: 0 0 220px;
    margin-right: 4px;
    background: var(--bg-base);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    overflow: hidden;
    min-width: 0;
    height: 100%;
  }

  .sidebar-label-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 8px 6px 12px;
  }
  .sidebar-label {
    flex: 1;
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
  }
  .sidebar-toggle-btn {
    display: flex; align-items: center; justify-content: center;
    width: 20px; height: 20px;
    padding: 0;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--text-muted);
    cursor: pointer;
    flex-shrink: 0;
    transition: background var(--transition-fast), color var(--transition-fast),
                border-color var(--transition-fast);
  }
  .sidebar-toggle-btn:hover { background: var(--bg-hover); color: var(--text-primary); }
  .sidebar-toggle-btn.active {
    background: var(--accent-subtle);
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .sidebar-divider { height: 1px; background: var(--border-subtle); margin: 0; flex-shrink: 0; }

  .files-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 8px;
    overflow-y: auto;
    flex: 1;
  }
  .files-list.tree-mode { gap: 0; padding: 4px 4px; }
  .files-list.tree-mode .file-item {
    padding: 3px 8px;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    box-shadow: none;
  }
  .files-list.tree-mode .file-item:hover {
    background: var(--bg-hover);
    border-color: transparent;
    box-shadow: none;
  }
  /* Active highlight uses theme-neutral accent so the same widget works
     for both merge (blue) and stash (amber) — the mode-specific tint is
     applied by the consuming wrapper via the parent `.mode-*` class. */
  :global(.mode-merge) .files-list.tree-mode .file-item.active {
    background: rgba(77,120,204,.14);
    border-color: rgba(77,120,204,.4);
  }
  :global(.mode-stash) .files-list.tree-mode .file-item.active {
    background: rgba(226,163,53,.12);
    border-color: rgba(226,163,53,.4);
  }

  .tree-dir {
    display: flex; align-items: center; gap: 4px;
    padding: 3px 8px;
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
    font-size: var(--font-size-sm);
    font-family: var(--font-ui-sans);
    cursor: pointer;
    width: 100%;
    text-align: left;
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .tree-dir:hover { background: var(--bg-hover); color: var(--text-primary); }
  :global(.tree-chev)        { color: var(--text-muted); flex-shrink: 0; }
  :global(.tree-folder-icon) { color: var(--text-muted); flex-shrink: 0; }
  .tree-dir-name {
    flex: 1; min-width: 0;
    font-size: var(--font-size-sm);
    font-family: var(--font-ui-sans);
  }

  .file-item {
    display: flex; align-items: center; gap: 7px;
    padding: 7px 10px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    cursor: pointer; text-align: left; width: 100%;
    color: inherit;
    transition: background var(--transition-fast), border-color var(--transition-fast),
                box-shadow var(--transition-fast);
  }
  .file-item:hover {
    background: var(--bg-overlay);
    border-color: var(--border);
    box-shadow: 0 1px 4px rgba(0,0,0,0.15);
  }
  :global(.mode-merge) .file-item.active {
    background: rgba(77,120,204,.14);
    border-color: rgba(77,120,204,.55);
  }
  :global(.mode-stash) .file-item.active {
    background: rgba(226,163,53,.12);
    border-color: rgba(226,163,53,.55);
  }
  .file-item.resolved .file-name { color: var(--text-muted); }

  .file-status-icon { display: flex; flex-shrink: 0; }
  :global(.icon-conflict) { color: var(--warning); }
  :global(.icon-resolved) { color: var(--success); }
  :global(.icon-viewed)   { color: var(--accent); }

  .status-badge {
    flex-shrink: 0;
    width: 16px;
    height: 16px;
    border-radius: var(--radius-sm);
    font-size: 10px;
    font-weight: 700;
    line-height: 16px;
    text-align: center;
    background: var(--bg-overlay);
    color: var(--text-muted);
    letter-spacing: 0;
    font-family: var(--font-code);
  }
  .status-badge.s-added    { background: color-mix(in srgb, var(--color-file-added) 22%, transparent);    color: var(--color-file-added); }
  .status-badge.s-deleted  { background: color-mix(in srgb, var(--color-file-deleted) 22%, transparent);  color: var(--color-file-deleted); }
  .status-badge.s-conflict { background: color-mix(in srgb, var(--warning) 22%, transparent);             color: var(--warning); }
  .status-badge.s-resolved { background: color-mix(in srgb, var(--success) 22%, transparent);             color: var(--success); }

  .file-name {
    font-size: var(--font-size-sm); color: var(--text-primary);
    font-family: var(--font-ui-sans); min-width: 0; flex: 1;
  }
  .file-saving { font-size: 10px; color: var(--text-muted); }

  .blocking-decision-badge {
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 14px;
    height: 14px;
    padding: 0 4px;
    font-size: 9px;
    font-weight: 700;
    font-family: var(--font-code);
    border-radius: var(--radius-sm);
    line-height: 1;
  }
  .blocking-decision-badge.keep {
    background: color-mix(in srgb, var(--accent) 24%, transparent);
    color: var(--accent);
    border: 1px solid color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .blocking-decision-badge.stash {
    background: color-mix(in srgb, var(--warning) 24%, transparent);
    color: var(--warning);
    border: 1px solid color-mix(in srgb, var(--warning) 45%, transparent);
  }
  .blocking-decision-badge.custom {
    background: color-mix(in srgb, var(--success) 24%, transparent);
    color: var(--success);
    border: 1px solid color-mix(in srgb, var(--success) 45%, transparent);
  }

  .next-btn {
    display: flex; align-items: center; gap: 4px;
    padding: 6px 12px; background: none; border: none; cursor: pointer;
    font-size: 11px; font-family: var(--font-ui-sans); width: 100%;
    transition: background var(--transition-fast);
  }
  :global(.mode-merge) .next-btn { color: var(--accent); }
  :global(.mode-stash) .next-btn { color: var(--warning); }
  .next-btn:hover:not(:disabled) { background: var(--bg-hover); }
  .next-btn:disabled { color: var(--text-disabled); cursor: default; }

  .truncate { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
</style>
