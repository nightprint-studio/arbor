<script lang="ts">
  /**
   * The tools Imber used on the way, folded into one line that opens on demand — the receipts matter less
   * than the answer. `summary` is the product's one-breath wording. The rows are `calls`, or `items` when the
   * product also records the model's answers between them; when times were recorded, the line ends with how
   * long the stretch took.
   */
  import { ChevronRight } from 'lucide-svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import ImberToolRow from './ImberToolRow.svelte';
  import ImberStepRow from './ImberStepRow.svelte';
  import { formatDuration, splitTime } from './imber-format';
  import './imber-motion.css';
  import type { ToolCall, WorkItem } from './imber-types';

  let { summary, calls = [], items }: { summary: string; calls?: ToolCall[]; items?: WorkItem[] } = $props();

  const rows = $derived<WorkItem[]>(items ?? calls.map((call) => ({ kind: 'call', call })));
  const split = $derived(splitTime(rows));
  const total = $derived(split.model + split.tools + split.waiting);
  const totalTip = $derived(
    [
      split.model ? `model ${formatDuration(split.model)}` : '',
      split.tools ? `tools ${formatDuration(split.tools)}` : '',
      split.waiting ? `waiting for you ${formatDuration(split.waiting)}` : '',
    ].filter(Boolean).join(' · '),
  );
</script>

<div class="tools asst-enter">
  <Collapsible>
    {#snippet header({ open })}
      <div class="tools-head" class:open>
        <ChevronRight size={11} class="chev" />
        <span>{summary || 'Working'}</span>
        {#if split.known}<span class="total" title={totalTip}>· {formatDuration(total)}</span>{/if}
      </div>
    {/snippet}
    <div class="tools-list">
      {#each rows as row, i (row.kind === 'call' ? row.call.id : `step-${i}`)}
        {#if row.kind === 'call'}<ImberToolRow call={row.call} />{:else}<ImberStepRow step={row.step} />{/if}
      {/each}
    </div>
  </Collapsible>
</div>

<style>
  .tools-head {
    display: inline-flex; align-items: center; gap: 6px;
    padding: 2px 6px 2px 2px; border-radius: var(--radius-sm);
    font-size: var(--font-size-xs); color: var(--text-muted);
    transition: color var(--transition-fast), background var(--transition-fast);
  }
  .tools-head:hover { color: var(--text-secondary); background: var(--bg-hover); }
  .tools-head :global(.chev) { transition: transform var(--transition-fast); }
  .tools-head.open :global(.chev) { transform: rotate(90deg); }
  .total { font-variant-numeric: tabular-nums; }
  .tools-list { display: flex; flex-direction: column; gap: 3px; padding: 5px 0 2px; }
</style>
