<script lang="ts">
  /**
   * What the colours of a profiler view mean: the three levels with the value each starts at.
   * Beside the table or the bar they colour, never in a manual — a colour with no key is
   * decoration, and the thresholds are the project's own, so only the panel knows them.
   */
  import type { SeverityLevels } from '$lib/ipc/bennu/profiler';
  import { severityColor } from './profiler-filter';

  interface Props {
    levels: SeverityLevels;
    /** Spells a level's value (`5%`, `250 ms`). */
    format?: (value: number) => string;
    /** What is being rated, in a few words (`self time`). */
    label?: string;
  }

  let { levels, format = (v) => `${v}%`, label }: Props = $props();

  const steps = $derived([
    { color: severityColor(1), at: levels.yellow, name: 'elevated' },
    { color: severityColor(2), at: levels.orange, name: 'high' },
    { color: severityColor(3), at: levels.red, name: 'critical' },
  ]);
</script>

<span class="legend" aria-label={label ? `Colour scale for ${label}` : 'Colour scale'}>
  {#if label}<span class="what">{label}</span>{/if}
  {#each steps as s (s.name)}
    <span class="step" title={`${s.name}: ${format(s.at)} or more`}>
      <span class="dot" style:background={s.color}></span>≥ {format(s.at)}
    </span>
  {/each}
</span>

<style>
  .legend { display: inline-flex; align-items: center; gap: 10px; font-size: var(--font-size-xs); color: var(--text-muted); }
  .what { color: var(--text-secondary); }
  .step { display: inline-flex; align-items: center; gap: 4px; white-space: nowrap; }
  .dot { width: 8px; height: 8px; border-radius: 50%; }
</style>
