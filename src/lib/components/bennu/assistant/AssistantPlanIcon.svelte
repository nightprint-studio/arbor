<script lang="ts">
  /**
   * The plan's own icon: a three-row checklist that fills as the plan goes. Each row stands for a third of the
   * steps — done rows take the success colour with a tick, the row the work is in takes the accent, the rest stay
   * hollow — so the icon is the progress, readable at 14px, rather than a generic list glyph beside a number.
   */
  let { done = 0, total = 0, active = false, size = 14 }: {
    done?: number;
    total?: number;
    /** A step is in progress: the first row not done is the one being worked on. */
    active?: boolean;
    size?: number;
  } = $props();

  const ROWS = [3.5, 8, 12.5];
  const finished = $derived(total > 0 && done >= total);
  // Rounded, so two steps of seven already show a row; the last row fills only when the plan is finished.
  const filled = $derived(finished ? ROWS.length : total > 0 ? Math.min(ROWS.length - 1, Math.round((done / total) * ROWS.length)) : 0);
  const rowState = (i: number): 'done' | 'now' | 'todo' =>
    finished || i < filled ? 'done' : active && i === filled ? 'now' : 'todo';
</script>

<svg width={size} height={size} viewBox="0 0 16 16" fill="none" aria-hidden="true">
  {#each ROWS as y, i (i)}
    {@const s = rowState(i)}
    {#if s === 'done'}
      <circle cx="3" cy={y} r="2.1" fill="var(--success)" />
      <path d="M2 {y} l0.8 0.8 l1.3 -1.5" stroke="var(--bg-elevated)" stroke-width="1" stroke-linecap="round" stroke-linejoin="round" />
      <line x1="7" y1={y} x2="14.5" y2={y} stroke="var(--success)" stroke-opacity="0.55" stroke-width="1.5" stroke-linecap="round" />
    {:else if s === 'now'}
      <circle cx="3" cy={y} r="2.1" fill="var(--accent)" />
      <line x1="7" y1={y} x2="14.5" y2={y} stroke="var(--accent)" stroke-width="1.5" stroke-linecap="round" />
    {:else}
      <circle cx="3" cy={y} r="1.6" stroke="currentColor" stroke-width="1.1" />
      <line x1="7" y1={y} x2="14.5" y2={y} stroke="currentColor" stroke-opacity="0.7" stroke-width="1.5" stroke-linecap="round" />
    {/if}
  {/each}
</svg>
