<script lang="ts">
  /**
   * The plan as a diagram — the shape of the execution, rather than a list of it.
   *
   * The indented list beside this is better at *reading* a plan: it holds every
   * detail line, it copies, it scans. What a list cannot do is show you the shape —
   * which branch is deep, where the rows multiply, which single node is the whole
   * cost. Those are the three questions people open a plan to answer, and all three
   * are geometric.
   *
   * So this borrows the one thing SSMS genuinely got right and leaves the rest:
   *
   *  • **Edge thickness is the row count** (log-scaled). The place where a thin line
   *    becomes a rope is where the query went wrong, and you find it without reading
   *    a number.
   *  • **A bar under each node is its share of the work**, computed from the numbers
   *    minus its children's — engines report costs inclusively, so without that
   *    subtraction every plan is one huge root and says nothing.
   *  • **Colour is reserved for a wrong estimate**, on the same thresholds the list
   *    badges, so a node the list calls bad is never drawn calm here.
   *
   * What it does not borrow is the right-to-left layout: rows do flow leaves-to-root,
   * but drawing them leaning left puts the answer where a left-to-right reader
   * finishes, and disagrees with the list one toggle away. Root on top, inputs below.
   *
   * ## Estimate and measurement are never blurred
   *
   * The one rule this whole panel is built on. An un-analysed plan draws the
   * planner's predictions, and every number it shows is prefixed `~`; an analysed one
   * draws what happened. The header says which, and so does the bar's own tooltip —
   * a diagram is *more* persuasive than a table, which makes mislabelling it worse.
   */
  import { tooltip } from '$lib/actions/tooltip';
  import ZoomControls from '$lib/components/shared/ui/ZoomControls.svelte';
  import { formatElapsed } from '$lib/stores/picus/query.svelte';
  import type { QueryPlan } from '$lib/ipc/picus/plan';
  import {
    edgePath, edgeWidth, formatCost, formatRows, layoutPlan, severityOf, deviation,
    NODE_H, NODE_W,
  } from './plan-graph';

  interface Props {
    plan: QueryPlan;
  }

  let { plan }: Props = $props();

  const graph = $derived(layoutPlan(plan));

  /** Which node's details are showing. Reset whenever the plan itself changes. */
  let selected = $state<number | null>(null);
  $effect(() => {
    void plan;
    selected = null;
  });

  const chosen = $derived(
    selected === null ? null : (graph.nodes.find((g) => g.index === selected) ?? null),
  );

  // ── Zoom ────────────────────────────────────────────────────────────────────
  // A plan of forty nodes is wider than any dock, so the diagram needs to be able to
  // step back. Scale rather than a re-layout: the geometry is in SVG units and the
  // viewport is CSS pixels, so zooming is one multiplication and nothing reflows.
  // The control (and the clamping) is `shared/ui/ZoomControls`.
  let zoom = $state(1);

  /** Padding around the drawing, so the outermost boxes are not flush to the edge. */
  const PAD = 16;

  function shareText(share: number): string {
    const pct = share * 100;
    if (pct >= 10) return `${Math.round(pct)}%`;
    return pct < 0.5 ? '<1%' : `${pct.toFixed(1)}%`;
  }
</script>

