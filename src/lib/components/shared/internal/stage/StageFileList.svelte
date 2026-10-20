<script lang="ts">
  /**
   * One accordion section of the staging panel — "Unstaged" or "Staged": a header (chevron,
   * title, count, view toggle, caller's actions) over the changed files as a flat list or a
   * folder tree. Presentational: it reports what the user did and owns no git state.
   */
  import type { Snippet } from 'svelte';
  import { ChevronDown, ChevronRight, FolderTree, List, Folder, Plus, RotateCcw, X } from 'lucide-svelte';
  import Tree from '$lib/components/shared/ui/Tree.svelte';
  import IconButton from '$lib/components/shared/ui/IconButton.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { basename, pathLabel, type StageTreeNode } from '$lib/utils/stage/stage-tree';
  import type { StatusEntry } from '$lib/types/corvus/git';

  interface Props {
    kind: 'unstaged' | 'staged';
    entries: StatusEntry[];
    tree: StageTreeNode;
    viewMode: 'list' | 'tree';
    collapsed: boolean;
    expandedPaths: Set<string>;
    selectedPath: string | null;
    onToggleSection: () => void;
    onToggleView: () => void;
    onToggleDir: (path: string) => void;
    onOpen: (entry: StatusEntry) => void;
    onAct: (entry: StatusEntry) => void;
    onDiscard?: (entry: StatusEntry) => void;
    onContext: (e: MouseEvent, entry: StatusEntry | null, node: StageTreeNode | null) => void;
    /** Buttons at the right of the header (stage all, discard all, stash…). */
    actions?: Snippet;
  }

  let {
    kind, entries, tree, viewMode, collapsed, expandedPaths, selectedPath,
    onToggleSection, onToggleView, onToggleDir, onOpen, onAct, onDiscard, onContext, actions,
  }: Props = $props();

  const staged = $derived(kind === 'staged');
  const title = $derived(staged ? 'Staged' : 'Unstaged');

  const statusOf = (e: StatusEntry) => (staged ? e.index_status : e.workdir_status);
  function letter(e: StatusEntry): string {
    const s = statusOf(e);
    if (s === 'added' || s === 'untracked') return 'A';
    if (s === 'modified') return 'M';
    if (s === 'deleted') return 'D';
    if (s === 'renamed') return 'R';
    return staged ? 'M' : '?';
  }
</script>

