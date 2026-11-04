<script lang="ts">
  /**
   * The tools Imber used on the way, folded into one line that opens on demand — the receipts matter less
   * than the answer. `summary` is the product's one-breath wording; `calls` are the rows inside.
   */
  import { ChevronRight } from 'lucide-svelte';
  import Collapsible from '$lib/components/shared/ui/Collapsible.svelte';
  import ImberToolRow from './ImberToolRow.svelte';
  import './imber-motion.css';
  import type { ToolCall } from './imber-types';

  let { summary, calls }: { summary: string; calls: ToolCall[] } = $props();
</script>

<div class="tools asst-enter">
  <Collapsible>
    {#snippet header({ open })}
      <div class="tools-head" class:open>
        <ChevronRight size={11} class="chev" />
        <span>{summary || 'Working'}</span>
      </div>
    {/snippet}
    <div class="tools-list">
      {#each calls as call (call.id)}<ImberToolRow {call} />{/each}
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
  .tools-list { display: flex; flex-direction: column; gap: 3px; padding: 5px 0 2px; }
</style>