<div class="pg">
  <div class="pg-bar">
    <span class="pg-legend">
      <span class="pg-legend-rope" aria-hidden="true"></span>
      thicker edge = more rows
    </span>
    <span class="pg-legend">
      <span class="pg-legend-bar" aria-hidden="true"></span>
      {graph.measured ? 'share of measured time' : 'share of estimated cost'}
    </span>
    <span class="pg-spacer"></span>
    <!-- No "fit": the diagram scrolls, and a plan wide enough to need fitting would
         fit at a scale where the labels stop being readable — which is a picture of a
         plan rather than a plan. -->
    <ZoomControls
      value={zoom}
      min={0.4}
      max={1.6}
      step={0.1}
      ariaLabel="Zoom the plan diagram"
      onChange={(next) => (zoom = next)}
    />
  </div>

  <div class="pg-canvas">
    <svg
      width={(graph.width + PAD * 2) * zoom}
      height={(graph.height + PAD * 2) * zoom}
      viewBox={`${-PAD} ${-PAD} ${graph.width + PAD * 2} ${graph.height + PAD * 2}`}
      role="img"
      aria-label={`Execution plan, ${graph.nodes.length} nodes`}
    >
      <!-- Edges first, so a box always covers the line arriving at it. -->
      {#each graph.nodes as g (g.index)}
        {#if g.parent !== null}
          {@const owner = graph.nodes[g.parent]}
          <path
            class="pg-edge"
            d={edgePath(g, owner)}
            stroke-width={edgeWidth(g.outRows, graph.maxRows)}
          />
          <!-- The count on the edge, at the child's end where the rows leave. -->
          <text class="pg-edge-label" x={g.x + NODE_W / 2 + 6} y={g.y - 6}>
            {plan.analyzed ? '' : '~'}{formatRows(g.outRows)}
          </text>
        {/if}
      {/each}

      {#each graph.nodes as g (g.index)}
        {@const bad = severityOf(plan, g.node)}
        {@const off = deviation(plan, g.node)}
        <!-- A group per node rather than a foreignObject: focusable, keyboard-
             reachable in execution order (the list is pre-order), and it keeps the
             whole diagram one paint instead of N nested documents. -->
        <g
          class="pg-node pg-{bad}"
          class:pg-selected={selected === g.index}
          role="button"
          tabindex="0"
          aria-label={`${g.node.label}${g.node.relation ? ` on ${g.node.relation}` : ''}`}
          onclick={() => (selected = selected === g.index ? null : g.index)}
          onkeydown={(e) => {
            if (e.key !== 'Enter' && e.key !== ' ') return;
            e.preventDefault();
            selected = selected === g.index ? null : g.index;
          }}
        >
          <rect class="pg-box" x={g.x} y={g.y} width={NODE_W} height={NODE_H} rx="6" />
          <text class="pg-label" x={g.x + 10} y={g.y + 19}>{g.node.label}</text>
          {#if g.node.relation}
            <text class="pg-rel" x={g.x + 10} y={g.y + 33}>{g.node.relation}</text>
          {/if}

          <!-- The work bar: the full width is the whole plan, the filled part is this
               node's own share of it. -->
          <rect class="pg-track" x={g.x + 10} y={g.y + NODE_H - 16} width={NODE_W - 20} height="4" rx="2" />
          <rect
            class="pg-fill"
            x={g.x + 10}
            y={g.y + NODE_H - 16}
            width={Math.max(0, Math.min(1, g.share)) * (NODE_W - 20)}
            height="4"
            rx="2"
          />
          <text class="pg-share" x={g.x + NODE_W - 10} y={g.y + NODE_H - 20}>
            {shareText(g.share)}
          </text>
          {#if off !== null && bad !== 'none'}
            <text class="pg-off" x={g.x + NODE_W - 10} y={g.y + 19}>
              {off > 0 ? '↑' : '↓'}×{Math.round(Math.abs(off))}
            </text>
          {/if}
        </g>
      {/each}
    </svg>
  </div>

  <!-- The detail of the node you picked. Below the diagram rather than in a tooltip:
       the filter and sort keys are several lines of the server's own words, and a
       tooltip is not a place to read text you may want to compare. -->
  {#if chosen}
    <div class="pg-detail">
      <div class="pg-detail-head">
        <strong>{chosen.node.label}</strong>
        {#if chosen.node.relation}<span class="pg-detail-rel">on {chosen.node.relation}</span>{/if}
        <span class="pg-spacer"></span>
        <span class="pg-num" use:tooltip={'The planner’s estimate — rows out of this node, per loop.'}>
          ~{formatRows(chosen.node.rows)} rows
        </span>
        <span class="pg-num" use:tooltip={'Estimated total cost at this node, subtree included.'}>
          cost {formatCost(chosen.node.cost)}
        </span>
        {#if plan.analyzed}
          <span class="pg-num pg-actual" use:tooltip={'Rows this node really produced, per loop.'}>
            {formatRows(chosen.node.actualRows)} actual
          </span>
          {#if chosen.node.actualMs !== null}
            <span class="pg-num pg-actual">{formatElapsed(chosen.node.actualMs)}</span>
          {/if}
        {/if}
      </div>
      {#if chosen.node.warning}
        <p class="pg-warn">{chosen.node.warning}</p>
      {/if}
      {#if chosen.node.detail.length}
        <ul class="pg-detail-list">
          {#each chosen.node.detail as line, i (i)}<li>{line}</li>{/each}
        </ul>
      {/if}
    </div>
  {/if}
</div>

<style>
  .pg { display: flex; flex-direction: column; min-height: 0; flex: 1; }

  .pg-bar {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 5px 8px;
    border-bottom: 1px solid var(--border-subtle);
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    flex-shrink: 0;
  }
  .pg-spacer { flex: 1; }
  .pg-legend { display: inline-flex; align-items: center; gap: 5px; }
  .pg-legend-rope {
    width: 18px;
    height: 6px;
    border-radius: 3px;
    background: linear-gradient(90deg,
      color-mix(in srgb, var(--text-muted) 40%, transparent) 0%, var(--text-muted) 100%);
  }
  .pg-legend-bar {
    width: 18px;
    height: 4px;
    border-radius: 2px;
    background: linear-gradient(90deg, var(--accent) 55%, var(--bg-hover) 55%);
  }
  .pg-canvas { flex: 1; min-height: 0; overflow: auto; padding: 4px; }

  /* ── Edges ── */
  .pg-edge {
    fill: none;
    stroke: color-mix(in srgb, var(--text-muted) 55%, transparent);
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .pg-edge-label {
    font-family: var(--font-code);
    font-size: 9px;
    fill: var(--text-disabled);
  }

  /* ── Nodes ── */
  .pg-node { cursor: pointer; outline: none; }
  .pg-box {
    fill: var(--bg-elevated);
    stroke: var(--border);
    stroke-width: 1;
  }
  .pg-node:hover .pg-box { stroke: var(--accent); }
  /* Focus is drawn on the box, because an SVG group has no box of its own to outline. */
  .pg-node:focus-visible .pg-box { stroke: var(--border-focus); stroke-width: 2; }
  .pg-node.pg-selected .pg-box {
    stroke: var(--accent);
    stroke-width: 2;
    fill: var(--bg-hover);
  }
  /* Colour means one thing here: the planner's estimate was wrong by the same factor
     the list badges. Everything else stays neutral. */
  .pg-node.pg-warn .pg-box { stroke: var(--warning); }
  .pg-node.pg-bad .pg-box { stroke: var(--error); }

  .pg-label {
    font-family: var(--font-ui-sans);
    font-size: 11px;
    font-weight: 600;
    fill: var(--text-primary);
  }
  .pg-rel {
    font-family: var(--font-code);
    font-size: 10px;
    fill: var(--text-secondary);
  }
  .pg-share, .pg-off {
    font-family: var(--font-code);
    font-size: 9px;
    text-anchor: end;
    fill: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .pg-node.pg-warn .pg-off { fill: var(--warning); }
  .pg-node.pg-bad .pg-off { fill: var(--error); }

  .pg-track { fill: var(--bg-hover); }
  .pg-fill { fill: var(--accent); }

  /* ── The picked node ── */
  .pg-detail {
    flex-shrink: 0;
    max-height: 40%;
    overflow: auto;
    padding: 7px 10px;
    border-top: 1px solid var(--border);
    background: var(--bg-base);
    font-size: var(--font-size-xs);
  }
  .pg-detail-head { display: flex; align-items: center; gap: 8px; }
  .pg-detail-rel { font-family: var(--font-code); color: var(--text-secondary); }
  .pg-num {
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .pg-actual { color: var(--success); }
  .pg-warn { margin: 6px 0 0; color: var(--warning); }
  .pg-detail-list {
    margin: 6px 0 0;
    padding-left: 16px;
    font-family: var(--font-code);
    font-size: var(--font-size-2xs);
    color: var(--text-secondary);
  }
  .pg-detail-list li { margin: 1px 0; }
</style>