{#snippet fileRow(entry: StatusEntry, name: string)}
  {@const s = statusOf(entry)}
  <span
    class="status-badge"
    class:s-added={s === 'added' || s === 'untracked'}
    class:s-modified={s === 'modified'}
    class:s-deleted={s === 'deleted'}
    class:s-renamed={s === 'renamed'}
  >{letter(entry)}</span>
  <span class="filename" use:tooltip={pathLabel(entry)}>{name}</span>
  <div class="file-actions">
    {#if staged}
      <IconButton size={20} variant="danger" tooltip="Unstage file" onclick={(e) => { e.stopPropagation(); onAct(entry); }}><X size={11} /></IconButton>
    {:else}
      <IconButton size={20} variant="accent" tooltip="Stage file" onclick={(e) => { e.stopPropagation(); onAct(entry); }}><Plus size={12} /></IconButton>
      {#if onDiscard}
        <IconButton size={20} tooltip="Discard changes" onclick={(e) => { e.stopPropagation(); onDiscard(entry); }}><RotateCcw size={11} /></IconButton>
      {/if}
    {/if}
  </div>
{/snippet}

<div class="section" class:collapsed class:has-staged={staged && entries.length > 0}>
  <div
    class="section-header"
    role="button"
    tabindex="0"
    aria-expanded={!collapsed}
    onclick={onToggleSection}
    onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onToggleSection(); } }}
  >
    <span class="chevron">{#if collapsed}<ChevronRight size={12} />{:else}<ChevronDown size={12} />{/if}</span>
    <span>{title}</span>
    <span class="count" class:staged-count={staged} class:nonzero={entries.length > 0}>{entries.length}</span>

    <div class="header-right">
      <IconButton
        size={20}
        active={viewMode === 'tree'}
        tooltip={viewMode === 'tree' ? 'Switch to flat list' : 'Group files by folder'}
        onclick={(e) => { e.stopPropagation(); onToggleView(); }}
      >
        {#if viewMode === 'tree'}<List size={11} />{:else}<FolderTree size={11} />{/if}
      </IconButton>
      {#if actions}
        <span class="header-sep"></span>
        <!-- Clicks on these must not toggle the section. -->
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <span class="header-actions" onclick={(e) => e.stopPropagation()}>{@render actions()}</span>
      {/if}
    </div>
  </div>

  <div class="file-list">
    {#if viewMode === 'list'}
      {#each entries as entry (entry.path)}
        <div
          class="file-entry"
          class:selected={selectedPath === entry.path}
          onclick={() => onOpen(entry)}
          oncontextmenu={(e) => onContext(e, entry, null)}
          role="row"
          tabindex="0"
          onkeydown={(e) => e.key === 'Enter' && onOpen(entry)}
        >
          {@render fileRow(entry, basename(entry.path))}
        </div>
      {/each}
    {:else}
      <Tree
        nodes={tree.sortedChildren}
        getId={(n: StageTreeNode) => n.fullPath}
        getChildren={(n: StageTreeNode) => n.sortedChildren}
        expandedIds={expandedPaths}
        onExpandToggle={onToggleDir}
        selectedId={selectedPath}
        selectable={(n: StageTreeNode) => !!n.entry}
        indentSize={12}
        basePadding={8}
        ariaLabel={`${title} files`}
        onSelect={(n: StageTreeNode) => { if (n.entry) onOpen(n.entry); }}
        onContextMenu={(n: StageTreeNode, e: MouseEvent) => onContext(e, n.entry ?? null, n.entry ? null : n)}
      >
        {#snippet row({ node }: { node: StageTreeNode })}
          {#if node.entry}
            {@render fileRow(node.entry, node.name)}
          {:else}
            <Folder size={11} class="stage-folder-icon" />
            <span class="dir-name">{node.name}</span>
          {/if}
        {/snippet}
      </Tree>
    {/if}
  </div>
</div>

<style>
  .section {
    display: flex;
    flex-direction: column;
    flex: 1 1 0;
    min-height: 0;
    overflow: hidden;
    transition: flex var(--transition-fast);
  }
  .section.collapsed { flex: 0 0 auto; }
  .section.collapsed .file-list { display: none; }

  .section-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px 5px 6px;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
    border-bottom: 1px solid var(--border-subtle);
    flex-shrink: 0;
    cursor: pointer;
    user-select: none;
  }
  .section-header:hover { background: var(--bg-hover); }
  /* Staged section: a green edge when there is something staged. */
  .section.has-staged > .section-header { border-left: 2px solid var(--success); padding-left: 4px; }

  .chevron { display: flex; align-items: center; color: var(--text-disabled); flex-shrink: 0; }

  .count {
    background: var(--bg-overlay);
    padding: 0 5px;
    border-radius: 999px;
    font-size: var(--font-size-2xs);
    color: var(--text-disabled);
    transition: background var(--transition-fast), color var(--transition-fast);
  }
  .count.nonzero { background: var(--info-subtle); color: var(--info); }
  .count.staged-count.nonzero { background: var(--success-subtle); color: var(--success); }

  .header-right { margin-left: auto; display: flex; align-items: center; gap: 2px; }
  .header-actions { display: flex; align-items: center; gap: 2px; }
  .header-sep { width: 1px; height: 12px; background: var(--border-subtle); margin: 0 3px; flex-shrink: 0; }

  .file-list { flex: 1; overflow-y: auto; padding: 2px; }

  .file-entry {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 3px 6px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: var(--font-size-xs);
    transition: background var(--transition-fast);
  }
  .file-entry:hover { background: var(--bg-hover); }
  .file-entry.selected { background: var(--bg-selected, rgba(77, 120, 204, 0.15)); }

  /* The row snippet renders inside shared <Tree>'s scope, hence :global on the folder glyph. */
  :global(.stage-folder-icon) { color: var(--warning); flex-shrink: 0; }
  .dir-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 500; }

  .status-badge {
    flex-shrink: 0;
    width: 16px;
    height: 16px;
    border-radius: var(--radius-sm);
    font-size: var(--font-size-2xs);
    font-weight: 700;
    line-height: 16px;
    text-align: center;
    background: var(--bg-overlay);
    color: var(--text-muted);
  }
  .status-badge.s-added    { background: color-mix(in srgb, var(--color-file-added) 22%, transparent);    color: var(--color-file-added); }
  .status-badge.s-modified { background: color-mix(in srgb, var(--color-file-modified) 22%, transparent); color: var(--color-file-modified); }
  .status-badge.s-deleted  { background: color-mix(in srgb, var(--color-file-deleted) 22%, transparent);  color: var(--color-file-deleted); }
  .status-badge.s-renamed  { background: color-mix(in srgb, var(--color-file-renamed) 22%, transparent);  color: var(--color-file-renamed); }

  .filename { flex: 1; color: var(--text-primary); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .file-actions { display: flex; align-items: center; gap: 2px; flex-shrink: 0; }
</style>
