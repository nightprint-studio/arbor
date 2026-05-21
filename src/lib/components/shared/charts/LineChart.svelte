<script lang="ts">
  /**
   * Multi-series SVG line chart — generic, props-driven.
   *
   * The chart renders one polyline per series, a y-axis with auto-picked
   * "nice" ticks, an x-axis (linear or time-aware), an optional legend
   * with toggle-to-hide, and a hover guide that surfaces a tooltip with
   * per-series values aligned to the X position under the cursor.
   *
   * Domain coupling: zero. The security dashboard's `<VulnTimeSeriesChart>`
   * is a thin wrapper that shapes severity buckets into `LineSeries` and
   * delegates everything else here. CI metrics, plugin telemetry, etc.
   * can do the same.
   */
  import type { Snippet } from 'svelte';
  import {
    niceTicks, scaleLinear, formatTimeAxis,
    pathFromPoints, xToNumber, type XYPoint,
  } from './chart-utils';
  import ChartTooltip from './ChartTooltip.svelte';
  import ChartLegend  from './ChartLegend.svelte';

  export interface LineSeries {
    id:     string;
    label:  string;
    color:  string;
    points: XYPoint[];
  }

  interface XAxisOpts {
    kind?:      'time' | 'linear';
    formatter?: (v: number) => string;
    min?:       number;
    max?:       number;
  }
  interface YAxisOpts {
    formatter?: (v: number) => string;
    min?:       number;
    max?:       number;
    /** Force inclusion of zero on the axis. Default true. */
    includeZero?: boolean;
  }

  interface Props {
    series:            LineSeries[];
    xAxis?:            XAxisOpts;
    yAxis?:            YAxisOpts;
    height?:           number;
    showLegend?:       boolean;
    legendInteractive?: boolean;
    showHoverGuide?:   boolean;
    /** Snippet rendered when ALL series are empty. */
    emptyState?:       Snippet;
    /** Optional padding override (top/right/bottom/left). */
    padding?:          { top?: number, right?: number, bottom?: number, left?: number };
  }

  let {
    series,
    xAxis  = { kind: 'time' },
    yAxis  = {},
    height = 240,
    showLegend = true,
    legendInteractive = true,
    showHoverGuide = true,
    emptyState,
    padding,
  }: Props = $props();

  const PAD = $derived({
    top:    padding?.top    ?? 14,
    right:  padding?.right  ?? 18,
    bottom: padding?.bottom ?? 28,
    left:   padding?.left   ?? 40,
  });

  // ── Container width: responsive via ResizeObserver ─────────────────
  let wrap = $state<HTMLDivElement | null>(null);
  let width = $state(640);

  $effect(() => {
    if (!wrap) return;
    const ro = new ResizeObserver((entries) => {
      const w = entries[0]?.contentRect.width;
      if (w && w > 0) width = w;
    });
    ro.observe(wrap);
    return () => ro.disconnect();
  });

  // ── Hidden-series state owned by this component (toggle on legend) ─
  let hidden = $state<Set<string>>(new Set());
  function toggleSeries(id: string) {
    if (!legendInteractive) return;
    const next = new Set(hidden);
    if (next.has(id)) next.delete(id); else next.add(id);
    hidden = next;
  }

  // ── Visible series + empty detection ────────────────────────────────
  const visibleSeries = $derived(series.filter((s) => !hidden.has(s.id)));
  const hasAnyPoints  = $derived(series.some((s) => s.points.length > 0));

  // ── Domain ──────────────────────────────────────────────────────────
  const xExtent = $derived.by(() => {
    if (xAxis.min != null && xAxis.max != null) return [xAxis.min, xAxis.max] as const;
    let min = Infinity, max = -Infinity;
    for (const s of series) {
      for (const p of s.points) {
        const x = xToNumber(p.x);
        if (x < min) min = x;
        if (x > max) max = x;
      }
    }
    if (!isFinite(min) || !isFinite(max)) return [0, 1] as const;
    if (min === max) { min -= 1; max += 1; }
    return [xAxis.min ?? min, xAxis.max ?? max] as const;
  });

  const yExtent = $derived.by(() => {
    let min = yAxis.includeZero === false ? Infinity : 0;
    let max = -Infinity;
    for (const s of visibleSeries) {
      for (const p of s.points) {
        if (p.y < min) min = p.y;
        if (p.y > max) max = p.y;
      }
    }
    if (max === -Infinity) max = 1;
    if (min === Infinity)  min = 0;
    if (min === max) max = min + 1;
    return [yAxis.min ?? min, yAxis.max ?? max] as const;
  });

  // ── Scales ──────────────────────────────────────────────────────────
  const innerW = $derived(Math.max(0, width - PAD.left - PAD.right));
  const innerH = $derived(Math.max(0, height - PAD.top - PAD.bottom));

  const sx = $derived(scaleLinear(xExtent[0], xExtent[1], PAD.left, PAD.left + innerW));
  const sy = $derived(scaleLinear(yExtent[0], yExtent[1], PAD.top + innerH, PAD.top));

  const yTicks = $derived(niceTicks(yExtent[0], yExtent[1], 5));
  const xTicks = $derived(niceTicks(xExtent[0], xExtent[1], 5));

  const xFormatter = $derived(
    xAxis.formatter
      ?? (xAxis.kind === 'time' ? formatTimeAxis(xExtent[1] - xExtent[0]) : (v: number) => String(v)),
  );
  const yFormatter = $derived(yAxis.formatter ?? ((v: number) => String(v)));

  // ── Series paths ────────────────────────────────────────────────────
  const seriesPaths = $derived.by(() => {
    return series.map((s) => ({
      ...s,
      d: pathFromPoints(s.points.map((p) => ({ x: sx(xToNumber(p.x)), y: sy(p.y) }))),
      visible: !hidden.has(s.id),
    }));
  });

  // ── Hover guide + tooltip ───────────────────────────────────────────
  let hoverX = $state<number | null>(null);

  function onMove(ev: MouseEvent) {
    if (!showHoverGuide) return;
    const rect = (ev.currentTarget as SVGElement).getBoundingClientRect();
    const px   = ev.clientX - rect.left;
    if (px < PAD.left || px > PAD.left + innerW) { hoverX = null; return; }
    hoverX = px;
  }
  function onLeave() { hoverX = null; }

  // Closest x value (in data space) to the hovered pixel.
  const hoverDataX = $derived.by(() => {
    if (hoverX == null) return null;
    const t = (hoverX - PAD.left) / Math.max(1, innerW);
    return xExtent[0] + t * (xExtent[1] - xExtent[0]);
  });

  // For each visible series, find the nearest point to `hoverDataX`.
  const hoverHits = $derived.by(() => {
    if (hoverDataX == null) return [];
    return visibleSeries.map((s) => {
      let best: XYPoint | null = null;
      let bestD = Infinity;
      for (const p of s.points) {
        const d = Math.abs(xToNumber(p.x) - hoverDataX);
        if (d < bestD) { bestD = d; best = p; }
      }
      return best ? { series: s, point: best } : null;
    }).filter((v): v is { series: LineSeries, point: XYPoint } => v != null);
  });

  // Snap the guide to the closest x value in the data, so the tooltip
  // aligns with where the dots actually are.
  const guideX = $derived.by(() => {
    if (hoverHits.length === 0 || hoverDataX == null) return hoverX;
    let bestX = hoverDataX;
    let bestD = Infinity;
    for (const h of hoverHits) {
      const x = xToNumber(h.point.x);
      const d = Math.abs(x - hoverDataX);
      if (d < bestD) { bestD = d; bestX = x; }
    }
    return sx(bestX);
  });

  const tooltipTitle = $derived.by(() => {
    if (hoverHits.length === 0) return '';
    const x = xToNumber(hoverHits[0].point.x);
    return xFormatter(x);
  });

  const tooltipRows = $derived.by(() => {
    return hoverHits.map((h) => ({
      id:    h.series.id,
      label: h.series.label,
      color: h.series.color,
      value: h.point.y,
    }));
  });

  const legendEntries = $derived(series.map((s) => ({ id: s.id, label: s.label, color: s.color })));
