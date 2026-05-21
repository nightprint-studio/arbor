<script lang="ts">
  import { HardDriveDownload, GitMerge, AlertTriangle } from 'lucide-svelte';
  import { nodeX, ROW_HEIGHT } from '$lib/utils/graph-renderer';
  import { uiStore } from '$lib/stores/ui.svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import type { RepoStatus } from '$lib/types/git';

  let {
    svgW,
    wipCounts,
    status,
    active,
    onclick,
    oncontextmenu,
  }: {
    svgW: number;
    wipCounts: { modified: number; added: number; deleted: number; total: number } | null;
    status: RepoStatus | null;
    active: boolean;
    onclick: () => void;
    oncontextmenu?: (e: MouseEvent) => void;
  } = $props();

  const isMerging   = $derived(status?.is_merging ?? false);
  const conflictCount = $derived(status?.conflicted.length ?? 0);
</script>

<div
  class="wip-row"
  class:wip-active={active}
  class:wip-merging={isMerging && conflictCount > 0}
  role="button"
  tabindex="0"
  {onclick}
  onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); onclick(); } }}
  oncontextmenu={oncontextmenu}
  use:tooltip={isMerging && conflictCount > 0
    ? { content: 'Merge in corso', description: `${conflictCount} file in conflitto` }
    : 'View working directory changes'}
>
  <div class="wip-graph-col" style="width: {svgW}px; min-width: {svgW}px">
    <svg width={svgW} height={ROW_HEIGHT}>
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
  <div class="wip-info">
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
</div>

<style>
  .wip-row {
    display: flex;
    align-items: center;
    height: 28px;
    padding: 0.3em 0 0 8px;
    background: transparent;
    border: none;
    border-bottom: 1px solid var(--border-subtle);
    cursor: pointer;
    width: 100%;
    text-align: left;
    transition: background var(--transition-fast);
    flex-shrink: 0;
  }
  .wip-row:hover    { background: var(--bg-hover); }
  .wip-row.wip-active { background: rgba(77,120,204,0.10); }
  .wip-row.wip-merging { background: rgba(226,163,53,0.06); border-bottom: 1px solid rgba(226,163,53,0.25); }

  .wip-graph-col {
    flex-shrink: 0;
    display: flex;
    align-items: center;
  }

  .wip-info {
    flex: 1;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 0 12px 0 6px;
    min-width: 0;
    overflow: hidden;
  }

  :global(.wip-icon) { color: var(--accent); flex-shrink: 0; }
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
