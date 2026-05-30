<script lang="ts">
  import { HardDriveDownload, GitMerge, AlertTriangle } from 'lucide-svelte';
  import { nodeX, ROW_HEIGHT } from '$lib/utils/graph-renderer';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { GraphColumn } from '$lib/types/config';
  import type { RepoStatus } from '$lib/types/git';

  let {
    gridTemplate,
    visibleCols,
    graphTrackWidth,
    wipCounts,
    status,
    active,
    onclick,
    oncontextmenu,
  }: {
    /** CSS grid-template-columns string shared with the sticky header and
     *  the commit rows — so the dashed WIP node sits in the SAME track as
     *  every commit lane below it, and the "Working Directory" label lines
     *  up with the Subject column wherever the user has put it. */
    gridTemplate: string;
    /** Visible columns in render order. Drives which cells render content
     *  vs. blank placeholders. */
    visibleCols: GraphColumn[];
    /** Effective width of the graph track (adaptive cap). Used to size
     *  the small inline SVG that draws the dashed circle. */
    graphTrackWidth: number;
    wipCounts: { modified: number; added: number; deleted: number; total: number } | null;
    status: RepoStatus | null;
    active: boolean;
    onclick: () => void;
    oncontextmenu?: (e: MouseEvent) => void;
  } = $props();

  const isMerging     = $derived(status?.is_merging ?? false);
  const conflictCount = $derived(status?.conflicted.length ?? 0);
</script>

<div
  class="wip-row"
  class:wip-active={active}
  class:wip-merging={isMerging && conflictCount > 0}
  style="grid-template-columns: {gridTemplate}; height: {ROW_HEIGHT}px;"
  role="button"
  tabindex="0"
  {onclick}
  onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onclick(); } }}
  oncontextmenu={oncontextmenu}
  use:tooltip={isMerging && conflictCount > 0
    ? { content: 'Merge in corso', description: `${conflictCount} file in conflitto` }
    : 'View working directory changes'}