</script>

<div class="line-chart" bind:this={wrap}>
  <div class="chart-canvas" style:height="{height}px">
    {#if !hasAnyPoints && emptyState}
      {@render emptyState()}
    {:else}
      <svg
        width={width}
        height={height}
        viewBox="0 0 {width} {height}"
        role="img"
        onmousemove={onMove}
        onmouseleave={onLeave}
      >
        <!-- y grid + ticks -->
        {#each yTicks as t (t)}
          {@const y = sy(t)}
          <line x1={PAD.left} y1={y} x2={PAD.left + innerW} y2={y}
            stroke="var(--border-subtle)" stroke-dasharray="2 3" />
          <text x={PAD.left - 6} y={y + 3} text-anchor="end"
            fill="var(--text-muted)" font-size="10" font-family="var(--font-ui-sans)">
            {yFormatter(t)}
          </text>
        {/each}

        <!-- x ticks -->
        {#each xTicks as t (t)}
          {@const x = sx(t)}
          <line x1={x} y1={PAD.top + innerH} x2={x} y2={PAD.top + innerH + 4}
            stroke="var(--border-subtle)" />
          <text x={x} y={PAD.top + innerH + 16} text-anchor="middle"
            fill="var(--text-muted)" font-size="10" font-family="var(--font-ui-sans)">
            {xFormatter(t)}
          </text>
        {/each}

        <!-- axes -->
        <line x1={PAD.left} y1={PAD.top + innerH} x2={PAD.left + innerW} y2={PAD.top + innerH}
          stroke="var(--border)" />

        <!-- series -->
        {#each seriesPaths as s (s.id)}
          <path d={s.d} fill="none" stroke={s.color} stroke-width="2"
            stroke-linejoin="round" stroke-linecap="round"
            opacity={s.visible ? 1 : 0.0}
          />
        {/each}

        <!-- hover guide + dots -->
        {#if showHoverGuide && hoverX != null && guideX != null}
          <line x1={guideX} y1={PAD.top} x2={guideX} y2={PAD.top + innerH}
            stroke="var(--text-muted)" stroke-dasharray="3 3" />
          {#each hoverHits as h (h.series.id)}
            <circle
              cx={sx(xToNumber(h.point.x))} cy={sy(h.point.y)} r="3.5"
              fill="var(--bg-base)" stroke={h.series.color} stroke-width="2"
            />
          {/each}
        {/if}
      </svg>

      {#if showHoverGuide && hoverHits.length > 0 && guideX != null}
        <ChartTooltip
          title={tooltipTitle}
          rows={tooltipRows}
          x={guideX}
          y={PAD.top + innerH / 2}
          containerWidth={width}
        />
      {/if}
    {/if}
  </div>

  {#if showLegend && series.length > 0}
    <div class="chart-legend-wrap">
      <ChartLegend
        entries={legendEntries}
        hidden={hidden}
        onToggle={legendInteractive ? toggleSeries : undefined}
      />
    </div>
  {/if}
</div>

<style>
  .line-chart {
    display: flex;
    flex-direction: column;
    width: 100%;
    font-family: var(--font-ui-sans);
  }
  .chart-canvas {
    position: relative;
    width: 100%;
  }
  .chart-canvas svg { display: block; }
  .chart-legend-wrap {
    margin-top: 8px;
    padding: 0 4px;
  }
</style>
