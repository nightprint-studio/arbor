<script lang="ts">
  /**
   * Floating tooltip used by chart widgets to display per-series values
   * at a hovered X position. Positioned absolutely inside the chart
   * container — the chart is responsible for the wrapping `position:
   * relative` element.
   *
   * Generic: knows nothing about the security domain. Pass a `title`
   * (typically the formatted X value), the visible `rows`, and the pixel
   * coordinates inside the chart container.
   */
  import type { Snippet } from 'svelte';

  interface TooltipRow {
    id:    string;
    label: string;
    color: string;
    value: string | number;
  }

  interface Props {
    title:   string;
    rows:    TooltipRow[];
    /** Pixel position relative to the chart container (top-left = 0,0). */
    x:       number;
    y:       number;
    /** Optional clamp width — tooltip will flip horizontally near the right edge. */
    containerWidth?:  number;
    /** Optional content slot rendered below `rows`. */
    footer?: Snippet;
  }

  let { title, rows, x, y, containerWidth, footer }: Props = $props();

  // Flip to the left of the cursor when we'd otherwise overflow.
  const TOOLTIP_W = 180;
  const flip = $derived(
    containerWidth != null && x + TOOLTIP_W + 16 > containerWidth,
  );
  const left = $derived(flip ? x - TOOLTIP_W - 12 : x + 12);
</script>

<div class="chart-tooltip" style:left="{left}px" style:top="{y}px" role="tooltip">
  <div class="tt-title">{title}</div>
  <ul class="tt-rows">
    {#each rows as row (row.id)}
      <li class="tt-row">
        <span class="tt-dot" style:background={row.color}></span>
        <span class="tt-label">{row.label}</span>
        <span class="tt-value">{row.value}</span>
      </li>
    {/each}
  </ul>
  {#if footer}
    <div class="tt-footer">{@render footer()}</div>
  {/if}
</div>

<style>
  .chart-tooltip {
    position: absolute;
    pointer-events: none;
    width: 180px;
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.45);
    padding: 8px 10px;
    font-family: var(--font-ui-sans);
    color: var(--text-primary);
    z-index: 10;
    transform: translateY(-50%);
  }
  .tt-title {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
    margin-bottom: 6px;
  }
  .tt-rows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  .tt-row {
    display: grid;
    grid-template-columns: 8px 1fr auto;
    align-items: center;
    gap: 6px;
    font-size: 11px;
  }
  .tt-dot {
    width: 8px;
    height: 8px;
    border-radius: 2px;
  }
  .tt-label { color: var(--text-secondary); }
  .tt-value {
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    color: var(--text-primary);
  }
  .tt-footer {
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px solid var(--border-subtle);
    font-size: 10px;
    color: var(--text-muted);
  }
</style>