>
  {#each visibleCols as col (col.id)}
    {#if col.id === 'graph'}
      <div class="cell cell-graph">
        <svg width={graphTrackWidth} height={ROW_HEIGHT}>
          <circle
            cx={nodeX(0)} cy={ROW_HEIGHT / 2} r="5"
            fill="none"
            stroke={isMerging && conflictCount > 0 ? 'var(--warning)' : 'var(--accent)'}
            stroke-width="1.5"
            stroke-dasharray="3 2"
          />
          <line
            x1={nodeX(0)} y1={ROW_HEIGHT / 2 + 5}
            x2={nodeX(0)} y2={ROW_HEIGHT}
            stroke={isMerging && conflictCount > 0 ? 'var(--warning)' : 'var(--accent)'}
            stroke-width="1.5"
            stroke-dasharray="3 2"
            opacity="0.5"
          />
        </svg>
      </div>
    {:else if col.id === 'subject'}
      <div class="cell wip-info">
        {#if isMerging && conflictCount > 0}
          <AlertTriangle size={11} class="wip-icon-conflict" />
          <span class="wip-label wip-label-conflict">Merge in corso</span>
          <span class="wip-pill wip-conflict" use:tooltip={`${conflictCount} file in conflitto`}>
            {conflictCount} conflitt{conflictCount === 1 ? 'o' : 'i'}
          </span>
          <button
            class="wip-resolve-btn"
            onclick={(e) => { e.stopPropagation(); uiStore.openMergeModal(); }}
            use:tooltip={'Apri risoluzione conflitti'}
          >
            <GitMerge size={10} /> Risolvi
          </button>
        {:else}
          <HardDriveDownload size={11} class="wip-icon" />
          <span class="wip-label">Working Directory</span>
          {#if isMerging}
            <span class="wip-merge-badge">MERGE</span>
          {/if}
          {#if wipCounts}
            {#if wipCounts.modified > 0}
              <span class="wip-pill wip-modified" use:tooltip={`${wipCounts.modified} modified`}>{wipCounts.modified}M</span>
            {/if}
            {#if wipCounts.added > 0}
              <span class="wip-pill wip-added" use:tooltip={`${wipCounts.added} added`}>{wipCounts.added}A</span>
            {/if}
            {#if wipCounts.deleted > 0}
              <span class="wip-pill wip-deleted" use:tooltip={`${wipCounts.deleted} deleted`}>{wipCounts.deleted}D</span>
            {/if}
          {/if}
          {#if (status?.staged.length ?? 0) > 0}
            <span class="wip-staged">{status!.staged.length} staged</span>
          {/if}
        {/if}
      </div>
    {:else}
      <div class="cell" aria-hidden="true"></div>
    {/if}
  {/each}
</div>

<style>
  /* WIP row mirrors the commit-row grid layout so its cells line up with
     the column headers (and the lane SVG behind it lines up with every
     commit lane below). The row sits sticky under the column header
     courtesy of `top:` set in CommitGraph.svelte — that way it's always
     visible while scrolling through history. */
  .wip-row {
    display: grid;
    align-items: center;
    cursor: pointer;
    /* OPAQUE bg so the row keeps its content readable when it's stuck
       below the header and commits scroll through behind it. Hover /
       active / merging states below override this normally. */
    background: var(--bg-base);
    border: none;
    border-bottom: 1px solid var(--border-subtle);
    transition: background var(--transition-fast);
    flex-shrink: 0;
  }
  /* States stay OPAQUE — the row is sticky-positioned, so any translucent
     fill would let scrolling commits bleed through behind it. `color-mix`
     reproduces the look of the previous `rgba(…)` accents pre-blended
     with the row's base bg. */
  .wip-row:hover         { background: var(--bg-hover); }
  .wip-row.wip-active    { background: color-mix(in srgb, var(--accent)  10%, var(--bg-base)); }
  .wip-row.wip-merging   { background: color-mix(in srgb, var(--warning)  6%, var(--bg-base));
                            border-bottom: 1px solid rgba(226,163,53,0.25); }

  .cell {
    display: flex;
    align-items: center;
    min-width: 0;
    overflow: hidden;
  }

  /* The graph cell hosts the dashed-circle SVG. No padding so the lane-0
     position lines up exactly with the same lane in commit rows below. */
  .cell-graph { padding: 0; }

  /* `wip-info` lives in the Subject column track. */
  .wip-info {
    gap: 6px;
    padding: 0 12px 0 6px;
  }

  :global(.wip-icon)          { color: var(--accent);  flex-shrink: 0; }
  :global(.wip-icon-conflict) { color: var(--warning); flex-shrink: 0; }

  .wip-label {
    font-family: var(--font-ui-sans);
    font-size: var(--font-size-sm);
    color: var(--text-primary);
    font-style: italic;
    white-space: nowrap;
  }
  .wip-label-conflict {
    font-style: normal;
    font-weight: 600;
    color: var(--warning);
  }
  .wip-conflict {
    color: var(--warning);
    background: rgba(226, 163, 53, 0.12);
    border: 1px solid rgba(226, 163, 53, 0.3);
  }
  .wip-merge-badge {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.05em;
    color: var(--accent);
    background: rgba(77, 120, 204, 0.12);
    border: 1px solid rgba(77, 120, 204, 0.3);
    border-radius: var(--radius-sm);
    padding: 0 4px;
    white-space: nowrap;
    flex-shrink: 0;
  }
  .wip-resolve-btn {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 2px 8px;
    border-radius: var(--radius-sm);
    font-size: 11px;
    font-family: var(--font-ui-sans);
    font-weight: 500;
    cursor: pointer;
    background: rgba(226, 163, 53, 0.15);
    border: 1px solid rgba(226, 163, 53, 0.4);
    color: var(--warning);
    margin-left: 2px;
    transition: background var(--transition-fast);
    flex-shrink: 0;
  }
  .wip-resolve-btn:hover { background: rgba(226, 163, 53, 0.28); }

  .wip-pill {
    font-size: 10px;
    font-weight: 600;
    border-radius: var(--radius-sm);
    padding: 0 4px;
    white-space: nowrap;
    flex-shrink: 0;
    letter-spacing: 0.2px;
  }
  .wip-modified { color: var(--warning); background: rgba(226,163,53,0.12); border: 1px solid rgba(226,163,53,0.25); }
  .wip-added    { color: var(--success); background: rgba(95,173,86,0.12);  border: 1px solid rgba(95,173,86,0.25); }
  .wip-deleted  { color: var(--error);   background: rgba(199,84,80,0.12);  border: 1px solid rgba(199,84,80,0.25); }

  .wip-staged {
    font-size: 10px;
    color: var(--success);
    background: var(--success-subtle);
    border: 1px solid rgba(95,173,86,0.3);
    border-radius: 999px;
    padding: 0 6px;
    white-space: nowrap;
    flex-shrink: 0;
  }
</style>
