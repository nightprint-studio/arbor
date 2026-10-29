<script lang="ts">
  /**
   * DownloadProgress — a progress bar with the line a person waiting for a download wants under it:
   * the percentage, how much has come and how much is left, how fast, and how long that means.
   *
   * App-agnostic: it only draws what {@link DownloadStats} says (see `utils/download-progress.ts`, which also
   * does the arithmetic), so a speech model, a tool and a sound bank all read the same way. The bar sweeps
   * (indeterminate) until the total is known.
   */
  import ProgressBar from './ProgressBar.svelte';
  import { describeProgress, type DownloadStats } from '$lib/utils/download-progress';

  let {
    stats,
    height = 4,
    ariaLabel = 'Download progress',
  }: {
    /** What the meter last said; `null` before the first reading (the bar sweeps, no line). */
    stats: DownloadStats | null;
    height?: number;
    ariaLabel?: string;
  } = $props();
</script>

<div class="dp">
  <ProgressBar pct={stats?.percent ?? undefined} indeterminate={stats?.percent == null} {height} {ariaLabel} />
  {#if stats}
    <span class="dp-line" role="status">{describeProgress(stats)}</span>
  {/if}
</div>

<style>
  .dp { display: flex; flex-direction: column; gap: 4px; min-width: 0; width: 100%; }
  .dp-line {
    font-size: var(--font-size-2xs); color: var(--text-secondary);
    font-variant-numeric: tabular-nums; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
</style>
