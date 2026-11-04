<script lang="ts">
  /**
   * Sparkline — a small line graph of a series, with an optional reference line (a frame budget, a
   * limit). It fills its container's width and has the height you give it; colours come from the
   * theme tokens, so it follows light and dark. App-agnostic: it knows numbers, not what they mean.
   */
  import { sparkGeometry } from './sparkline';

  let {
    values,
    height = 56,
    reference,
    referenceLabel,
    ariaLabel,
  }: {
    values: readonly number[];
    height?: number;
    /** A level drawn as a dashed line, and included in the scale. */
    reference?: number;
    referenceLabel?: string;
    ariaLabel: string;
  } = $props();

  const WIDTH = 240;
  const geo = $derived(sparkGeometry(values, WIDTH, height, reference));
</script>

<svg class="sp" viewBox="0 0 {WIDTH} {height}" preserveAspectRatio="none" role="img" aria-label={ariaLabel} style="height:{height}px">
  {#if geo.referenceY !== null}
    <line class="sp-ref" x1="0" x2={WIDTH} y1={geo.referenceY} y2={geo.referenceY}>
      {#if referenceLabel}<title>{referenceLabel}</title>{/if}
    </line>
  {/if}
  {#if geo.points}
    <polyline class="sp-line" points={geo.points} />
  {/if}
</svg>

<style>
  .sp { display: block; width: 100%; background: var(--bg-overlay); border: 1px solid var(--border-subtle); border-radius: var(--radius-md); }
  .sp-line { fill: none; stroke: var(--accent); stroke-width: 1.5; vector-effect: non-scaling-stroke; }
  .sp-ref { stroke: var(--text-muted); stroke-width: 1; stroke-dasharray: 3 3; vector-effect: non-scaling-stroke; }
</style>
