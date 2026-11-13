<script lang="ts">
  /**
   * One answer of the model inside the tools fold: the time between the previous results going out and this
   * answer coming back whole — the part of a run spent by the model rather than by the tools. The first-token
   * time says how much of it was waiting for the model to start; the rest is writing.
   */
  import { Sparkles } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import { formatDuration, formatTokens } from './imber-format';
  import type { ModelStep } from './imber-types';

  let { step }: { step: ModelStep } = $props();

  const tip = $derived(
    step.firstMs === undefined
      ? 'How long the model took to answer'
      : `Started answering after ${formatDuration(step.firstMs)}, then wrote for ${formatDuration(Math.max(0, step.ms - step.firstMs))}`,
  );
</script>

<div class="step" use:tooltip={tip}>
  <span class="glyph"><Sparkles size={11} /></span>
  <span class="label">Model answered</span>
  <span class="detail">{formatTokens(step.outputTokens)} tokens out</span>
  <span class="time">{formatDuration(step.ms)}</span>
</div>

<style>
  .step {
    display: flex; align-items: center; gap: 8px;
    padding: 3px 8px; min-width: 0;
    font-size: var(--font-size-xs);
    color: var(--text-muted);
  }
  .glyph {
    display: flex; align-items: center; justify-content: center; flex-shrink: 0;
    width: 18px; height: 18px; border-radius: 5px;
    color: var(--accent); background: var(--accent-subtle);
  }
  .label { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .detail { flex-shrink: 0; font-variant-numeric: tabular-nums; }
  .time { flex-shrink: 0; min-width: 44px; text-align: right; color: var(--text-secondary); font-variant-numeric: tabular-nums; }
</style>
