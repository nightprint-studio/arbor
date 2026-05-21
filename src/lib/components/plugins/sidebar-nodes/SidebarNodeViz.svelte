<!--
  SidebarNodeViz — visualisation primitives:
    · sparkline   — inline mini line chart
    · chart       — full LineChart embed
    · state_graph — SVG state-machine graph (variants + transitions)

  All three are read-only views; the only interactive bits are the
  sparkline pin button and an optional whole-row click.
-->
<script lang="ts">
  import { Pin, PinOff } from 'lucide-svelte';
  import { tooltip } from '$lib/actions/tooltip';
  import LineChart, { type LineSeries } from '$lib/components/shared/charts/LineChart.svelte';
  import type { SidebarNodeCtx } from './ctx';
  import { pointsToXY, sparklinePath } from './helpers';

  interface Props {
    node: any;
    ctx:  SidebarNodeCtx;
  }
  let { node: n, ctx }: Props = $props();
</script>

{#if n.type === 'sparkline'}
  {@const skPts   = pointsToXY(n.points)}
  {@const skColor = (typeof n.color === 'string' && n.color) ? n.color : 'var(--accent)'}
  {@const skClick = typeof n.action === 'string' ? n.action : ''}
  {@const skLast  = skPts.length > 0 ? skPts[skPts.length - 1].y : null}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div class="node-sparkline" class:clickable={!!skClick}
       role={skClick ? 'button' : undefined}
       tabindex={skClick ? 0 : -1}
       onclick={skClick ? () => ctx.fireAction(skClick, n.payload ?? {}) : undefined}
       onkeydown={skClick ? (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); ctx.fireAction(skClick, n.payload ?? {}); } } : undefined}
  >
    <div class="sparkline-head">
      {#if n.pin_action}
        <button
          type="button"
          class="sparkline-pin"
          class:pinned={!!n.pinned}
          use:tooltip={n.pinned ? 'Unpin from charts' : 'Pin live chart'}
          onclick={(e) => { e.stopPropagation(); ctx.fireAction(n.pin_action, n.payload ?? {}); }}
        >
          {#if n.pinned}<PinOff size={11} />{:else}<Pin size={11} />{/if}
        </button>
      {/if}
      {#if n.label}
        <span class="sparkline-label">{n.label}</span>
      {/if}
      {#if n.value_label != null}
        <span class="sparkline-value">{n.value_label}</span>
      {:else if skLast != null}
        <span class="sparkline-value">{skLast.toLocaleString(undefined, { maximumFractionDigits: 3 })}</span>
      {/if}
    </div>
    <svg class="sparkline-svg" viewBox={`0 0 ${(n.width ?? 180)} ${(n.height ?? 28)}`}
         preserveAspectRatio="none" aria-hidden="true">
      {#if skPts.length >= 2}
        <path d={sparklinePath(skPts, n.width ?? 180, n.height ?? 28)}
              fill="none" stroke={skColor} stroke-width="1.4" stroke-linejoin="round" />
      {:else}
        <text x="50%" y="55%" text-anchor="middle" font-size="9"
              fill="var(--text-muted)">{skPts.length === 1 ? 'collecting…' : 'no data'}</text>
      {/if}
    </svg>
  </div>

{:else if n.type === 'chart'}
  {@const chSeries = (Array.isArray(n.series) ? n.series : []) as Array<any>}
  {@const chLines = chSeries.map((s: any, k: number): LineSeries => ({
    id:    String(s.id ?? `series-${k}`),
    label: String(s.label ?? s.id ?? `series ${k + 1}`),
    color: String(s.color ?? ['#4ea1ff','#f59e0b','#22c55e','#ef4444','#a855f7','#10b981'][k % 6]),
    points: pointsToXY(s.points),
  }))}
  <div class="node-chart">
    {#if n.title}
      <div class="chart-title">{n.title}</div>
    {/if}
    <LineChart
      series={chLines}
      height={Number(n.height) > 0 ? Number(n.height) : 200}
      showLegend={chLines.length > 1 && n.show_legend !== false}
      xAxis={{ kind: 'time' }}
    />
  </div>

{:else if n.type === 'state_graph'}
  {@const sgVars = (Array.isArray(n.variants) ? n.variants : []) as string[]}
  {@const sgTrans = (Array.isArray(n.transitions) ? n.transitions : []) as Array<any>}
  {@const sgCur = typeof n.current === 'string' ? n.current : null}
  {@const sgPending = typeof n.pending === 'string' ? n.pending : null}
  {@const sgW = 320}
  {@const sgH = Math.max(180, 70 + sgVars.length * 22)}
  {@const sgCx = sgW / 2}
  {@const sgCy = sgH / 2}
  {@const sgR  = Math.max(50, Math.min(sgW, sgH) * 0.36)}
  {@const sgPositions = (() => {
    const map = new Map<string, { x: number; y: number; angle: number }>();
    const n2 = sgVars.length;
    if (n2 === 0) return map;
    if (n2 === 1) {
      map.set(sgVars[0], { x: sgCx, y: sgCy, angle: 0 });
      return map;
    }
    for (let k = 0; k < n2; k++) {
      const ang = -Math.PI / 2 + (k / n2) * Math.PI * 2;
      map.set(sgVars[k], { x: sgCx + Math.cos(ang) * sgR, y: sgCy + Math.sin(ang) * sgR, angle: ang });
    }
    return map;
  })()}
  <div class="node-state-graph">
    {#if n.title}
      <div class="chart-title">{n.title}</div>
    {/if}
    {#if sgVars.length === 0}
      <div class="panel-empty">No variants to graph.</div>
    {:else}
      <svg viewBox={`0 0 ${sgW} ${sgH}`} class="state-graph-svg" aria-hidden="true">
        <defs>
          <marker id="sg-arrow" viewBox="0 0 8 8" refX="7" refY="4"
                  markerWidth="6" markerHeight="6" orient="auto-start-reverse">
            <path d="M0 0 L8 4 L0 8 Z" fill="var(--text-muted)" />
          </marker>
          <marker id="sg-arrow-recent" viewBox="0 0 8 8" refX="7" refY="4"
                  markerWidth="6" markerHeight="6" orient="auto-start-reverse">
            <path d="M0 0 L8 4 L0 8 Z" fill="var(--accent)" />
          </marker>
        </defs>
        {#each sgTrans as t, ti (`${t.from}->${t.to}:${ti}`)}
          {@const pf = sgPositions.get(String(t.from))}
          {@const pt = sgPositions.get(String(t.to))}
          {#if pf && pt}
            {@const dx = pt.x - pf.x}
            {@const dy = pt.y - pf.y}
            {@const len = Math.max(1, Math.hypot(dx, dy))}
            {@const ux = dx / len}
            {@const uy = dy / len}
            {@const nx = -uy}
            {@const ny = ux}
            {@const curve = Math.min(40, len * 0.25)}
            {@const sx1 = pf.x + ux * 14}
            {@const sy1 = pf.y + uy * 14}
            {@const ex1 = pt.x - ux * 14}
            {@const ey1 = pt.y - uy * 14}
            {@const cx1 = (sx1 + ex1) / 2 + nx * curve}
            {@const cy1 = (sy1 + ey1) / 2 + ny * curve}
            <path
              d={`M${sx1.toFixed(1)} ${sy1.toFixed(1)} Q${cx1.toFixed(1)} ${cy1.toFixed(1)} ${ex1.toFixed(1)} ${ey1.toFixed(1)}`}
              fill="none"
              stroke={t.recent ? 'var(--accent)' : 'var(--text-muted)'}
              stroke-width={t.recent ? 1.6 : 1.0}
              stroke-opacity={t.recent ? 0.95 : 0.55}
              marker-end={`url(#${t.recent ? 'sg-arrow-recent' : 'sg-arrow'})`}
            />
            {#if t.count != null && Number(t.count) > 0}
              <text x={cx1.toFixed(1)} y={(cy1 - 4).toFixed(1)} text-anchor="middle"
                    font-size="9" fill="var(--text-muted)">{t.count}×</text>
            {/if}
          {/if}
        {/each}
        {#each sgVars as v, vi (v + ':' + vi)}
          {@const pos = sgPositions.get(v)}
          {#if pos}
            {@const isCur = v === sgCur}
            {@const isPending = v === sgPending && !isCur}
            <g class="state-node" class:current={isCur} class:pending={isPending}>
              <circle cx={pos.x} cy={pos.y} r={isCur ? 13 : 10}
                      fill={isCur ? 'var(--accent)' : 'var(--bg-elevated)'}
                      stroke={isCur ? 'var(--accent)' : isPending ? 'var(--accent)' : 'var(--border-default)'}
                      stroke-width={isCur ? 2 : 1.2}
                      stroke-dasharray={isPending ? '3 3' : undefined} />
              <text x={pos.x} y={(pos.y + sgR * 0.45 + 14).toFixed(1)}
                    text-anchor="middle" font-size="11"
                    fill={isCur ? 'var(--text-primary)' : 'var(--text-secondary)'}
                    font-weight={isCur ? 600 : 400}>{v}</text>
            </g>
          {/if}
        {/each}
      </svg>
    {/if}
  </div>
{/if}
