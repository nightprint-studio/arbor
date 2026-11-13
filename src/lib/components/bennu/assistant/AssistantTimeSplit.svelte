<script lang="ts">
  /**
   * The line under a finished answer that says where its time went: the model's answers, the tools running,
   * and the cards waiting for the user. The question it answers is "is it the tools that are slow, or the
   * model?" — so the parts are shown apart, not only their sum.
   */
  import { Timer } from 'lucide-svelte';
  import { formatDuration, type TimeSplit } from '$lib/components/shared/internal/imber/imber-format';

  let { split }: { split: TimeSplit } = $props();

  const parts = $derived(
    [
      { label: 'model', ms: split.model, tone: 'model' },
      { label: 'tools', ms: split.tools, tone: 'tools' },
      { label: 'waiting for you', ms: split.waiting, tone: 'wait' },
    ].filter((p) => p.ms > 0),
  );
  const total = $derived(split.model + split.tools + split.waiting);
</script>

<div class="split" title="Time of the model's answers, of the tools running, and of the cards waiting for your answer">
  <Timer size={11} />
  <span class="total">{formatDuration(total)}</span>
  {#each parts as p (p.label)}
    <span class="part {p.tone}">{p.label} {formatDuration(p.ms)}</span>
  {/each}
</div>

<style>
  .split {
    display: flex; align-items: center; flex-wrap: wrap; gap: 4px 10px;
    font-size: var(--font-size-xs); color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .total { color: var(--text-secondary); }
  .part::before { content: ''; display: inline-block; width: 6px; height: 6px; margin-right: 5px; border-radius: 50%; vertical-align: 1px; }
  .model::before { background: var(--accent); }
  .tools::before { background: var(--info); }
  .wait::before { background: var(--warning); }
</style>
