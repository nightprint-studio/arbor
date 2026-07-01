<script lang="ts">
  /** Last 365-day commit heatmap — GitHub-style 52×7 calendar grid in SVG.
   *  @param days  Array of [YYYY-MM-DD, count] from RepoStats.commits_by_day.
   *               Only dates with ≥1 commit are expected; zeros are filled in. */
  let { days }: { days: [string, number][] } = $props();

  const CELL  = 11;
  const GAP   = 2;
  const STEP  = CELL + GAP;
  const WEEKS = 52;
  const LABEL_H  = 18;   // height reserved for month labels
  const LABEL_W  = 22;   // width reserved for day labels

  // ── Reactive data ───────────────────────────────────────────────────────────

  const dateMap  = $derived(new Map<string, number>(days));
  const maxCount = $derived(days.length > 0 ? Math.max(...days.map(d => d[1])) : 1);

  interface Cell { date: string; count: number; col: number; row: number; }

  function buildGrid(): Cell[] {
    const cells: Cell[] = [];
    const today = new Date();
    today.setHours(0, 0, 0, 0);
    // Start 52 weeks back, aligned to Monday (getDay: 0=Sun … 6=Sat)
    const start = new Date(today.getTime() - WEEKS * 7 * 86_400_000);
    const dow   = start.getDay();
    const shift = dow === 0 ? 1 : (1 - dow);   // days to add to land on Monday
    start.setDate(start.getDate() + shift);

    for (let col = 0; col < WEEKS; col++) {
      for (let row = 0; row < 7; row++) {
        const d   = new Date(start.getTime() + (col * 7 + row) * 86_400_000);
        if (d > today) continue;
        const iso = d.toISOString().slice(0, 10);
        cells.push({ date: iso, count: dateMap.get(iso) ?? 0, col, row });
      }
    }
    return cells;
  }

  function buildMonthLabels(grid: Cell[]): Array<{ label: string; col: number }> {
    const labels: Array<{ label: string; col: number }> = [];
    let lastMonth = '';
    for (const c of grid) {
      if (c.row !== 0) continue;
      const m = c.date.slice(0, 7);
      if (m === lastMonth) continue;
      lastMonth = m;
      const d = new Date(c.date + 'T12:00:00');
      labels.push({ label: d.toLocaleString('default', { month: 'short' }), col: c.col });
    }
    return labels;
  }

  const grid        = $derived(buildGrid());
  const monthLabels = $derived(buildMonthLabels(grid));

  function cellOpacity(count: number): number {
    if (count === 0) return 0;
    return 0.18 + (count / maxCount) * 0.82;
  }

  // ── Tooltip ─────────────────────────────────────────────────────────────────
  let tooltip = $state<{ x: number; y: number; date: string; count: number } | null>(null);

  function formatDate(iso: string): string {
    return new Date(iso + 'T12:00:00').toLocaleDateString('default', {
      weekday: 'short', year: 'numeric', month: 'short', day: 'numeric',
    });
  }

  // ── Dimensions ──────────────────────────────────────────────────────────────
  const svgW = LABEL_W + WEEKS * STEP + 2;
  const svgH = LABEL_H + 7 * STEP;
</script>

<div class="heatmap-card">
<div class="heatmap-wrap">
  <svg width="100%" height={svgH} viewBox="0 0 {svgW} {svgH}" preserveAspectRatio="none" role="img" aria-label="Commit activity heatmap">
    <!-- Month labels -->
    {#each monthLabels as { label, col }}
      <text x={LABEL_W + col * STEP} y={LABEL_H - 5} class="lbl-month">{label}</text>
    {/each}

    <!-- Day labels: Mon / Wed / Fri -->
    {#each [['M', 0], ['W', 2], ['F', 4]] as [ch, row]}
      <text
        x={LABEL_W - 4}
        y={LABEL_H + (row as number) * STEP + CELL}
        class="lbl-day"
        text-anchor="end"
      >{ch}</text>
    {/each}

    <!-- Cells -->
    {#each grid as cell (cell.date)}
      <rect
        role="img"
        aria-label="{cell.count} commits on {cell.date}"
        x={LABEL_W + cell.col * STEP}
        y={LABEL_H + cell.row * STEP}
        width={CELL}
        height={CELL}
        rx={2}
        class="cell"
        style="
          fill: {cell.count === 0 ? 'var(--bg-hover)' : 'var(--accent)'};
          fill-opacity: {cell.count === 0 ? 0.5 : cellOpacity(cell.count)};
          cursor: default;
        "
        onmouseenter={(e: MouseEvent) => {
          const r = (e.target as Element).getBoundingClientRect();
          tooltip = { x: r.left + r.width / 2, y: r.top - 8, date: cell.date, count: cell.count };
        }}
        onmouseleave={() => tooltip = null}
      />
    {/each}
  </svg>

  <!-- Tooltip (fixed-position so it escapes SVG stacking context) -->
  {#if tooltip}
    <div class="tooltip" style="left:{tooltip.x}px; top:{tooltip.y}px;">
      <span class="tt-date">{formatDate(tooltip.date)}</span>
      <span class="tt-count">{tooltip.count} commit{tooltip.count !== 1 ? 's' : ''}</span>
    </div>
  {/if}
</div>

<!-- Legend -->
<div class="legend" style="padding-left: 2px;">
  <span class="legend-label">Less</span>
  {#each [0, 0.2, 0.45, 0.7, 1.0] as op}
    <svg width={CELL} height={CELL}>
      <rect
        width={CELL} height={CELL} rx={2}
        style="fill: {op === 0 ? 'var(--bg-hover)' : 'var(--accent)'}; fill-opacity: {op === 0 ? 0.5 : op};"
      />
    </svg>
  {/each}
  <span class="legend-label">More</span>
</div>
</div>

<style>
  .heatmap-card {
    background: var(--bg-elevated);
    border: 1px solid var(--border-subtle);
    border-radius: var(--radius-md);
    padding: 14px 16px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    width: 100%;
  }
  .heatmap-wrap { position: relative; display: block; }

  .cell { transition: fill-opacity 0.1s; }
  .cell:hover { stroke: var(--accent); stroke-width: 1px; stroke-opacity: 0.6; }

  .lbl-month {
    font-family: var(--font-ui-sans);
    font-size: 10px;
    fill: var(--text-muted);
  }
  .lbl-day {
    font-family: var(--font-ui-sans);
    font-size: 9px;
    fill: var(--text-muted);
    dominant-baseline: middle;
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
  .tt-date  { font-size: 11px; color: var(--text-secondary); font-family: var(--font-ui-sans); }
  .tt-count { font-size: 12px; color: var(--text-primary);   font-family: var(--font-ui-sans); font-weight: 600; }

  .legend {
    display: flex;
    align-items: center;
    gap: 3px;
    margin-top: 6px;
  }
  .legend-label {
    font-size: 10px;
    color: var(--text-muted);
    font-family: var(--font-ui-sans);
    margin: 0 4px;
  }
</style>
