<script lang="ts">
  /** Two small bar charts: commits by hour of day + commits by weekday. */
  let {
    byHour,     // len=24
    byWeekday,  // len=7, 0=Mon…6=Sun
  }: {
    byHour: number[];
    byWeekday: number[];
  } = $props();

  const HOUR_LABELS    = ['0','','2','','4','','6','','8','','10','','12','','14','','16','','18','','20','','22',''];
  const WEEKDAY_LABELS = ['Mon','Tue','Wed','Thu','Fri','Sat','Sun'];

  // ── Bar chart helper ────────────────────────────────────────────────────────
  interface BarDatum { label: string; value: number; }

  interface ChartConfig {
    data: BarDatum[];
    width: number;
    height: number;
    barColor?: string;
  }

  function barPath(cfg: ChartConfig) {
    const { data, width, height } = cfg;
    const n   = data.length;
    if (n === 0) return { bars: [], max: 1 };
    const max = Math.max(1, ...data.map(d => d.value));
    const gap = 1;
    const bw  = (width - gap * (n - 1)) / n;
    const bars = data.map((d, i) => ({
      x: i * (bw + gap),
      y: height - (d.value / max) * height,
      w: Math.max(1, bw),
      h: (d.value / max) * height,
      label: d.label,
      value: d.value,
    }));
    return { bars, max };
  }

  // ── Tooltip ─────────────────────────────────────────────────────────────────
  let tooltip = $state<{ x: number; y: number; label: string; value: number } | null>(null);

  // ── Hour chart ──────────────────────────────────────────────────────────────
  const HOUR_W = 360;
  const HOUR_H = 56;

  const hourData = $derived(byHour.map((v, i) => ({ label: HOUR_LABELS[i] ?? '', value: v })));
  const hourBars = $derived(barPath({ data: hourData, width: HOUR_W, height: HOUR_H }));

  // ── Weekday chart ───────────────────────────────────────────────────────────
  const WD_W = 200;
  const WD_H = 56;

  const wdData = $derived(byWeekday.map((v, i) => ({ label: WEEKDAY_LABELS[i], value: v })));
  const wdBars = $derived(barPath({ data: wdData, width: WD_W, height: WD_H }));
</script>

<div class="time-dist">
  <!-- Commits by Hour -->
  <div class="chart-card">
    <div class="chart-title">By Hour of Day</div>
    <div class="chart-wrap" style="position:relative;">
      <svg width="100%" height={HOUR_H + 16} viewBox="0 0 {HOUR_W} {HOUR_H + 16}" preserveAspectRatio="none" role="img" aria-label="Commits by hour">
        {#each hourBars.bars as bar}
          <rect x={bar.x} y={bar.y} width={bar.w} height={bar.h} rx={1} class="bar" />
        {/each}
        <!-- baseline -->
        <line x1={0} y1={HOUR_H} x2={HOUR_W} y2={HOUR_H} class="baseline" />
        <!-- x-axis labels every 4 hours -->
        {#each hourBars.bars as bar, i}
          {#if i % 4 === 0}
            <text x={bar.x + bar.w / 2} y={HOUR_H + 13} class="axis-lbl" text-anchor="middle">{i}</text>
          {/if}
        {/each}
        <!-- Full-height invisible hit areas — ensures hover works even on tiny bars -->
        {#each hourBars.bars as bar}
          <rect
            role="img"
            aria-label="{bar.value} commits at {bar.label}:00"
            x={bar.x} y={0} width={bar.w} height={HOUR_H}
            fill="transparent"
            style="cursor:default;"
            onmouseenter={(e: MouseEvent) => {
              const r = (e.target as Element).getBoundingClientRect();
              tooltip = { x: r.left + r.width/2, y: r.top - 8, label: `${bar.label}:00`, value: bar.value };
            }}
            onmouseleave={() => tooltip = null}
          />
        {/each}
      </svg>
    </div>
  </div>

  <!-- Commits by Weekday -->
  <div class="chart-card">
    <div class="chart-title">By Day of Week</div>
    <div class="chart-wrap">
      <svg width="100%" height={WD_H + 16} viewBox="0 0 {WD_W} {WD_H + 16}" preserveAspectRatio="none" role="img" aria-label="Commits by weekday">
        {#each wdBars.bars as bar}
          <rect x={bar.x} y={bar.y} width={bar.w} height={bar.h} rx={1} class="bar" />
        {/each}
        {#each wdBars.bars as bar}
          <text x={bar.x + bar.w / 2} y={WD_H + 13} class="axis-lbl" text-anchor="middle">{bar.label.slice(0, 2)}</text>
        {/each}
        <!-- Full-height invisible hit areas -->
        {#each wdBars.bars as bar}
          <rect
            role="img"
            aria-label="{bar.value} commits on {bar.label}"
            x={bar.x} y={0} width={bar.w} height={WD_H}
            fill="transparent"
            style="cursor:default;"
            onmouseenter={(e: MouseEvent) => {
              const r = (e.target as Element).getBoundingClientRect();
              tooltip = { x: r.left + r.width/2, y: r.top - 8, label: bar.label, value: bar.value };
            }}
            onmouseleave={() => tooltip = null}
          />
        {/each}
      </svg>
    </div>
  </div>
</div>

{#if tooltip}
  <div class="tooltip" style="left:{tooltip.x}px; top:{tooltip.y}px;">
    <span class="tt-label">{tooltip.label}</span>
    <span class="tt-value">{tooltip.value.toLocaleString()} commits</span>
  </div>
{/if}

<style>
  .time-dist {
    display: flex;
    gap: 32px;
    flex-wrap: wrap;
    align-items: flex-start;
  }

  .chart-card {
    flex: 1;
    min-width: 0;
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 12px 14px 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .chart-title {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
  }

  .bar {
    fill: var(--accent);
    fill-opacity: 0.7;
    cursor: default;
    transition: fill-opacity 0.1s;
  }
  .bar:hover { fill-opacity: 1; }

  .baseline {
    stroke: var(--border);
    stroke-width: 1;
  }

  .axis-lbl {
    font-size: 9px;
    fill: var(--text-muted);
    font-family: var(--font-ui-sans);
  }

  .tooltip {
    position: fixed;
    transform: translateX(-50%) translateY(-100%);
    background: var(--bg-overlay);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 5px 9px;
    pointer-events: none;
    z-index: 9999;
    display: flex;
    flex-direction: column;
    gap: 2px;
    box-shadow: 0 4px 16px rgba(0,0,0,0.4);
    white-space: nowrap;
  }
  .tt-label { font-size: 11px; color: var(--text-secondary); font-family: var(--font-ui-sans); }
  .tt-value { font-size: 12px; color: var(--text-primary); font-family: var(--font-ui-sans); font-weight: 600; }
</style>
